use crate::{interaction::ShelfTab, *};

impl Service {
    pub(crate) fn browse_shelf(
        &self,
        query: &str,
        limit: u32,
        after: Option<i64>,
    ) -> Result<serde_json::Value> {
        if !(1..=1000).contains(&limit) {
            return Err(ApiError::new("invalid_argument", "Expected limit 1..1000"));
        }
        if after.is_some_and(|id| id <= 0) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected positive cursor",
            ));
        }
        // Resolve live folder paths before search and pagination. All stored
        // arrivals remain addressable through Get; this is a shelf projection.
        let mut events = Vec::new();
        let mut cursor = None;
        loop {
            let page = storage::query_events(&self.db, "", 1000, cursor)?;
            let exhausted = page.len() < 1000;
            cursor = page.last().map(|event| event.id);
            events.extend(page);
            if exhausted {
                break;
            }
        }
        let live_root = self
            .settings
            .sharex_folder
            .as_ref()
            .and_then(|path| fs::canonicalize(path).ok());
        let identity_events: Vec<_> = events
            .iter()
            .filter(|event| {
                event.source != "sharex"
                    || live_root
                        .as_ref()
                        .is_some_and(|root| event.path.starts_with(root))
            })
            .cloned()
            .collect();
        let screenshot_copies =
            crate::screenshot_identity::known_clipboard_ids(&self.db, &identity_events)?;
        let downloads_root = if self.settings.downloads_enabled {
            crate::downloads::configured_folder(self.settings.downloads_folder.as_deref())
                .ok()
                .and_then(|p| p.canonicalize().ok())
        } else {
            None
        };
        let needle = query.to_lowercase();
        let mut paths = std::collections::HashSet::new();
        let selected: Vec<_> = events
            .into_iter()
            .filter_map(|mut event| {
                if is_sharex_clipboard(&event) || screenshot_copies.contains(&event.id) {
                    return None;
                }
                let download = event.source == "downloads";
                if download {
                    if !event.available
                        || !downloads_root
                            .as_ref()
                            .is_some_and(|root| event.path.starts_with(root))
                    {
                        return None;
                    }
                    if !paths.insert(event.path.to_string_lossy().to_lowercase()) {
                        return None;
                    }
                    event.origin.source_app = Some("Downloads".into());
                }
                let screenshot = event.source == "sharex";
                if screenshot {
                    let inside = live_root.as_ref().is_some_and(|root| {
                        fs::canonicalize(&event.path).is_ok_and(|path| path.starts_with(root))
                    });
                    if !inside {
                        return None;
                    }
                    // Scanner paths are canonical. Fold Windows casing for legacy
                    // references, retaining only the newest arrival for a path.
                    let key = event.path.to_string_lossy().to_lowercase();
                    if !paths.insert(key) || !event.available {
                        return None;
                    }
                    event.origin.source_app = Some("ShareX folder".into());
                }
                let in_tab = match self.interaction.tab {
                    ShelfTab::All => true,
                    ShelfTab::Images => is_image(&event) && !screenshot && !download,
                    ShelfTab::Screenshots => screenshot,
                    ShelfTab::Downloads => download,
                };
                (in_tab && after.is_none_or(|id| event.id < id) && matches_query(&event, &needle))
                    .then_some(event)
            })
            .take(limit as usize)
            .collect();
        let next = if selected.len() == limit as usize {
            selected.last().map(|event| event.id)
        } else {
            None
        };
        Ok(serde_json::json!({"events":selected,"next_cursor":next}))
    }

    pub(crate) fn scan_sharex(&mut self) -> Result<serde_json::Value> {
        let Some(folder) = self.settings.sharex_folder.clone() else {
            return Ok(serde_json::json!({"items":[],"scanned":false}));
        };
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS sharex_seen(path TEXT NOT NULL, hash TEXT NOT NULL, PRIMARY KEY(path,hash));")?;
        let paths = self.sharex_scanner.scan(&folder)?;
        let mut items = Vec::new();
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS sharex_availability(event_id INTEGER PRIMARY KEY, available INTEGER NOT NULL);")?;
        let statuses = {
            let mut statement = self
                .db
                .prepare("SELECT id,path FROM events WHERE source='sharex'")?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for (id, path) in statuses {
            let available = std::path::Path::new(&path).is_file();
            let previous = self
                .db
                .query_row(
                    "SELECT available FROM sharex_availability WHERE event_id=?1",
                    [id],
                    |row| row.get::<_, bool>(0),
                )
                .ok();
            self.db.execute(
                "INSERT OR REPLACE INTO sharex_availability(event_id,available) VALUES(?1,?2)",
                params![id, available],
            )?;
            if previous.is_some_and(|value| value != available)
                || (previous.is_none() && !available)
            {
                items.push(serde_json::json!({"ok":true,"availability_changed":true,"id":id}));
            }
        }
        for path in paths {
            let result = (|| -> Result<Option<serde_json::Value>> {
                let before = fs::metadata(&path)?;
                let hash = preview::content_hash(&path)?;
                let key = path.to_string_lossy().to_string();
                let seen: bool = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sharex_seen WHERE path=?1 AND hash=?2)",
                    params![key, hash],
                    |r| r.get(0),
                )?;
                if seen {
                    let latest: Option<String> = self.db.query_row(
                        "SELECT reference_hash FROM events WHERE source='sharex' AND path=?1 ORDER BY id DESC LIMIT 1",
                        [&key], |row| row.get(0),
                    ).ok().flatten();
                    // A deleted shelf event must not resurrect merely because
                    // its receipt remains. A newer revision reverting to old
                    // bytes, however, needs a fresh live event.
                    if latest.is_none_or(|latest| latest == hash) {
                        return Ok(None);
                    }
                }
                // Keep the durable receipt and event together, including failure
                // to read the event back. IngestPath uses the ordinary dispatcher.
                self.db.execute_batch("SAVEPOINT sharex_ingest")?;
                let ingest = (|| -> Result<serde_json::Value> {
                    let event = self.dispatch(Command::IngestPath {
                        path: path.clone(),
                        source: "sharex".into(),
                    })?;
                    let reference_hash: String = self.db.query_row(
                        "SELECT reference_hash FROM events WHERE id=?1",
                        params![event["id"].as_i64()],
                        |row| row.get(0),
                    )?;
                    let after = fs::metadata(&path)?;
                    if reference_hash != hash
                        || before.len() != after.len()
                        || before.modified()? != after.modified()?
                        || preview::content_hash(&path)? != hash
                    {
                        return Err(ApiError::new(
                            "source_changed",
                            "ShareX file changed during ingestion",
                        ));
                    }
                    self.db.execute(
                        "INSERT OR IGNORE INTO sharex_seen(path,hash) VALUES(?1,?2)",
                        params![key, hash],
                    )?;
                    self.db.execute_batch("RELEASE SAVEPOINT sharex_ingest")?;
                    Ok(event)
                })();
                if ingest.is_err() {
                    self.db.execute_batch(
                        "ROLLBACK TO SAVEPOINT sharex_ingest; RELEASE SAVEPOINT sharex_ingest",
                    )?;
                }
                ingest.map(Some)
            })();
            match result {
                Ok(Some(event)) => items.push(serde_json::json!({"ok":true,"event":event})),
                Ok(None) => {}
                Err(error) => {
                    self.sharex_scanner = Default::default();
                    items.push(serde_json::json!({"ok":false,"path":path,"error":error}));
                }
            }
        }
        Ok(serde_json::json!({"items":items,"scanned":true}))
    }
}

fn is_image(event: &Event) -> bool {
    event.name.rsplit_once('.').is_some_and(|(_, extension)| {
        matches!(
            extension.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
        )
    })
}
fn is_sharex_clipboard(event: &Event) -> bool {
    is_image(event)
        && matches!(
            event.source.as_str(),
            "clipboard" | "clipboard_image" | "clipboard_file"
        )
        && event
            .origin
            .source_app
            .as_deref()
            .is_some_and(|app| app.eq_ignore_ascii_case("ShareX"))
}
fn matches_query(event: &Event, query: &str) -> bool {
    [
        &event.name,
        &event.source,
        event.origin.source_app.as_deref().unwrap_or(""),
        event.origin.source_domain.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(query))
}

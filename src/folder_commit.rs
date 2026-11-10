use crate::*;

impl Service {
    pub(crate) fn commit_folder_candidates(
        &mut self,
        items: Vec<folder_watch::PreparedCandidate>,
    ) -> Result<serde_json::Value> {
        if items.len() > 8 {
            return Err(ApiError::new(
                "invalid_argument",
                "At most eight prepared files",
            ));
        }
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS folder_commits(source TEXT,path TEXT,hash TEXT,event_id INTEGER,PRIMARY KEY(source,path));")?;
        let mut results = Vec::new();
        for item in items {
            let result = (|| -> Result<Option<Event>> {
                let (source, root) = match item.source {
                    folder_watch::FolderSource::Sharex => (
                        "sharex",
                        self.settings.sharex_folder.clone().ok_or_else(|| {
                            ApiError::new("source_disabled", "ShareX not configured")
                        })?,
                    ),
                    folder_watch::FolderSource::Downloads => {
                        if !self.settings.downloads_enabled {
                            return Err(ApiError::new("source_disabled", "Downloads disabled"));
                        }
                        (
                            "downloads",
                            downloads::configured_folder(
                                self.settings.downloads_folder.as_deref(),
                            )?,
                        )
                    }
                };
                let root = root.canonicalize()?;
                let path = item.path.canonicalize()?;
                if root != item.root
                    || !path.starts_with(&root)
                    || path.starts_with(&self.root)
                    || item.content_hash.len() != 64
                    || !item.content_hash.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(ApiError::new(
                        "unsafe_source",
                        "Prepared source does not match configured folder",
                    ));
                }
                let metadata = fs::metadata(&path)?;
                if !metadata.is_file()
                    || metadata.len() != item.size
                    || metadata.modified()? != item.modified
                {
                    return Err(ApiError::new("source_changed", "Prepared source changed"));
                }
                let key = path.to_string_lossy();
                let previous: Option<(String, i64)> = self
                    .db
                    .query_row(
                        "SELECT hash,event_id FROM folder_commits WHERE source=?1 AND path=?2",
                        params![source, key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                if previous
                    .as_ref()
                    .is_some_and(|(hash, id)| hash == &item.content_hash && self.get(*id).is_ok())
                {
                    return Ok(None);
                }
                // Reuse only the latest matching arrival; content reverting to an older hash is a new revision.
                let existing: Option<i64> = self.db.query_row("SELECT id FROM events WHERE id=(SELECT MAX(id) FROM events WHERE source=?1 AND path=?2) AND reference_hash=?3",params![source,key,item.content_hash],|r|r.get(0)).optional()?;
                self.db.execute_batch("SAVEPOINT folder_commit")?;
                let write = (|| -> Result<i64> {
                    let id = if let Some(id) = existing {
                        id
                    } else {
                        let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                            ApiError::new("invalid_argument", "Non-Unicode filename")
                        })?;
                        let origin = origin::Origin {
                            source_app: Some(
                                if source == "sharex" {
                                    "ShareX folder"
                                } else {
                                    "Downloads"
                                }
                                .into(),
                            ),
                            source_domain: None,
                        };
                        self.db.execute("INSERT INTO events(detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash) VALUES(?1,?2,?3,?4,?5,NULL,?6,?7)",params![now_ms(),source,name,key,item.size,serde_json::to_string(&origin).unwrap(),item.content_hash])?;
                        self.db.last_insert_rowid()
                    };
                    self.db.execute(
                        "INSERT OR REPLACE INTO folder_commits VALUES(?1,?2,?3,?4)",
                        params![source, key, item.content_hash, id],
                    )?;
                    Ok(id)
                })();
                match write {
                    Ok(id) => {
                        self.db.execute_batch("RELEASE folder_commit")?;
                        Ok(Some(self.get(id)?))
                    }
                    Err(error) => {
                        self.db
                            .execute_batch("ROLLBACK TO folder_commit; RELEASE folder_commit")?;
                        Err(error)
                    }
                }
            })();
            match result {
                Ok(Some(event)) => results.push(serde_json::json!({"ok":true,"event":event})),
                Ok(None) => {}
                Err(error) => {
                    results.push(serde_json::json!({"ok":false,"path":item.path,"error":error}))
                }
            }
        }
        Ok(serde_json::json!({"scanned":true,"items":results}))
    }

    pub(crate) fn screenshot_match_inputs(&self) -> Result<serde_json::Value> {
        let Some(root) = self
            .settings
            .sharex_folder
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
        else {
            return Ok(serde_json::json!({"events":[]}));
        };
        let mut events = Vec::new();
        let mut cursor = None;
        loop {
            let page = storage::query_events(&self.db, "", 1000, cursor)?;
            let exhausted = page.len() < 1000;
            cursor = page.last().map(|e| e.id);
            events.extend(page.into_iter().filter(|e| {
                (e.source == "sharex" && e.path.starts_with(&root))
                    || matches!(e.source.as_str(), "clipboard_image" | "clipboard")
            }));
            if exhausted {
                break;
            }
        }
        Ok(serde_json::json!({"events":events}))
    }
}

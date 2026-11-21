//! Flat Obsidian vault for intentionally promoted Text cards and BULL archives.
use crate::{
    ApiError, Event, EventKind, Result, Service, collection, file_types, interaction::ShelfTab,
    open_file, storage, text_fragments, text_vault,
};
use baloh_collection::bs::{FileEntry, MAX_FILE_BYTES};
use pulldown_cmark::{Event as MarkdownEvent, Options, Parser, Tag};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub(crate) const ID_BASE: i64 = -(1 << 50);
pub(crate) const VAULT_NAME: &str = "Katran Pages";
pub(crate) fn is_id(id: i64) -> bool {
    (ID_BASE - (1 << 40)..ID_BASE).contains(&id)
}

pub(crate) fn is_document(path: &Path) -> bool {
    file_types::is_markdown(path)
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bull"))
}

pub(crate) fn vault(root: &Path) -> Result<PathBuf> {
    text_vault::directory(&root.canonicalize()?, VAULT_NAME, true)
}

// The Pages projection IDs last for one session. Store local metadata by vault
// filename so edits to the contents don't change the pin or touch Markdown.
pub(crate) fn ensure_pins(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS pages_pins(
            filename TEXT PRIMARY KEY NOT NULL,
            position INTEGER NOT NULL,
            slot_number INTEGER CHECK(slot_number BETWEEN 1 AND 9)
        );
        INSERT OR IGNORE INTO tag_scoped_slots
            SELECT 'pages',number,name,color,material,scale,intensity,angle,seed,opacity FROM tag_slots;",
    )?;
    Ok(())
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct Pin {
    pub(crate) position: i64,
    pub(crate) slot: Option<u8>,
}

fn pin_key(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
pub(crate) fn rename_metadata(db: &Connection, source: &Path, destination: &Path) -> Result<()> {
    let old = pin_key(source);
    let new = pin_key(destination);
    if old != new {
        db.execute("DELETE FROM pages_pins WHERE filename=?1", [&new])?;
        db.execute(
            "UPDATE pages_pins SET filename=?2 WHERE filename=?1",
            params![old, new],
        )?;
        db.execute(
            "UPDATE page_moves SET filename=?2 WHERE filename=?1",
            params![old, new],
        )?;
    }
    Ok(())
}

pub(crate) fn pin(db: &Connection, path: &Path) -> Result<Option<Pin>> {
    pin_by_name(db, &pin_key(path))
}

pub(crate) fn pin_by_name(db: &Connection, name: &str) -> Result<Option<Pin>> {
    Ok(db
        .prepare_cached("SELECT position,slot_number FROM pages_pins WHERE filename=?1")?
        .query_row([name], |row| {
            Ok(Pin {
                position: row.get(0)?,
                slot: row.get(1)?,
            })
        })
        .optional()?)
}

pub(crate) struct BrowsePins {
    pub(crate) entries: HashMap<String, Pin>,
    pub(crate) pinned_only: bool,
    pub(crate) slot: Option<u8>,
}

impl BrowsePins {
    pub(crate) fn get(&self, event: &Event) -> Option<Pin> {
        self.entries.get(&event.name).copied()
    }
}

fn filename(event: &Event, title: &str, extension: &str) -> String {
    let title: String = title
        .chars()
        .filter(|&ch| {
            !ch.is_control() && !matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
        .take(50)
        .collect();
    let title = title.trim().trim_end_matches('.');
    if title.is_empty() {
        format!("Page {}.{extension}", event.id)
    } else {
        format!("{} - {title}.{extension}", event.id)
    }
}

pub(crate) fn open_note(event: &Event, launcher: &dyn open_file::Launcher) -> Result<Value> {
    open_file::validate(event)?;
    launcher.authorize()?;
    text_vault::open_obsidian_path(&event.path, launcher)?;
    Ok(json!({"id":event.id,"obsidian_requested":true}))
}

fn read_entry(event: &Event) -> Result<FileEntry> {
    if event.kind == EventKind::TextFragment {
        let text = text_fragments::read_text_event(event)?;
        return Ok(FileEntry {
            name: filename(event, &text_fragments::title(&text), "md"),
            bytes: text.into_bytes(),
        });
    }
    if collection::is_text_collection(event) {
        open_file::validate(event)?;
        let mut bytes = Vec::new();
        fs::File::open(&event.path)?
            .take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(ApiError::new(
                "page_too_large",
                "BULL exceeds the move size limit",
            ));
        }
        open_file::validate(event)?;
        let title = Path::new(&event.name)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        return Ok(FileEntry {
            name: filename(event, title, "bull"),
            bytes,
        });
    }
    Err(ApiError::new(
        "not_text",
        "Choose Text snippets or text BULLs",
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    Media,
}

pub enum Section {
    Markdown(Vec<crate::notion_reader::Block>),
    Embed(String),
}

/// Project Obsidian embeds and Markdown images into blocks, leaving all other
/// formatting to the shared reader. Code spans and fenced code stay literal.
pub fn reader_sections(text: &str) -> Vec<Section> {
    let mut matches = Vec::new();
    let mut literal = Vec::new();
    for (event, range) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
        match event {
            MarkdownEvent::Start(Tag::Image { dest_url, .. }) => {
                matches.push((range, dest_url.into_string()));
            }
            MarkdownEvent::Code(_) | MarkdownEvent::Start(Tag::CodeBlock(_)) => literal.push(range),
            _ => {}
        }
    }
    for (start, _) in text.match_indices("![[") {
        if text[..start].ends_with('\\') || literal.iter().any(|range| range.contains(&start)) {
            continue;
        }
        let tail = &text[start + 3..];
        let Some(close) = tail.find("]]") else {
            continue;
        };
        let link = &tail[..close];
        if link.contains(['\n', '\r', '[', ']']) {
            continue;
        }
        let target = link.split('|').next().unwrap_or_default().trim();
        if !target.is_empty() {
            matches.push((start..start + 3 + close + 2, target.to_owned()));
        }
    }
    matches.sort_by_key(|(range, _)| range.start);
    let mut sections = Vec::new();
    let mut start = 0;
    for (range, target) in matches.into_iter().take(64) {
        if range.start < start {
            continue;
        }
        if let Ok(blocks) = crate::notion_reader::parse(&text[start..range.start]) {
            sections.push(Section::Markdown(blocks));
        }
        sections.push(Section::Embed(target));
        start = range.end;
    }
    if let Ok(blocks) = crate::notion_reader::parse(&text[start..]) {
        sections.push(Section::Markdown(blocks));
    }
    sections
}

pub fn attachment(note: &Path, target: &str) -> Result<(PathBuf, AttachmentKind)> {
    let root = note.parent().ok_or_else(invalid_attachment)?;
    let path = Path::new(target);
    if target.is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(invalid_attachment());
    }
    let mut checked = root.to_owned();
    for component in path.components() {
        checked.push(component);
        if fs::symlink_metadata(&checked)?.file_type().is_symlink()
            || checked.canonicalize()? != checked
        {
            return Err(invalid_attachment());
        }
    }
    let extension = checked
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let kind = match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => AttachmentKind::Image,
        "avif" | "svg" | "mp3" | "wav" | "ogg" | "m4a" | "opus" | "flac" | "mp4" | "webm"
        | "mov" | "mkv" | "pdf" => AttachmentKind::Media,
        _ => return Err(invalid_attachment()),
    };
    if !checked.is_file() {
        return Err(invalid_attachment());
    }
    Ok((checked, kind))
}

fn invalid_attachment() -> ApiError {
    ApiError::new(
        "invalid_page_attachment",
        "Expected a local file inside the Pages vault",
    )
}

impl Service {
    pub(crate) fn browse_pages(
        &self,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<Value> {
        self.browse_pages_with_pins(
            query,
            limit,
            cursor,
            self.interaction.sort,
            self.interaction.pinned_only,
        )
    }

    fn browse_pages_with_pins(
        &self,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
        sort: crate::sorting::SortSpec,
        pinned_only: bool,
    ) -> Result<Value> {
        let root = vault(&self.root)?;
        let mut pages = self.pages.borrow_mut();
        if pages.current() != Some(root.as_path()) {
            pages.choose(&root)?;
        }
        let mut statement = self
            .db
            .prepare("SELECT filename,position,slot_number FROM pages_pins")?;
        let entries = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    Pin {
                        position: row.get(1)?,
                        slot: row.get(2)?,
                    },
                ))
            })?
            .collect::<std::result::Result<_, _>>()?;
        pages.browse_with_pins(
            query,
            limit,
            cursor,
            sort,
            Some(&BrowsePins {
                entries,
                pinned_only,
                slot: self.interaction.tag_slot_filter,
            }),
        )
    }

    pub(crate) fn resolve_page_pins(&self, ids: &[i64]) -> Result<Vec<Event>> {
        if ids.len() > 1000 || ids.iter().any(|&id| !is_id(id)) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected at most 1000 Pages item IDs",
            ));
        }
        ids.iter().map(|&id| self.get(id)).collect()
    }

    pub(crate) fn toggle_page_pins(&mut self, ids: Vec<i64>) -> Result<Value> {
        let mut seen = HashSet::new();
        let ids: Vec<_> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
        if self.interaction.tab != ShelfTab::Pages || ids.is_empty() {
            return Err(ApiError::new("invalid_selection", "Select items in Pages"));
        }
        let events = self.resolve_page_pins(&ids)?;
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let mut any_unpinned = false;
        for event in &events {
            any_unpinned |= pin(&tx, &event.path)?.is_none();
        }
        let mut items = Vec::new();
        for event in events {
            let key = pin_key(&event.path);
            if any_unpinned {
                tx.execute(
                    "INSERT OR IGNORE INTO pages_pins(filename,position)
                     VALUES(?1,(SELECT COALESCE(MAX(position),0)+1 FROM pages_pins))",
                    [&key],
                )?;
            } else {
                tx.execute("DELETE FROM pages_pins WHERE filename=?1", [&key])?;
            }
            items.push(crate::pinned::ItemResult {
                id: event.id,
                pinned: Some(any_unpinned),
                error: None,
            });
        }
        tx.commit()?;
        self.interaction.view_generation = self.interaction.view_generation.wrapping_add(1);
        Ok(json!({"items":items}))
    }

    pub(crate) fn assign_page_tag(
        &self,
        ids: &[i64],
        slot: Option<u8>,
    ) -> Result<Vec<crate::tag_slots::ItemOutcome>> {
        use crate::tag_slots::{ItemOutcome, Outcome};
        if ids.is_empty() || slot.is_some_and(|n| !(1..=9).contains(&n)) {
            return Err(ApiError::new(
                "invalid_tag_slot",
                "Select Pages items and a slot in 1..9",
            ));
        }
        let events = self.resolve_page_pins(ids)?;
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        for event in events {
            let outcome = if !seen.insert(event.id) {
                Outcome::Duplicate
            } else if let Some(current) = pin(&tx, &event.path)? {
                let next = if current.slot == slot { None } else { slot };
                tx.execute(
                    "UPDATE pages_pins SET slot_number=?2 WHERE filename=?1",
                    params![pin_key(&event.path), next],
                )?;
                Outcome::Applied
            } else {
                Outcome::NotPinned
            };
            items.push(ItemOutcome {
                id: event.id,
                outcome,
            });
        }
        tx.commit()?;
        Ok(items)
    }

    pub(crate) fn move_page_pin(
        &self,
        id: i64,
        direction: crate::pinned_order::Direction,
    ) -> Result<crate::pinned_order::MoveResult> {
        use crate::{
            pinned_order::{Direction, MoveResult},
            sorting::{SortMode, SortSpec},
        };
        let sort = SortSpec {
            mode: SortMode::Pinned,
            reverse: false,
            seed: 0,
        };
        let mut ids = Vec::new();
        let mut cursor = None;
        loop {
            let page = self.browse_pages_with_pins(
                &self.interaction.browse_query,
                1000,
                cursor.as_deref(),
                sort,
                true,
            )?;
            ids.extend(
                page["events"]
                    .as_array()
                    .iter()
                    .flat_map(|events| events.iter())
                    .filter_map(|event| event["id"].as_i64()),
            );
            cursor = page["next_cursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        let index = ids
            .iter()
            .position(|&candidate| candidate == id)
            .ok_or_else(|| {
                ApiError::new("not_visible", "Pinned page is outside the current view")
            })?;
        let other = match direction {
            Direction::Up => index.checked_sub(1),
            Direction::Down => (index + 1 < ids.len()).then_some(index + 1),
        }
        .map_or(id, |i| ids[i]);
        let [event, neighbour] = [self.get(id)?, self.get(other)?];
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let rank = |event: &Event| {
            pin(&tx, &event.path)?
                .map(|pin| pin.position)
                .ok_or_else(|| ApiError::new("not_pinned", "Page is not pinned"))
        };
        let current = rank(&event)?;
        let target = rank(&neighbour)?;
        if id != other {
            tx.execute(
                "UPDATE pages_pins SET position=CASE WHEN filename=?1 THEN ?2 ELSE ?3 END WHERE filename IN (?1,?4)",
                params![pin_key(&event.path),target,current,pin_key(&neighbour.path)],
            )?;
        }
        tx.commit()?;
        Ok(MoveResult {
            moved: id != other,
            rank: target,
            generation: 0,
        })
    }

    pub(crate) fn pages_collection_available(&self) -> bool {
        let ids = if self.interaction.selected.is_empty() {
            self.interaction.cursor.as_slice()
        } else {
            self.interaction.selected.as_slice()
        };
        if let [id] = ids {
            return self
                .get(*id)
                .is_ok_and(|event| crate::collection_kind::external(&event));
        }
        (2..=baloh_collection::MAX_ENTRIES).contains(&ids.len())
            && ids.iter().all(|&id| is_id(id) && self.get(id).is_ok())
    }

    pub(crate) fn can_move_to_pages(&self) -> bool {
        let ids = if self.interaction.selected.is_empty() {
            self.interaction.cursor.as_slice()
        } else {
            self.interaction.selected.as_slice()
        };
        self.interaction.tab == ShelfTab::Text
            && self.settings.tab_enabled(ShelfTab::Pages)
            && !ids.is_empty()
            && ids.iter().all(|&id| {
                self.get(id).is_ok_and(|event| {
                    event.kind == EventKind::TextFragment || collection::is_text_collection(&event)
                })
            })
    }

    pub(crate) fn move_to_pages(&mut self) -> Result<Value> {
        if !self.can_move_to_pages() {
            return Err(ApiError::new(
                "invalid_selection",
                "Enable Pages and choose Text snippets or text BULLs",
            ));
        }
        let items = self.selected_refs()?;
        if items.len() > baloh_collection::MAX_ENTRIES {
            return Err(ApiError::new(
                "invalid_selection",
                "Too many selected pages",
            ));
        }
        let root = vault(&self.root)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut total = 0;
        let mut snippets_only = true;
        let mut entries = Vec::with_capacity(items.len());
        for item in &items {
            let event = storage::get_event(&tx, item.id)?;
            if event.revision != item.revision
                || !storage::visible_collection_input(&tx, item.id, ShelfTab::Text)?
            {
                return Err(ApiError::new(
                    "stale_selection",
                    "A selected Text card has changed",
                ));
            }
            snippets_only &= event.kind == EventKind::TextFragment;
            let entry = read_entry(&event)?;
            total += entry.bytes.len();
            if total > baloh_collection::bs::MAX_TOTAL_BYTES {
                return Err(ApiError::new(
                    "pages_size_limit",
                    "Selected items are too large to move at once",
                ));
            }
            entries.push(entry);
        }
        let merged = snippets_only && entries.len() > 1;
        if merged {
            let size = total + 2 * (entries.len() - 1);
            if size > text_fragments::MAX_TEXT_BYTES {
                return Err(ApiError::new(
                    "pages_size_limit",
                    "Combined Markdown exceeds the page size limit",
                ));
            }
            let name = entries[0].name.clone();
            let mut bytes = Vec::with_capacity(size);
            for (index, entry) in entries.into_iter().enumerate() {
                if index > 0 {
                    bytes.extend_from_slice(b"\n\n");
                }
                bytes.extend(entry.bytes);
            }
            entries = vec![FileEntry { name, bytes }];
        }
        let publication =
            crate::folder_collection::prepare_flat_extraction(&root, entries)?.publish();
        if let Some(error) = publication.error {
            return Err(ApiError::new(
                "pages_move_incomplete",
                format!(
                    "{} page files were written. All original Text cards are still available. {}",
                    publication.paths.len(),
                    error.message
                ),
            ));
        }
        for (index, item) in items.iter().enumerate() {
            let path = &publication.paths[if merged { 0 } else { index }];
            tx.execute(
                "INSERT INTO page_moves(event_id, filename) VALUES (?1, ?2)",
                rusqlite::params![
                    item.id,
                    path.file_name().unwrap_or_default().to_string_lossy(),
                ],
            )?;
        }
        tx.commit()?;
        let ids: Vec<_> = items.iter().map(|item| item.id).collect();
        self.interaction.replace_collection_rows(&ids, None);
        Ok(json!({
            "pages_moved":ids.len(),
            "pages_created":publication.paths.len(),
            "interaction":self.interaction
        }))
    }

    pub(crate) fn open_pages_vault(&self, in_obsidian: bool) -> Result<Value> {
        let root = vault(&self.root)?;
        let launcher = self
            .file_launcher
            .lock()
            .map_err(|_| ApiError::new("open_busy", "File launcher is busy"))?;
        launcher.authorize()?;
        if in_obsidian {
            text_vault::open_obsidian_path(&root, launcher.as_ref())?;
        } else {
            launcher.launch(&root, open_file::Operation::Open)?;
        }
        Ok(json!({"pages_vault_opened":true}))
    }

    pub(crate) fn open_page_attachment(&self, id: i64, target: &str) -> Result<Value> {
        if self.interaction.tab != ShelfTab::Pages
            || !is_id(id)
            || self.interaction.preview != Some(id)
        {
            return Err(invalid_attachment());
        }
        let note = self.get(id)?;
        let (path, _) = attachment(&note.path, target)?;
        let launcher = self
            .file_launcher
            .lock()
            .map_err(|_| ApiError::new("open_busy", "File launcher is busy"))?;
        launcher.authorize()?;
        launcher.launch(&path, open_file::Operation::Open)?;
        Ok(json!({"page_attachment_opened":true}))
    }
}

//! Read-only pixel identity for clipboard arrivals also observed in the ShareX folder.
//! Callers must include only ShareX arrivals belonging to the configured folder.
use crate::{Event, Result};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MatchPair {
    pub clipboard_id: i64,
    pub clipboard_revision: String,
    pub source_id: i64,
    pub source_revision: String,
}

/// Pure worker preparation: no database access, writes, or source mutations.
/// The caller supplies current-folder sources and schedules further batches.
/// At most 128 sources and 128 clipboard arrivals are decoded per call.
pub fn prepare_matches(events: &[Event]) -> Vec<MatchPair> {
    let mut pixels = HashMap::new();
    for source in events
        .iter()
        .filter(|e| e.source == "sharex" && e.available)
        .take(128)
    {
        if let Some(digest) = worker_digest(source) {
            pixels.entry(digest).or_insert(source);
        }
    }
    if pixels.is_empty() {
        return Vec::new();
    }
    events
        .iter()
        .filter(|e| e.available && matches!(e.source.as_str(), "clipboard_image" | "clipboard"))
        .take(128)
        .filter_map(|event| {
            let digest = worker_digest(event)?;
            let source = pixels.get(&digest)?;
            Some(MatchPair {
                clipboard_id: event.id,
                clipboard_revision: event.revision.clone(),
                source_id: source.id,
                source_revision: source.revision.clone(),
            })
        })
        .collect()
}

/// Commit prepared identities after checking arrivals still exist with the same revisions.
/// This performs SQL and event metadata checks only; it never decodes or hashes files.
pub fn commit_matches(db: &Connection, pairs: &[MatchPair]) -> Result<usize> {
    initialize(db)?;
    let mut changed = 0;
    for pair in pairs.iter().take(128) {
        let Ok(clipboard) = crate::storage::get_event(db, pair.clipboard_id) else {
            continue;
        };
        let Ok(source) = crate::storage::get_event(db, pair.source_id) else {
            continue;
        };
        if clipboard.revision != pair.clipboard_revision
            || source.revision != pair.source_revision
            || source.source != "sharex"
            || !matches!(clipboard.source.as_str(), "clipboard_image" | "clipboard")
        {
            continue;
        }
        changed += db.execute(
            "INSERT OR IGNORE INTO screenshot_clipboard_matches VALUES (?1,?2,?3,?4)",
            params![
                pair.clipboard_id,
                pair.clipboard_revision,
                pair.source_id,
                pair.source_revision
            ],
        )?;
    }
    Ok(changed)
}

fn initialize(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS screenshot_pixel_cache (
            event_id INTEGER NOT NULL, revision TEXT NOT NULL, metadata_revision TEXT NOT NULL,
            digest TEXT NOT NULL, PRIMARY KEY(event_id,revision,metadata_revision));
         CREATE TABLE IF NOT EXISTS screenshot_clipboard_matches (
            clipboard_id INTEGER NOT NULL, clipboard_revision TEXT NOT NULL,
            source_id INTEGER NOT NULL, source_revision TEXT NOT NULL,
            PRIMARY KEY(clipboard_id,clipboard_revision,source_id,source_revision));",
    )?;
    Ok(())
}

/// Read persisted classification without opening or decoding image files.
/// Sources must still be scoped to the current configured folder by the caller.
pub fn known_clipboard_ids(db: &Connection, events: &[Event]) -> Result<HashSet<i64>> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='screenshot_clipboard_matches')",
        [], |row| row.get(0),
    )?;
    if !exists {
        return Ok(HashSet::new());
    }
    let sources: HashSet<_> = events
        .iter()
        .filter(|e| e.source == "sharex")
        .map(|e| (e.id, e.revision.as_str()))
        .collect();
    let mut matched = HashSet::new();
    let mut statement = db.prepare("SELECT source_id,source_revision FROM screenshot_clipboard_matches WHERE clipboard_id=?1 AND clipboard_revision=?2")?;
    for event in events
        .iter()
        .filter(|e| matches!(e.source.as_str(), "clipboard_image" | "clipboard"))
    {
        let rows = statement.query_map(params![event.id, event.revision], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, revision) = row?;
            if sources.contains(&(id, revision.as_str())) {
                matched.insert(event.id);
                break;
            }
        }
    }
    Ok(matched)
}

/// Compute new pixel associations in the folder scan worker, never the UI browse path.
pub fn matched_clipboard_ids(db: &Connection, events: &[Event]) -> Result<HashSet<i64>> {
    initialize(db)?;
    let mut matched = known_clipboard_ids(db, events)?;
    let sources: HashMap<_, _> = events
        .iter()
        .filter(|e| e.source == "sharex")
        .map(|e| ((e.id, e.revision.as_str()), e))
        .collect();
    let mut pixels = HashMap::new();
    for source in sources.values().filter(|e| e.available) {
        if let Some(digest) = cached_digest(db, source)? {
            pixels.entry(digest).or_insert(*source);
        }
    }
    for event in events
        .iter()
        .filter(|e| matches!(e.source.as_str(), "clipboard_image" | "clipboard"))
    {
        if matched.contains(&event.id) {
            continue;
        }
        if !event.available || pixels.is_empty() {
            continue;
        }
        if let Some(digest) = cached_digest(db, event)?
            && let Some(source) = pixels.get(&digest)
        {
            db.execute(
                "INSERT OR IGNORE INTO screenshot_clipboard_matches VALUES (?1,?2,?3,?4)",
                params![event.id, event.revision, source.id, source.revision],
            )?;
            matched.insert(event.id);
        }
    }
    Ok(matched)
}

fn cached_digest(db: &Connection, event: &Event) -> Result<Option<String>> {
    let Ok(metadata) = std::fs::metadata(&event.path) else {
        return Ok(None);
    };
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Ok(None);
    }
    let current = crate::preview::revision(&event.path, None);
    let cached = db.query_row("SELECT digest FROM screenshot_pixel_cache WHERE event_id=?1 AND revision=?2 AND metadata_revision=?3",
        params![event.id,event.revision,current], |r| r.get::<_, String>(0)).optional()?;
    if let Some(digest) = cached {
        return Ok((!digest.is_empty()).then_some(digest));
    }
    let digest = decode_digest(event);
    // Do not cache a result obtained while a writer was changing the file.
    if current != crate::preview::revision(&event.path, None) {
        return Ok(None);
    }
    db.execute(
        "INSERT OR REPLACE INTO screenshot_pixel_cache VALUES (?1,?2,?3,?4)",
        params![
            event.id,
            event.revision,
            current,
            digest.as_deref().unwrap_or("")
        ],
    )?;
    Ok(digest)
}

// Only digest strings are cached; decoded images are released after each worker item.
fn worker_digest(event: &Event) -> Option<String> {
    type Cache = std::collections::VecDeque<(String, Option<String>)>;
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Cache>> = std::sync::OnceLock::new();
    let key = format!(
        "{}:{}:{}",
        event.path.display(),
        event.revision,
        crate::preview::revision(&event.path, None)
    );
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, value)) = cache.lock().ok()?.iter().find(|(stored, _)| *stored == key) {
        return value.clone();
    }
    let value = decode_digest(event);
    if let Ok(mut entries) = cache.lock() {
        if entries.len() >= 1024 {
            entries.pop_front();
        }
        entries.push_back((key, value.clone()));
    }
    value
}

fn decode_digest(event: &Event) -> Option<String> {
    // Read a bounded immutable snapshot; validate the arrival's recorded bytes before decoding.
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(&event.path)
        .ok()?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    if let Some(expected) = event
        .revision
        .strip_prefix("ref:")
        .or_else(|| event.revision.strip_prefix("blob:"))
        && format!("{:x}", Sha256::digest(&bytes)) != expected
    {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_BYTES);
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    if u64::from(decoded.width()) * u64::from(decoded.height()) > MAX_BYTES / 4 {
        return None;
    }
    let rgba = decoded.to_rgba8();
    let mut hash = Sha256::new();
    hash.update(rgba.width().to_le_bytes());
    hash.update(rgba.height().to_le_bytes());
    hash.update(rgba.as_raw());
    Some(format!("{:x}", hash.finalize()))
}

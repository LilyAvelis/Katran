//! Persistent metadata for shelf events and the Folder picker.
//! Event metadata belongs to an event revision, not to a filename or blob.
use crate::{ApiError, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

const MAX_NAME: usize = 512;
const MAX_TAG: usize = 128;
const MAX_TAGS: usize = 64;
const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Properties {
    pub pinned: bool,
    pub display_name: Option<String>,
    pub opener: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct ItemResult {
    pub id: i64,
    pub pinned: Option<bool>,
    pub error: Option<ApiError>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Tag {
    pub key: String,
    pub display: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FolderEntry {
    pub path: PathBuf,
    pub timestamp_ms: i64,
}

fn transaction(db: &Connection) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(
        db,
        TransactionBehavior::Immediate,
    )?)
}

fn event_exists(db: &Connection, id: i64) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1)",
        [id],
        |r| r.get(0),
    )?)
}

fn require_event(db: &Connection, id: i64) -> Result<()> {
    if !event_exists(db, id)? {
        return Err(ApiError::new(
            "not_found",
            format!("Shelf record {id} does not exist"),
        ));
    }
    Ok(())
}

fn require_pinned(db: &Connection, id: i64) -> Result<()> {
    if !properties(db, id)?.pinned {
        return Err(ApiError::new(
            "not_pinned",
            "Properties and tags can only be assigned to pinned records",
        ));
    }
    Ok(())
}

pub fn properties(db: &Connection, id: i64) -> Result<Properties> {
    require_event(db, id)?;
    Ok(db
        .query_row(
            "SELECT pinned,display_name,opener_path FROM katran_properties WHERE event_id=?1",
            [id],
            |r| {
                Ok(Properties {
                    pinned: r.get(0)?,
                    display_name: r.get(1)?,
                    opener: r.get::<_, Option<String>>(2)?.map(PathBuf::from),
                })
            },
        )
        .optional()?
        .unwrap_or_default())
}

/// Duplicate IDs are processed once, preserving their first occurrence order.
/// Missing IDs are reported separately and do not determine the batch direction.
pub fn toggle_pins(db: &Connection, ids: &[i64]) -> Result<Vec<ItemResult>> {
    let mut seen = HashSet::new();
    let ids: Vec<_> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    if ids.is_empty() {
        return Err(ApiError::new(
            "empty_selection",
            "No shelf records selected",
        ));
    }
    let tx = transaction(db)?;
    let mut states = Vec::with_capacity(ids.len());
    let mut any_unpinned = false;
    for &id in &ids {
        if event_exists(&tx, id)? {
            let pinned = properties(&tx, id)?.pinned;
            any_unpinned |= !pinned;
            states.push((id, true));
        } else {
            states.push((id, false));
        }
    }
    let mut result = Vec::with_capacity(states.len());
    for (id, exists) in states {
        if exists {
            tx.execute("INSERT INTO katran_properties(event_id,pinned) VALUES(?1,?2) ON CONFLICT(event_id) DO UPDATE SET pinned=excluded.pinned", params![id, any_unpinned])?;
            result.push(ItemResult {
                id,
                pinned: Some(any_unpinned),
                error: None,
            });
        } else {
            result.push(ItemResult {
                id,
                pinned: None,
                error: Some(ApiError::new(
                    "not_found",
                    format!("Shelf record {id} does not exist"),
                )),
            });
        }
    }
    tx.commit()?;
    Ok(result)
}

fn normalized(value: &str, max: usize, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(ApiError::new(
            "invalid_argument",
            format!("Invalid {field}"),
        ));
    }
    Ok(value.to_owned())
}

/// Replaces both optional properties; None or a blank name clears the value.
pub fn set_properties(
    db: &Connection,
    id: i64,
    display_name: Option<String>,
    opener: Option<PathBuf>,
) -> Result<Properties> {
    let display_name = match display_name {
        Some(v) if !v.trim().is_empty() => Some(normalized(&v, MAX_NAME, "display name")?),
        _ => None,
    };
    let opener = opener
        .map(|path| {
            let text = path.to_str().ok_or_else(|| {
                ApiError::new("invalid_argument", "Application path must be Unicode")
            })?;
            if text.is_empty() || text.chars().any(char::is_control) {
                return Err(ApiError::new(
                    "invalid_argument",
                    "Invalid application path",
                ));
            }
            Ok(text.to_owned())
        })
        .transpose()?;
    let tx = transaction(db)?;
    require_pinned(&tx, id)?;
    tx.execute(
        "UPDATE katran_properties SET display_name=?2,opener_path=?3 WHERE event_id=?1",
        params![id, display_name, opener],
    )?;
    let result = properties(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

pub fn tags(db: &Connection, id: i64) -> Result<Vec<Tag>> {
    require_event(db, id)?;
    let mut stmt = db.prepare(
        "SELECT tag_key,tag_display FROM katran_tags WHERE event_id=?1 ORDER BY tag_key",
    )?;
    let result = stmt
        .query_map([id], |r| {
            Ok(Tag {
                key: r.get(0)?,
                display: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(result)
}

/// Replaces the full set; keys are trimmed Unicode lowercase, display spelling is retained.
pub fn set_tags(db: &Connection, id: i64, values: &[String]) -> Result<Vec<Tag>> {
    if values.len() > MAX_TAGS {
        return Err(ApiError::new("invalid_argument", "Too many tags"));
    }
    let mut seen = HashSet::new();
    let mut new_tags = Vec::new();
    for value in values {
        let display = normalized(value, MAX_TAG, "tag")?;
        let key = display.to_lowercase();
        if seen.insert(key.clone()) {
            new_tags.push(Tag { key, display });
        }
    }
    let tx = transaction(db)?;
    require_pinned(&tx, id)?;
    tx.execute("DELETE FROM katran_tags WHERE event_id=?1", [id])?;
    for tag in &new_tags {
        tx.execute(
            "INSERT INTO katran_tags(event_id,tag_key,tag_display) VALUES(?1,?2,?3)",
            params![id, tag.key, tag.display],
        )?;
    }
    let result = tags(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

fn folder_key(path: &Path) -> Result<(String, String)> {
    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err(ApiError::new(
            "invalid_argument",
            "Folder must be an existing directory",
        ));
    }
    let text = path
        .to_str()
        .ok_or_else(|| ApiError::new("invalid_argument", "Folder path must be Unicode"))?
        .to_owned();
    let key = if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.clone()
    };
    Ok((key, text))
}

pub fn record_folder(db: &Connection, path: &Path) -> Result<FolderEntry> {
    let (key, path) = folder_key(path)?;
    let timestamp_ms = crate::now_ms();
    let tx = transaction(db)?;
    // Reinsert to make rowid a deterministic tiebreaker for same-millisecond opens.
    tx.execute("DELETE FROM folder_history WHERE path_key=?1", [&key])?;
    tx.execute(
        "INSERT INTO folder_history(path_key,path,opened_at_ms) VALUES(?1,?2,?3)",
        params![key, path, timestamp_ms],
    )?;
    tx.execute("DELETE FROM folder_history WHERE path_key IN (SELECT path_key FROM folder_history ORDER BY opened_at_ms DESC,rowid DESC LIMIT -1 OFFSET ?1)", [HISTORY_LIMIT as i64])?;
    tx.commit()?;
    Ok(FolderEntry {
        path: PathBuf::from(path),
        timestamp_ms,
    })
}

pub fn toggle_favorite(db: &Connection, path: &Path) -> Result<bool> {
    let (key, path) = folder_key(path)?;
    let tx = transaction(db)?;
    let removed = tx.execute("DELETE FROM folder_favorites WHERE path_key=?1", [&key])?;
    if removed == 0 {
        tx.execute(
            "INSERT INTO folder_favorites(path_key,path,added_at_ms) VALUES(?1,?2,?3)",
            params![key, path, crate::now_ms()],
        )?;
    }
    tx.commit()?;
    Ok(removed == 0)
}

pub fn folder_history(db: &Connection) -> Result<Vec<FolderEntry>> {
    let mut stmt = db.prepare(
        "SELECT path,opened_at_ms FROM folder_history ORDER BY opened_at_ms DESC,rowid DESC",
    )?;
    let result = stmt
        .query_map([], |r| {
            Ok(FolderEntry {
                path: PathBuf::from(r.get::<_, String>(0)?),
                timestamp_ms: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(result)
}

/// Removes a previously listed favorite even when the folder is unavailable.
/// This only deletes the database record; it never accesses the filesystem.
pub fn remove_favorite(db: &Connection, saved_path: &Path) -> Result<bool> {
    let text = saved_path
        .to_str()
        .ok_or_else(|| ApiError::new("invalid_argument", "Folder path must be Unicode"))?;
    let key = if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.to_owned()
    };
    Ok(db.execute("DELETE FROM folder_favorites WHERE path_key=?1", [key])? != 0)
}

pub fn folder_favorites(db: &Connection) -> Result<Vec<FolderEntry>> {
    let mut stmt = db.prepare(
        "SELECT path,added_at_ms FROM folder_favorites ORDER BY added_at_ms DESC,path_key",
    )?;
    let result = stmt
        .query_map([], |r| {
            Ok(FolderEntry {
                path: PathBuf::from(r.get::<_, String>(0)?),
                timestamp_ms: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(result)
}

pub fn clear_folder_history(db: &Connection) -> Result<()> {
    db.execute("DELETE FROM folder_history", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(db: &Connection) {
        db.execute_batch("PRAGMA foreign_keys=ON;
            CREATE TABLE events(id INTEGER PRIMARY KEY);
            INSERT INTO events VALUES(1),(2);
            CREATE TABLE katran_properties(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE, pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN(0,1)), display_name TEXT, opener_path TEXT);
            CREATE TABLE katran_tags(event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE, tag_key TEXT NOT NULL, tag_display TEXT NOT NULL, PRIMARY KEY(event_id,tag_key));
            CREATE INDEX katran_tags_lookup ON katran_tags(tag_key,event_id);
            CREATE INDEX katran_properties_pinned ON katran_properties(pinned,event_id);
            CREATE TABLE folder_history(path_key TEXT PRIMARY KEY, path TEXT NOT NULL, opened_at_ms INTEGER NOT NULL);
            CREATE INDEX folder_history_recent ON folder_history(opened_at_ms DESC,path_key);
            CREATE TABLE folder_favorites(path_key TEXT PRIMARY KEY, path TEXT NOT NULL, added_at_ms INTEGER NOT NULL);").unwrap();
    }

    #[test]
    fn properties_tags_and_mixed_pin_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("index.sqlite3");
        let db = Connection::open(&db_path).unwrap();
        schema(&db);
        let result = toggle_pins(&db, &[1, 99, 1]).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].pinned, Some(true));
        assert_eq!(result[1].error.as_ref().unwrap().code, "not_found");
        set_properties(
            &db,
            1,
            Some("  Name  ".into()),
            Some(PathBuf::from("app.exe")),
        )
        .unwrap();
        let assigned =
            set_tags(&db, 1, &[" Notion ".into(), "notion".into(), "Тест".into()]).unwrap();
        assert_eq!(assigned.len(), 2);
        assert_eq!(assigned[0].display, "Notion");
        let mixed = toggle_pins(&db, &[1, 2]).unwrap();
        assert!(mixed.iter().all(|item| item.pinned == Some(true)));
        toggle_pins(&db, &[1, 2]).unwrap();
        assert_eq!(
            set_tags(&db, 1, &["other".into()]).unwrap_err().code,
            "not_pinned"
        );
        assert_eq!(
            set_properties(&db, 1, None, None).unwrap_err().code,
            "not_pinned"
        );
        drop(db);
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        let p = properties(&db, 1).unwrap();
        assert!(!p.pinned);
        assert_eq!(p.display_name.as_deref(), Some("Name"));
        assert_eq!(p.opener, Some(PathBuf::from("app.exe")));
        assert_eq!(tags(&db, 1).unwrap(), assigned);
        db.execute("DELETE FROM events WHERE id=1", []).unwrap();
        let remaining: i64 = db
            .query_row("SELECT count(*) FROM katran_tags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn invalid_tag_does_not_replace_existing_set() {
        let db = Connection::open_in_memory().unwrap();
        schema(&db);
        toggle_pins(&db, &[1]).unwrap();
        let original = set_tags(&db, 1, &["First".into()]).unwrap();
        assert!(set_tags(&db, 1, &["new".into(), "\n".into()]).is_err());
        assert_eq!(tags(&db, 1).unwrap(), original);
    }

    #[test]
    fn history_clear_keeps_favorites_and_missing_folders_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        let db = Connection::open_in_memory().unwrap();
        schema(&db);
        record_folder(&db, &folder).unwrap();
        record_folder(&db, &folder).unwrap();
        assert_eq!(folder_history(&db).unwrap().len(), 1);
        assert!(toggle_favorite(&db, &folder).unwrap());
        clear_folder_history(&db).unwrap();
        std::fs::remove_dir(&folder).unwrap();
        assert!(folder_history(&db).unwrap().is_empty());
        let favorites = folder_favorites(&db).unwrap();
        assert_eq!(favorites.len(), 1);
        assert!(record_folder(&db, &folder).is_err());
        assert!(remove_favorite(&db, &favorites[0].path).unwrap());
    }
}

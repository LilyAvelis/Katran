//! Crash recovery owns only validated SHA-256 filenames inside the cache directory.
use crate::{ApiError, Event, Result, origin::Origin};
use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn directory(root: &Path) -> Result<PathBuf> {
    let directory = root.join("blobs");
    if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        return Err(ApiError::new(
            "integrity_error",
            "Cache directory is a symbolic link",
        ));
    }
    Ok(directory)
}

/// Run after the origin_json column migration. The table rebuild and cleanup hold
/// SQLite's writer lock, so no concurrent ingestion can lose its staged object.
/// Missing referenced files remain explicit unavailable events and can be repaired
/// by ingesting the same bytes again. External event paths are never followed.
pub fn initialize(db: &Connection, root: &Path) -> Result<()> {
    db.create_scalar_function(
        "katran_contains",
        2,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let haystack: String = context.get(0)?;
            let needle: String = context.get(1)?;
            Ok(haystack.to_lowercase().contains(&needle.to_lowercase()))
        },
    )?;
    let directory = directory(root)?;
    let tx = db.unchecked_transaction()?;
    // Acquire the writer lock before inspecting files.
    tx.execute("UPDATE blobs SET size=size WHERE 0", [])?;
    let schema: String = tx.query_row(
        "SELECT sql FROM sqlite_master WHERE name='events'",
        [],
        |r| r.get(0),
    )?;
    let has_reference_hash: bool = tx.query_row(
        "SELECT COUNT(*)>0 FROM pragma_table_info('events') WHERE name='reference_hash'",
        [],
        |row| row.get(0),
    )?;
    if !has_reference_hash {
        tx.execute_batch("ALTER TABLE events ADD COLUMN reference_hash TEXT;")?;
    }
    if !schema.to_ascii_uppercase().contains("AUTOINCREMENT") {
        tx.execute_batch("CREATE TABLE events_monotonic(
            id INTEGER PRIMARY KEY AUTOINCREMENT, detected_at_ms INTEGER NOT NULL,
            source TEXT NOT NULL, name TEXT NOT NULL, path TEXT NOT NULL,
            size INTEGER NOT NULL, blob_hash TEXT REFERENCES blobs(hash),
            origin_json TEXT NOT NULL DEFAULT '{}', reference_hash TEXT);
            INSERT INTO events_monotonic SELECT id,detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash FROM events;
            DROP TABLE events;
            ALTER TABLE events_monotonic RENAME TO events;")?;
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS events_blob_hash ON events(blob_hash);")?;
    let references = {
        let mut stmt =
            tx.prepare("SELECT id,path,reference_hash FROM events WHERE blob_hash IS NULL")?;
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut protected_paths = HashSet::new();
    for (id, path, hash) in references {
        if let Ok(canonical) = fs::canonicalize(&path) {
            protected_paths.insert(canonical);
        }
        if hash.is_none()
            && let Ok(hash) = reference_content_hash(&path)
        {
            tx.execute(
                "UPDATE events SET reference_hash=?1 WHERE id=?2",
                params![hash, id],
            )?;
        }
    }
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let staging = name.strip_suffix(".tmp").is_some_and(is_hash);
        if !(staging || is_hash(name)) || !entry.file_type()?.is_file() {
            continue;
        }
        if fs::canonicalize(entry.path()).is_ok_and(|path| protected_paths.contains(&path)) {
            continue;
        }
        let referenced: bool = if staging {
            false
        } else {
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE blob_hash=?1)",
                [name],
                |r| r.get(0),
            )?
        };
        if !referenced {
            fs::remove_file(entry.path())?;
        }
    }
    tx.execute("DELETE FROM blobs WHERE NOT EXISTS(SELECT 1 FROM events WHERE events.blob_hash=blobs.hash)", [])?;
    tx.commit()?;
    Ok(())
}

pub struct ByteIngest<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub bytes: &'a [u8],
    pub origin: &'a Origin,
}

/// Publish durable bytes before committing their metadata. A failed/crashed
/// commit leaves only a recognizable orphan, reclaimed on the next startup.
pub fn ingest_bytes(
    db: &mut Connection,
    root: &Path,
    quota: u64,
    input: ByteIngest<'_>,
) -> Result<i64> {
    let directory = directory(root)?;
    let hash = format!("{:x}", Sha256::digest(input.bytes));
    let path = directory.join(&hash);
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM blobs WHERE hash=?1)",
        [&hash],
        |r| r.get(0),
    )?;
    let usage: u64 = tx.query_row("SELECT COALESCE(SUM(size),0) FROM blobs", [], |r| r.get(0))?;
    if !exists && usage.saturating_add(input.bytes.len() as u64) > quota {
        return Err(ApiError::new(
            "quota_exceeded",
            "Cache is full; no existing object was deleted",
        ));
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || fs::read(&path)? != input.bytes {
                return Err(ApiError::new(
                    "integrity_error",
                    "Cached object failed content verification",
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let staging = directory.join(format!("{hash}.tmp"));
            // Never reclaim a legacy reference that happens to use a staging name.
            if let Ok(canonical) = fs::canonicalize(&staging) {
                let mut stmt = tx.prepare("SELECT path FROM events WHERE blob_hash IS NULL")?;
                for path in stmt.query_map([], |row| row.get::<_, String>(0))? {
                    if fs::canonicalize(path?).is_ok_and(|path| path == canonical) {
                        return Err(ApiError::new(
                            "integrity_error",
                            "Cache staging path is an external reference",
                        ));
                    }
                }
            }
            // A preceding failed attempt cannot still be writing under this lock.
            match fs::symlink_metadata(&staging) {
                Ok(meta) if meta.file_type().is_file() => fs::remove_file(&staging)?,
                Ok(_) => {
                    return Err(ApiError::new(
                        "integrity_error",
                        "Unsafe cache staging path",
                    ));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)?;
            file.write_all(input.bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&staging, &path)?;
        }
        Err(error) => return Err(error.into()),
    }
    let encoded = path
        .to_str()
        .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode path"))?;
    tx.execute(
        "INSERT OR IGNORE INTO blobs(hash,size) VALUES(?1,?2)",
        params![hash, input.bytes.len() as u64],
    )?;
    tx.execute("INSERT INTO events(detected_at_ms,source,name,path,size,blob_hash,origin_json) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![crate::now_ms(),input.source,input.name,encoded,input.bytes.len() as u64,hash,serde_json::to_string(input.origin).map_err(|e| ApiError::new("invalid_argument",e))?])?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(id)
}

/// One SQL query applies search and the stable descending-ID cursor before LIMIT.
/// Unicode folding and literal substring matching preserve filename semantics.
pub fn query_events(
    db: &Connection,
    query: &str,
    limit: u32,
    after_id: Option<i64>,
) -> Result<Vec<Event>> {
    if !(1..=1000).contains(&limit) || after_id.is_some_and(|id| id <= 0) {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected limit 1..1000 and positive cursor",
        ));
    }
    let mut stmt = db.prepare("SELECT id,detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash FROM events
        WHERE id <= ?1 AND (?2='' OR katran_contains(name,?2) OR katran_contains(source,?2)
        OR katran_contains(CASE WHEN json_valid(origin_json) THEN COALESCE(json_extract(origin_json,'$.source_app'),'') ELSE '' END,?2)
        OR katran_contains(CASE WHEN json_valid(origin_json) THEN COALESCE(json_extract(origin_json,'$.source_domain'),'') ELSE '' END,?2))
        ORDER BY id DESC LIMIT ?3")?;
    Ok(stmt
        .query_map(
            params![after_id.map_or(i64::MAX, |id| id - 1), query, limit],
            event_row,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let path = PathBuf::from(r.get::<_, String>(4)?);
    let hash: Option<String> = r.get(6)?;
    let reference_hash: Option<String> = r.get(8)?;
    Ok(Event {
        revision: if hash.is_none() && reference_hash.is_some() {
            format!("ref:{}", reference_hash.as_deref().unwrap())
        } else {
            crate::preview::revision(&path, hash.as_deref())
        },
        id: r.get(0)?,
        detected_at_ms: r.get(1)?,
        source: r.get(2)?,
        name: r.get(3)?,
        available: path.is_file(),
        path,
        size: r.get(5)?,
        storage: if hash.is_some() {
            "cached"
        } else {
            "reference"
        }
        .into(),
        origin: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default(),
    })
}

pub fn get_event(db: &Connection, id: i64) -> Result<Event> {
    match db.query_row("SELECT id,detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash FROM events WHERE id=?1",[id],event_row) {
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(ApiError::new("not_found","Unknown event ID")),
        result => Ok(result?),
    }
}

/// Streaming identity capture for legacy references. Only explicitly referenced
/// regular files are read; failed reads leave the legacy record unchanged.
fn reference_content_hash(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("Not a regular file"));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

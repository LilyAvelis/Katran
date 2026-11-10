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

fn stale_owned_stage(name: &str) -> bool {
    let parts: Vec<_> = name.split('.').collect();
    if parts.len() != 4
        || !is_hash(parts[0])
        || parts[3] != "tmp"
        || parts[2].len() != 16
        || !parts[2]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return false;
    }
    let Ok(pid) = parts[1].parse::<u32>() else {
        return false;
    };
    if parts[1] != pid.to_string() || pid == 0 || pid == std::process::id() {
        return false;
    }
    process_has_exited(pid)
}
#[cfg(windows)]
fn process_has_exited(pid: u32) -> bool {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetExitCodeProcess(handle: *mut c_void, code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    let handle = unsafe { OpenProcess(0x1000, 0, pid) };
    if handle.is_null() {
        return std::io::Error::last_os_error().raw_os_error() == Some(87);
    }
    let mut code = 259;
    let known = unsafe { GetExitCodeProcess(handle, &mut code) } != 0;
    unsafe {
        CloseHandle(handle);
    }
    known && code != 259
}
#[cfg(not(windows))]
fn process_has_exited(_pid: u32) -> bool {
    false
}

/// Run after the origin_json column migration. The table rebuild and cleanup hold
/// SQLite's writer lock, so no concurrent ingestion can lose its staged object.
/// Missing referenced files remain explicit unavailable events and can be repaired
/// by ingesting the same bytes again. External event paths are never followed.
pub fn initialize(db: &Connection, root: &Path) -> Result<()> {
    crate::catalogue_schema::check_version(db)?;
    crate::sorting::register_sql(db)?;
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
        let legacy_staging = name.strip_suffix(".tmp").is_some_and(is_hash);
        let owned_staging = stale_owned_stage(name);
        let staging = legacy_staging || owned_staging;
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
    crate::catalogue_schema::migrate(db)?;
    Ok(())
}

#[derive(Clone, Copy)]
pub struct ByteIngest<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub bytes: &'a [u8],
    pub origin: &'a Origin,
}

/// A private durable worker receipt. Its staging path is owned until publication.
pub(crate) struct PreparedBlob {
    directory: PathBuf,
    hash: String,
    size: u64,
    stage: Option<OwnedStage>,
    existing: Option<FileStamp>,
    verified_stamp: FileStamp,
    interest: PublicationInterest,
    name: String,
    source: String,
    origin: Origin,
    text: Option<String>,
    link: bool,
    table: bool,
}
impl PreparedBlob {
    pub(crate) fn mark_text(&mut self, text: String) {
        self.text = Some(text);
    }
    pub(crate) fn mark_link(&mut self, text: String) {
        self.text = Some(text);
        self.link = true;
    }
    pub(crate) fn mark_table(&mut self, tsv: String) {
        self.text = Some(tsv);
        self.table = true;
    }
    pub(crate) fn is_table(&self) -> bool {
        self.table
    }
    pub(crate) fn is_link(&self) -> bool {
        self.link
    }
    pub(crate) fn is_text(&self) -> bool {
        self.text.is_some()
    }
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}
struct PublicationInterest(PathBuf);
struct Publication {
    interests: usize,
    stamp: Option<FileStamp>,
}
fn publications() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, Publication>> {
    static PUBLICATIONS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, Publication>>,
    > = std::sync::OnceLock::new();
    PUBLICATIONS.get_or_init(Default::default)
}
impl PublicationInterest {
    fn new(path: PathBuf) -> Self {
        let mut known = publications()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let entry = known.entry(path.clone()).or_insert(Publication {
            interests: 0,
            stamp: None,
        });
        entry.interests += 1;
        Self(path)
    }
    fn matches(&self, stamp: &FileStamp) -> bool {
        publications()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(&self.0)
            .is_some_and(|publication| publication.stamp.as_ref() == Some(stamp))
    }
    fn publish(&self, stamp: FileStamp) {
        if let Some(publication) = publications()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get_mut(&self.0)
        {
            publication.stamp = Some(stamp);
        }
    }
}
impl Drop for PublicationInterest {
    fn drop(&mut self) {
        let mut known = publications()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(publication) = known.get_mut(&self.0) {
            publication.interests -= 1;
            if publication.interests == 0 {
                known.remove(&self.0);
            }
        }
    }
}
struct OwnedStage(PathBuf);
impl Drop for OwnedStage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    size: u64,
    modified: std::time::SystemTime,
}
fn file_stamp(metadata: &fs::Metadata) -> Result<FileStamp> {
    Ok(FileStamp {
        size: metadata.len(),
        modified: metadata.modified()?,
    })
}
static NEXT_STAGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Hash, validate cached bytes, and durably stage new bytes on a preparation worker.
/// References are an immutable dispatcher snapshot; modern ingest cannot add
/// references inside managed storage while this receipt is pending.
pub(crate) fn prepare_bytes(
    root: &Path,
    quota: u64,
    input: ByteIngest<'_>,
    references: &[PathBuf],
) -> Result<PreparedBlob> {
    crate::validate_name(input.name)?;
    crate::validate_source(input.source)?;
    let directory = directory(root)?;
    let hash = format!("{:x}", Sha256::digest(input.bytes));
    let path = directory.join(&hash);
    // Register before observing the destination: every pending observer retains
    // proof of an overlapping trusted publication until its own commit/drop.
    let interest = PublicationInterest::new(path.clone());
    let legacy = directory.join(format!("{hash}.tmp"));
    if let Ok(legacy) = fs::canonicalize(&legacy) {
        for reference in references {
            if fs::canonicalize(reference).is_ok_and(|reference| reference == legacy) {
                return Err(ApiError::new(
                    "integrity_error",
                    "Cache staging path is an external reference",
                ));
            }
        }
    }
    let existing = match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file()
                || metadata.len() != input.bytes.len() as u64
                || fs::read(&path)? != input.bytes
            {
                return Err(ApiError::new(
                    "integrity_error",
                    "Cached object failed content verification",
                ));
            }
            let after = fs::symlink_metadata(&path)?;
            if file_stamp(&metadata)? != file_stamp(&after)? {
                return Err(ApiError::new(
                    "integrity_error",
                    "Cached object changed while verifying",
                ));
            }
            Some(file_stamp(&after)?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if existing.is_none() && input.bytes.len() as u64 > quota {
        return Err(ApiError::new(
            "quota_exceeded",
            "Cache is full; no existing object was deleted",
        ));
    }
    let mut verified_stamp = existing.clone();
    let stage = if existing.is_none() {
        let (stage, mut file) = loop {
            let nonce = NEXT_STAGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ^ (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64);
            let path = directory.join(format!("{hash}.{}.{nonce:016x}.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => break (OwnedStage(path), file),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        };
        file.write_all(input.bytes)?;
        file.sync_all()?;
        verified_stamp = Some(file_stamp(&file.metadata()?)?);
        drop(file);
        Some(stage)
    } else {
        None
    };
    Ok(PreparedBlob {
        directory,
        hash,
        size: input.bytes.len() as u64,
        stage,
        existing,
        verified_stamp: verified_stamp.expect("verified existing or staged bytes"),
        interest,
        name: input.name.to_owned(),
        source: input.source.to_owned(),
        origin: input.origin.clone(),
        text: None,
        link: false,
        table: false,
    })
}

/// Metadata/quota commit holds SQLite's writer lock across final publication.
/// Preparation receipts are private and consumed; no client can assert a digest.
pub(crate) fn commit_prepared_bytes(
    db: &mut Connection,
    root: &Path,
    quota: u64,
    prepared: PreparedBlob,
) -> Result<i64> {
    if prepared.directory != directory(root)? {
        return Err(ApiError::new(
            "integrity_error",
            "Prepared cache root changed",
        ));
    }
    let path = prepared.directory.join(&prepared.hash);
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM blobs WHERE hash=?1)",
        [&prepared.hash],
        |row| row.get(0),
    )?;
    let usage: u64 = tx.query_row("SELECT COALESCE(SUM(size),0) FROM blobs", [], |row| {
        row.get(0)
    })?;
    if !exists && usage.saturating_add(prepared.size) > quota {
        return Err(ApiError::new(
            "quota_exceeded",
            "Cache is full; no existing object was deleted",
        ));
    }
    match (&prepared.existing, fs::symlink_metadata(&path)) {
        (Some(expected), Ok(metadata))
            if metadata.file_type().is_file() && file_stamp(&metadata)? == *expected => {}
        (None, Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            let stage = prepared.stage.as_ref().ok_or_else(|| {
                ApiError::new("integrity_error", "Missing prepared cache staging file")
            })?;
            // A hard-link publication is atomic and refuses an existing target;
            // unlike rename it can never overwrite a concurrent/legacy file.
            fs::hard_link(&stage.0, &path)?;
        }
        (None, Ok(metadata))
            if metadata.file_type().is_file()
                && prepared.interest.matches(&file_stamp(&metadata)?) => {}
        (None, Ok(_)) => {
            return Err(ApiError::new(
                "prepared_conflict",
                "An unverified writer published this digest; prepare again",
            ));
        }
        _ => {
            return Err(ApiError::new(
                "integrity_error",
                "Cached object changed after preparation; prepare again",
            ));
        }
    }
    prepared.interest.publish(prepared.verified_stamp.clone());
    let encoded = path
        .to_str()
        .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode path"))?;
    tx.execute(
        "INSERT OR IGNORE INTO blobs(hash,size) VALUES(?1,?2)",
        params![prepared.hash, prepared.size],
    )?;
    tx.execute("INSERT INTO events(detected_at_ms,source,name,path,size,blob_hash,origin_json) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![crate::now_ms(),prepared.source,prepared.name,encoded,prepared.size,prepared.hash,serde_json::to_string(&prepared.origin).map_err(|error|ApiError::new("invalid_argument",error))?])?;
    let id = tx.last_insert_rowid();
    if let Some(content) = &prepared.text {
        let preview: String = content.chars().take(512).collect();
        let content_table = if prepared.table {
            "table_contents"
        } else {
            "text_contents"
        };
        tx.execute(
            &format!("INSERT OR IGNORE INTO {content_table}(blob_hash,content) VALUES(?1,?2)"),
            params![prepared.hash, content],
        )?;
        let table = if prepared.table {
            "table_fragments"
        } else if prepared.link {
            "link_fragments"
        } else {
            "text_fragments"
        };
        tx.execute(
            &format!("INSERT INTO {table}(event_id,preview) VALUES(?1,?2)"),
            params![id, preview],
        )?;
    }
    if prepared.text.is_none() {
        crate::classification_runtime::enqueue(&tx, id)?;
    }
    tx.commit()?;
    Ok(id)
}

/// Synchronous clients use the same preparation and metadata commit as jobs.
pub fn ingest_bytes(
    db: &mut Connection,
    root: &Path,
    quota: u64,
    input: ByteIngest<'_>,
) -> Result<i64> {
    let references = {
        let mut statement = db.prepare("SELECT path FROM events WHERE blob_hash IS NULL")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    };
    let prepared = prepare_bytes(root, quota, input, &references)?;
    match commit_prepared_bytes(db, root, quota, prepared) {
        Err(error) if error.code == "prepared_conflict" => {
            let prepared = prepare_bytes(root, quota, input, &references)?;
            commit_prepared_bytes(db, root, quota, prepared)
        }
        result => result,
    }
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
    let mut stmt = db.prepare("SELECT id,detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash,delete_policy_json,COALESCE((SELECT preview FROM text_fragments WHERE event_id=events.id),(SELECT preview FROM link_fragments WHERE event_id=events.id),(SELECT preview FROM table_fragments WHERE event_id=events.id)) AS text_preview, EXISTS(SELECT 1 FROM link_fragments WHERE event_id=events.id) AS is_link, EXISTS(SELECT 1 FROM table_fragments WHERE event_id=events.id) AS is_table, modified_at_ms FROM events
        WHERE id <= ?1 AND (?2='' OR katran_contains(name,?2) OR katran_contains(source,?2) OR EXISTS(SELECT 1 FROM text_fragments t JOIN text_contents c ON c.blob_hash=events.blob_hash WHERE t.event_id=events.id AND katran_contains(c.content,?2)) OR EXISTS(SELECT 1 FROM link_fragments l JOIN text_contents c ON c.blob_hash=events.blob_hash WHERE l.event_id=events.id AND katran_contains(c.content,?2)) OR EXISTS(SELECT 1 FROM table_fragments t JOIN table_contents c ON c.blob_hash=events.blob_hash WHERE t.event_id=events.id AND katran_contains(c.content,?2))
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

pub(crate) fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    event_row_with_availability(r, None)
}
pub(crate) fn event_row_with_availability(
    r: &rusqlite::Row<'_>,
    available: Option<bool>,
) -> rusqlite::Result<Event> {
    let path = PathBuf::from(r.get::<_, String>(4)?);
    let hash: Option<String> = r.get(6)?;
    let reference_hash: Option<String> = r.get(8)?;
    let text_preview = r
        .as_ref()
        .column_index("text_preview")
        .ok()
        .map(|i| r.get::<_, Option<String>>(i))
        .transpose()?
        .flatten();
    let name: String = r.get(3)?;
    let is_link = r
        .as_ref()
        .column_index("is_link")
        .ok()
        .map(|i| r.get::<_, bool>(i))
        .transpose()?
        .unwrap_or(false);
    let modified_at_ms = r
        .as_ref()
        .column_index("modified_at_ms")
        .ok()
        .map(|i| r.get::<_, Option<i64>>(i))
        .transpose()?
        .flatten();
    let is_table = r
        .as_ref()
        .column_index("is_table")
        .ok()
        .map(|i| r.get::<_, bool>(i))
        .transpose()?
        .unwrap_or(false);
    let kind = if is_table {
        crate::EventKind::TableFragment
    } else if is_link {
        crate::EventKind::LinkFragment
    } else if text_preview.is_some() {
        crate::EventKind::TextFragment
    } else if hash.is_some() && crate::file_types::is_native_image(Path::new(&name)) {
        crate::EventKind::ManagedImage
    } else {
        crate::EventKind::FileReference
    };
    Ok(Event {
        kind,
        text_preview,
        modified_at_ms,
        delete_policy: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
        revision: if hash.is_none() && reference_hash.is_some() {
            format!("ref:{}", reference_hash.as_deref().unwrap())
        } else {
            crate::preview::revision(&path, hash.as_deref())
        },
        id: r.get(0)?,
        detected_at_ms: r.get(1)?,
        source: r.get(2)?,
        name,
        available: available.unwrap_or_else(|| path.is_file()),
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
    match db.query_row("SELECT id,detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash,delete_policy_json,COALESCE((SELECT preview FROM text_fragments WHERE event_id=events.id),(SELECT preview FROM link_fragments WHERE event_id=events.id),(SELECT preview FROM table_fragments WHERE event_id=events.id)) AS text_preview, EXISTS(SELECT 1 FROM link_fragments WHERE event_id=events.id) AS is_link, EXISTS(SELECT 1 FROM table_fragments WHERE event_id=events.id) AS is_table, modified_at_ms FROM events WHERE id=?1",[id],event_row) {
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

#[cfg(test)]
mod prepared_tests {
    use super::*;
    fn input(bytes: &[u8]) -> ByteIngest<'_> {
        static ORIGIN: std::sync::OnceLock<Origin> = std::sync::OnceLock::new();
        ByteIngest {
            name: "synthetic.bin",
            source: "test",
            bytes,
            origin: ORIGIN.get_or_init(Default::default),
        }
    }
    #[test]
    fn live_unique_worker_stages_survive_another_service_recovery_and_drop_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let service = crate::Service::open(temp.path(), 1000).unwrap();
        let first = prepare_bytes(&service.root, 1000, input(b"fixture"), &[]).unwrap();
        let second = prepare_bytes(&service.root, 1000, input(b"fixture"), &[]).unwrap();
        let first_path = first.stage.as_ref().unwrap().0.clone();
        let second_path = second.stage.as_ref().unwrap().0.clone();
        assert_ne!(first_path, second_path);
        let other = crate::Service::open(temp.path(), 1000).unwrap();
        assert!(first_path.exists());
        assert!(second_path.exists());
        drop(other);
        drop(first);
        assert!(!first_path.exists());
        assert!(second_path.exists());
        drop(second);
        assert!(!second_path.exists());
    }
    #[test]
    fn simultaneous_same_digest_receipts_never_overwrite_the_published_file() {
        let temp = tempfile::tempdir().unwrap();
        let mut service = crate::Service::open(temp.path(), 1000).unwrap();
        let first = prepare_bytes(&service.root, 1000, input(b"fixture"), &[]).unwrap();
        let second = prepare_bytes(&service.root, 1000, input(b"fixture"), &[]).unwrap();
        let second_stage = second.stage.as_ref().unwrap().0.clone();
        let id = commit_prepared_bytes(&mut service.db, &service.root, 1000, first).unwrap();
        let second_id =
            commit_prepared_bytes(&mut service.db, &service.root, 1000, second).unwrap();
        assert_ne!(id, second_id);
        assert!(!second_stage.exists());
        let event = get_event(&service.db, id).unwrap();
        assert_eq!(fs::read(event.path).unwrap(), b"fixture");
    }
    #[test]
    fn quota_failure_drops_only_private_stage_and_keeps_committed_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let mut service = crate::Service::open(temp.path(), 4).unwrap();
        let first = prepare_bytes(&service.root, 4, input(b"four"), &[]).unwrap();
        let id = commit_prepared_bytes(&mut service.db, &service.root, 4, first).unwrap();
        let second = prepare_bytes(&service.root, 4, input(b"x"), &[]).unwrap();
        let stage = second.stage.as_ref().unwrap().0.clone();
        assert_eq!(
            commit_prepared_bytes(&mut service.db, &service.root, 4, second)
                .unwrap_err()
                .code,
            "quota_exceeded"
        );
        assert!(!stage.exists());
        assert_eq!(
            fs::read(get_event(&service.db, id).unwrap().path).unwrap(),
            b"four"
        );
    }
}

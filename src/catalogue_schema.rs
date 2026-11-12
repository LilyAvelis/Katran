//! Ordered catalogue migrations. Future databases are rejected before any writes.
use crate::{ApiError, Result};
use rusqlite::Connection;

pub(crate) const VERSION: i64 = 10;
pub(crate) fn check_version(db: &Connection) -> Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > VERSION {
        return Err(ApiError::new(
            "unsupported_schema",
            format!("Database version {version} is newer than supported {VERSION}"),
        ));
    }
    Ok(())
}

/// Preserve a consistent pre-migration DB, including committed WAL contents.
/// Old 1.2.x binaries overwrite user_version and must use this copy for rollback.
pub(crate) fn backup_before_migration(db: &Connection, root: &std::path::Path) -> Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let existing: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='events')",
        [],
        |r| r.get(0),
    )?;
    if version >= VERSION || !existing {
        return Ok(());
    }
    let directory = root.join("backups");
    std::fs::create_dir_all(&directory)?;
    for nonce in 0..100 {
        let path = directory.join(format!(
            "index-before-v{VERSION}-{}-{}-{nonce}.sqlite3",
            crate::now_ms(),
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => drop(file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
        if let Err(error) = db.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()]) {
            let _ = std::fs::remove_file(&path);
            return Err(ApiError::new("migration_backup_failed", error));
        }
        return Ok(());
    }
    Err(ApiError::new(
        "migration_backup_failed",
        "No unused backup name",
    ))
}
pub(crate) fn migrate(db: &Connection) -> Result<()> {
    check_version(db)?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == VERSION {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    if version < 2 {
        let has_policy: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('events') WHERE name='delete_policy_json')",
        [],
        |r| r.get(0),
    )?;
        if !has_policy {
            tx.execute_batch("ALTER TABLE events ADD COLUMN delete_policy_json TEXT NOT NULL DEFAULT '{\"kind\":\"shelf_only\"}';
            ALTER TABLE events ADD COLUMN legacy_source_ref INTEGER NOT NULL DEFAULT 0;
            UPDATE events SET legacy_source_ref=1 WHERE blob_hash IS NULL AND source IN ('sharex','downloads');")?;
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS source_configurations(
        source_id TEXT PRIMARY KEY, generation INTEGER NOT NULL, root TEXT NOT NULL, enabled INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS source_projection(
        source_id TEXT NOT NULL, generation INTEGER NOT NULL, path_key TEXT NOT NULL,
        event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE, available INTEGER NOT NULL,
        PRIMARY KEY(source_id,generation,path_key));
      CREATE INDEX IF NOT EXISTS source_projection_event ON source_projection(event_id);
      CREATE TABLE IF NOT EXISTS source_receipts(source_id TEXT NOT NULL,root_key TEXT NOT NULL,path_key TEXT NOT NULL,hash TEXT NOT NULL,event_id INTEGER NOT NULL,PRIMARY KEY(source_id,root_key,path_key));
      CREATE INDEX IF NOT EXISTS source_projection_browse ON source_projection(source_id,generation,available,event_id DESC);
      CREATE INDEX IF NOT EXISTS events_shelf_browse ON events(id DESC) WHERE legacy_source_ref=0 AND json_extract(delete_policy_json,'$.kind')='shelf_only';
      CREATE INDEX IF NOT EXISTS events_source_id ON events(source,id DESC);
      CREATE TABLE IF NOT EXISTS classification_work(
        event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE, revision TEXT NOT NULL,
        kind TEXT NOT NULL, source_generation INTEGER NOT NULL DEFAULT 0,
        state TEXT NOT NULL DEFAULT 'queued', digest TEXT, error TEXT,
        PRIMARY KEY(event_id,revision,kind,source_generation));
      CREATE INDEX IF NOT EXISTS classification_pending ON classification_work(state,event_id);
      CREATE INDEX IF NOT EXISTS classification_kind ON classification_work(kind,source_generation);
      CREATE INDEX IF NOT EXISTS classification_digest ON classification_work(digest,kind,source_generation);
      CREATE TABLE IF NOT EXISTS classification_counts(state TEXT PRIMARY KEY,count INTEGER NOT NULL);
      DELETE FROM classification_counts;
      INSERT INTO classification_counts SELECT state,COUNT(*) FROM classification_work GROUP BY state;
      CREATE TRIGGER IF NOT EXISTS classification_insert AFTER INSERT ON classification_work BEGIN
        INSERT INTO classification_counts VALUES(NEW.state,1) ON CONFLICT(state) DO UPDATE SET count=count+1;
      END;
      CREATE TRIGGER IF NOT EXISTS classification_delete AFTER DELETE ON classification_work BEGIN
        UPDATE classification_counts SET count=count-1 WHERE state=OLD.state;
      END;
      CREATE TRIGGER IF NOT EXISTS classification_state AFTER UPDATE OF state ON classification_work WHEN OLD.state<>NEW.state BEGIN
        UPDATE classification_counts SET count=count-1 WHERE state=OLD.state;
        INSERT INTO classification_counts VALUES(NEW.state,1) ON CONFLICT(state) DO UPDATE SET count=count+1;
      END;
      CREATE TABLE IF NOT EXISTS verified_source_digests(digest TEXT NOT NULL,source_generation INTEGER NOT NULL,PRIMARY KEY(digest,source_generation));
      CREATE TABLE IF NOT EXISTS verified_screenshot_matches(
        clipboard_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
        clipboard_revision TEXT NOT NULL, source_generation INTEGER NOT NULL,
        PRIMARY KEY(clipboard_id,clipboard_revision,source_generation));
      INSERT OR IGNORE INTO classification_work(event_id,revision,kind,source_generation)
        SELECT id,CASE WHEN blob_hash IS NOT NULL THEN 'blob:'||blob_hash ELSE 'ref:'||reference_hash END,'clipboard',0
        FROM events WHERE source IN ('clipboard','clipboard_image','clipboard_file') AND (blob_hash IS NOT NULL OR reference_hash IS NOT NULL);
      UPDATE classification_work SET state='queued' WHERE state='inflight';
      PRAGMA user_version=2;")?;
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS text_contents(blob_hash TEXT PRIMARY KEY REFERENCES blobs(hash) ON DELETE CASCADE,content TEXT NOT NULL); CREATE TABLE IF NOT EXISTS text_fragments(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,preview TEXT NOT NULL); PRAGMA user_version=3;")?;
    if version < 4 {
        let has_modified: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('events') WHERE name='modified_at_ms')",
            [],
            |r| r.get(0),
        )?;
        if !has_modified {
            tx.execute_batch("ALTER TABLE events ADD COLUMN modified_at_ms INTEGER;")?;
        }
        tx.execute_batch(
            "CREATE INDEX IF NOT EXISTS events_sort_name ON events(name COLLATE NOCASE,id);
            CREATE INDEX IF NOT EXISTS events_sort_size ON events(size,id);
            CREATE INDEX IF NOT EXISTS events_sort_modified ON events(modified_at_ms,id);
            PRAGMA user_version=4;",
        )?;
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS link_fragments(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,preview TEXT NOT NULL); PRAGMA user_version=5;")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS table_contents(blob_hash TEXT PRIMARY KEY REFERENCES blobs(hash) ON DELETE CASCADE,content TEXT NOT NULL); CREATE TABLE IF NOT EXISTS table_fragments(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,preview TEXT NOT NULL); PRAGMA user_version=6;")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS katran_properties(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN(0,1)),display_name TEXT,opener_path TEXT);
      CREATE INDEX IF NOT EXISTS katran_properties_pinned ON katran_properties(pinned,event_id);
      CREATE TABLE IF NOT EXISTS katran_tags(event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,tag_key TEXT NOT NULL,tag_display TEXT NOT NULL,PRIMARY KEY(event_id,tag_key));
      CREATE INDEX IF NOT EXISTS katran_tags_lookup ON katran_tags(tag_key,event_id);
      CREATE TABLE IF NOT EXISTS folder_history(path_key TEXT PRIMARY KEY,path TEXT NOT NULL,opened_at_ms INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS folder_history_recent ON folder_history(opened_at_ms DESC,path_key);
      CREATE TABLE IF NOT EXISTS folder_favorites(path_key TEXT PRIMARY KEY,path TEXT NOT NULL,added_at_ms INTEGER NOT NULL);
      PRAGMA user_version=7;")?;
    tx.execute_batch("UPDATE events SET delete_policy_json=json_set(delete_policy_json,'$.mode','recycle') WHERE json_extract(delete_policy_json,'$.kind')='live_original'; UPDATE events SET modified_at_ms=detected_at_ms WHERE blob_hash IS NOT NULL AND modified_at_ms IS NULL; PRAGMA user_version=8;")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS files_pointers(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,path_key TEXT NOT NULL UNIQUE);
      PRAGMA user_version=9;")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS tag_slots(number INTEGER PRIMARY KEY CHECK(number BETWEEN 1 AND 9), name TEXT NOT NULL, color TEXT NOT NULL, material TEXT NOT NULL CHECK(material IN('solid','dots','lines','grain')), scale INTEGER NOT NULL CHECK(scale BETWEEN 1 AND 100), intensity INTEGER NOT NULL CHECK(intensity BETWEEN 0 AND 100), angle INTEGER NOT NULL CHECK(angle BETWEEN 0 AND 359), seed INTEGER NOT NULL CHECK(seed BETWEEN 0 AND 4294967295));
      CREATE TABLE IF NOT EXISTS tag_slot_assignments(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,slot_number INTEGER NOT NULL REFERENCES tag_slots(number));
      CREATE INDEX IF NOT EXISTS tag_slot_assignments_slot ON tag_slot_assignments(slot_number,event_id DESC);
      INSERT OR IGNORE INTO tag_slots(number,name,color,material,scale,intensity,angle,seed) VALUES
      (1,'Тег 1','#808080','solid',50,0,0,1),(2,'Тег 2','#808080','solid',50,0,0,2),
      (3,'Тег 3','#808080','solid',50,0,0,3),(4,'Тег 4','#808080','solid',50,0,0,4),
      (5,'Тег 5','#808080','solid',50,0,0,5),(6,'Тег 6','#808080','solid',50,0,0,6),
      (7,'Тег 7','#808080','solid',50,0,0,7),(8,'Тег 8','#808080','solid',50,0,0,8),
      (9,'Тег 9','#808080','solid',50,0,0,9);
      PRAGMA user_version=10;")?;
    tx.commit()?;
    Ok(())
}

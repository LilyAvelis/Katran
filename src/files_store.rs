//! Durable Files membership. The caller prepares trusted file metadata before
//! entering the database transaction; this module never copies or deletes files.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

/// Prepared internally, never accepted as a client-supplied receipt. `path` is
/// the canonical Unicode locator, and `path_key` is its canonical identity key.
/// The reference hash must be computed from the file by a trusted worker.
pub(crate) struct PreparedFile<'a> {
    pub path: &'a str,
    pub path_key: &'a str,
    pub name: &'a str,
    pub size: u64,
    pub reference_hash: &'a str,
    pub modified_at_ms: Option<i64>,
    pub detected_at_ms: i64,
    pub origin_json: &'a str,
}

/// Reuse does not change the original event, including its revision and metadata.
pub(crate) fn insert_or_reuse(
    db: &mut Connection,
    file: &PreparedFile<'_>,
) -> rusqlite::Result<i64> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing: Option<i64> = tx
        .query_row(
            "SELECT event_id FROM files_pointers WHERE path_key=?1",
            [file.path_key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        tx.commit()?;
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO events(detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash,modified_at_ms) VALUES(?1,'files',?2,?3,?4,NULL,?5,?6,?7)",
        params![file.detected_at_ms, file.name, file.path, file.size, file.origin_json, file.reference_hash, file.modified_at_ms],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO files_pointers(event_id,path_key) VALUES(?1,?2)",
        params![id, file.path_key],
    )?;
    tx.commit()?;
    Ok(id)
}

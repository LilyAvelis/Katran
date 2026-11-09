//! Shelf deletion never follows event paths. Only unreferenced owned cache
//! files under root/blobs/<sha256> are reclaimed after event commit.
use crate::{ApiError, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{collections::HashSet, path::Path};

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub ids: Vec<i64>,
    pub confirmation_required: bool,
    pub deleted: usize,
    pub cleanup_pending: Vec<String>,
}

/// Confirmation uses the supplied exact snapshot, independent of GUI selection.
/// Cleanup errors do not disguise an already committed event deletion as failure.
pub fn delete_events(
    db: &mut Connection,
    root: &Path,
    ids: &[i64],
    confirmed: bool,
) -> Result<DeleteResult> {
    let mut seen = HashSet::new();
    let ids: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    if ids.is_empty() {
        return Err(ApiError::new(
            "empty_selection",
            "No shelf records selected",
        ));
    }
    let tx = db.transaction()?;
    let mut hashes = HashSet::new();
    for id in &ids {
        let hash = tx
            .query_row("SELECT blob_hash FROM events WHERE id=?1", [id], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()?;
        match hash {
            None => {
                return Err(ApiError::new(
                    "not_found",
                    format!("Shelf record {id} does not exist"),
                ));
            }
            Some(Some(hash)) => {
                hashes.insert(hash);
            }
            Some(None) => {}
        }
    }
    if ids.len() > 1 && !confirmed {
        return Ok(DeleteResult {
            ids,
            confirmation_required: true,
            deleted: 0,
            cleanup_pending: vec![],
        });
    }
    for id in &ids {
        tx.execute("DELETE FROM events WHERE id=?1", [id])?;
    }
    tx.commit()?;
    let mut cleanup_pending = vec![];
    for hash in hashes {
        let cleanup = (|| -> std::result::Result<(), String> {
            // Immediate transaction prevents a writer attaching this blob during cleanup.
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let referenced: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM events WHERE blob_hash=?1)",
                    [&hash],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if referenced {
                return Ok(());
            }
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid owned blob hash".into());
            }
            let directory = root.join("blobs");
            if let Ok(candidate) = directory.join(&hash).canonicalize() {
                let mut references = tx
                    .prepare("SELECT path FROM events WHERE blob_hash IS NULL")
                    .map_err(|e| e.to_string())?;
                let paths = references
                    .query_map([], |r| r.get::<_, String>(0))
                    .map_err(|e| e.to_string())?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                if paths
                    .iter()
                    .any(|p| std::fs::canonicalize(p).is_ok_and(|p| p == candidate))
                {
                    return Ok(());
                }
            }
            if std::fs::symlink_metadata(&directory)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("cache directory is a symbolic link".into());
            }
            match std::fs::remove_file(directory.join(&hash)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
            tx.execute("DELETE FROM blobs WHERE hash=?1", [&hash])
                .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })();
        if let Err(error) = cleanup {
            cleanup_pending.push(format!("{hash}: {error}"));
        }
    }
    let deleted = ids.len();
    Ok(DeleteResult {
        ids,
        confirmation_required: false,
        deleted,
        cleanup_pending,
    })
}

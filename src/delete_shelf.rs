//! Shelf deletion never follows event paths. Only unreferenced owned cache
//! files under root/blobs/<sha256> are reclaimed after event commit.
use crate::{ApiError, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{collections::HashSet, path::Path};

/// Protect an event and aliases of a pinned external path. Compare both the
/// stored path and its canonical form so a missing/replaced file at the same
/// name remains protected. Owned blob paths are not original-file aliases.
pub(crate) fn protected_by_pin(db: &Connection, id: i64) -> Result<bool> {
    let event: Option<(String, Option<String>, bool)> = db.query_row(
        "SELECT e.path,e.blob_hash,COALESCE(p.pinned,0) FROM events e LEFT JOIN katran_properties p ON p.event_id=e.id WHERE e.id=?1",
        [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).optional()?;
    let Some((path, blob_hash, pinned)) = event else {
        return Err(ApiError::new(
            "not_found",
            format!("Shelf record {id} does not exist"),
        ));
    };
    if pinned {
        return Ok(true);
    }
    if blob_hash.is_some() {
        return Ok(false);
    }
    let mut stmt = db.prepare("SELECT e.path FROM events e JOIN katran_properties p ON p.event_id=e.id WHERE p.pinned=1 AND e.blob_hash IS NULL")?;
    let paths = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let canonical = std::fs::canonicalize(&path).ok();
    let equal = |a: &Path, b: &Path| {
        if cfg!(windows) {
            a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
        } else {
            a == b
        }
    };
    for candidate in paths {
        if equal(Path::new(&path), Path::new(&candidate)) {
            return Ok(true);
        }
        if let (Some(a), Ok(b)) = (&canonical, std::fs::canonicalize(candidate))
            && equal(a, &b)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug, Serialize)]
pub struct DeleteItem {
    pub id: i64,
    pub ok: bool,
    pub skipped_pinned: bool,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub ids: Vec<i64>,
    pub confirmation_required: bool,
    pub deleted: usize,
    pub cleanup_pending: Vec<String>,
    pub items: Vec<DeleteItem>,
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
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut hashes = HashSet::new();
    let mut eligible = Vec::new();
    let mut items = Vec::new();
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
            Some(hash) => {
                let skipped_pinned = protected_by_pin(&tx, *id)?;
                if !skipped_pinned {
                    if let Some(hash) = hash {
                        hashes.insert(hash);
                    }
                    eligible.push(*id);
                }
                items.push(DeleteItem {
                    id: *id,
                    ok: !skipped_pinned,
                    skipped_pinned,
                });
            }
        }
    }
    if ids.len() > 1 && !confirmed {
        return Ok(DeleteResult {
            ids,
            confirmation_required: true,
            deleted: 0,
            cleanup_pending: vec![],
            items: vec![],
        });
    }
    for id in &eligible {
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
    let deleted = eligible.len();
    Ok(DeleteResult {
        ids,
        confirmation_required: false,
        deleted,
        cleanup_pending,
        items,
    })
}

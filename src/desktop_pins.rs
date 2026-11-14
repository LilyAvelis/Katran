//! Persistent pin order for managed Desktop assets, whose IDs are projections
//! rather than rows in the capture-event catalogue.
use crate::{ApiError, Result, desktop_projection, pinned, pinned_order, tag_slots};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::collections::HashSet;

pub(crate) fn ensure_schema(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS desktop_pins(\
        asset_id INTEGER PRIMARY KEY, position INTEGER NOT NULL, slot_number INTEGER);\
        CREATE INDEX IF NOT EXISTS desktop_pins_order ON desktop_pins(position,asset_id);",
    )?;
    let has_slot: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('desktop_pins') WHERE name='slot_number')",
        [],
        |row| row.get(0),
    )?;
    if !has_slot {
        db.execute_batch("ALTER TABLE desktop_pins ADD COLUMN slot_number INTEGER;")?;
    }
    Ok(())
}

pub(crate) fn rank(db: &Connection, id: i64) -> Result<Option<i64>> {
    Ok(db
        .query_row(
            "SELECT position FROM desktop_pins WHERE asset_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?)
}

pub(crate) fn slot(db: &Connection, id: i64) -> Result<Option<u8>> {
    Ok(db
        .query_row(
            "SELECT slot_number FROM desktop_pins WHERE asset_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .flatten())
}

pub(crate) fn assign_slot(
    db: &Connection,
    ids: &[i64],
    number: Option<u8>,
) -> Result<Vec<tag_slots::ItemOutcome>> {
    if ids.is_empty()
        || ids.len() > 1000
        || ids.iter().any(|id| !desktop_projection::is_desktop_id(*id))
    {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected 1..1000 Desktop asset IDs",
        ));
    }
    if let Some(number) = number {
        if !(1..=9).contains(&number) {
            return Err(ApiError::new("invalid_tag_slot", "slot must be 1..9"));
        }
        let configured: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM tag_slots WHERE number=?1)",
            [number],
            |row| row.get(0),
        )?;
        if !configured {
            return Err(ApiError::new("invalid_tag_slot", "slot is not configured"));
        }
    }
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let mut seen = HashSet::new();
    let mut outcomes = Vec::new();
    for &id in ids {
        let outcome = if !seen.insert(id) {
            tag_slots::Outcome::Duplicate
        } else if rank(&tx, id)?.is_none() {
            tag_slots::Outcome::NotPinned
        } else {
            let next = if slot(&tx, id)? == number {
                None
            } else {
                number
            };
            tx.execute(
                "UPDATE desktop_pins SET slot_number=?2 WHERE asset_id=?1",
                params![id, next],
            )?;
            tag_slots::Outcome::Applied
        };
        outcomes.push(tag_slots::ItemOutcome { id, outcome });
    }
    tx.commit()?;
    Ok(outcomes)
}

pub(crate) fn toggle(db: &Connection, ids: &[i64]) -> Result<Vec<pinned::ItemResult>> {
    let mut seen = HashSet::new();
    let ids: Vec<_> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    if ids.is_empty()
        || ids.len() > 1000
        || ids.iter().any(|id| !desktop_projection::is_desktop_id(*id))
    {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected 1..1000 Desktop asset IDs",
        ));
    }
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let mut any_unpinned = false;
    for id in &ids {
        any_unpinned |= rank(&tx, *id)?.is_none();
    }
    let mut results = Vec::new();
    for id in ids {
        if any_unpinned {
            tx.execute("INSERT OR IGNORE INTO desktop_pins(asset_id,position) VALUES(?1,(SELECT COALESCE(MAX(position),0)+1 FROM desktop_pins))", [id])?;
        } else {
            tx.execute("DELETE FROM desktop_pins WHERE asset_id=?1", [id])?;
        }
        results.push(pinned::ItemResult {
            id,
            pinned: Some(any_unpinned),
            error: None,
        });
    }
    tx.commit()?;
    Ok(results)
}

pub(crate) fn swap_visible(
    db: &Connection,
    id: i64,
    other: i64,
) -> Result<pinned_order::MoveResult> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let current_rank =
        rank(&tx, id)?.ok_or_else(|| ApiError::new("not_pinned", "Desktop asset is not pinned"))?;
    let other_rank = rank(&tx, other)?
        .ok_or_else(|| ApiError::new("not_pinned", "Desktop asset is not pinned"))?;
    if id != other {
        tx.execute("UPDATE desktop_pins SET position=CASE WHEN asset_id=?1 THEN ?2 ELSE ?3 END WHERE asset_id IN (?1,?4)", params![id,other_rank,current_rank,other])?;
    }
    tx.commit()?;
    Ok(pinned_order::MoveResult {
        moved: id != other,
        rank: other_rank,
        generation: 0,
    })
}

pub(crate) fn remove(db: &Connection, id: i64) -> Result<()> {
    db.execute("DELETE FROM desktop_pins WHERE asset_id=?1", [id])?;
    Ok(())
}

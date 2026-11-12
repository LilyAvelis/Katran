//! Durable row-major ordering for pinned shelf events.
//!
//! Call `ensure_schema` in the catalogue schema transaction after `katran_properties`
//! exists. Triggers keep membership and generation in sync with all pin/deletion
//! paths. Existing pins migrate in descending event ID order (the old view order).
use crate::{ApiError, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct MoveResult {
    pub moved: bool,
    pub rank: i64,
    pub generation: i64,
}

/// Install the schema and triggers; safe to call repeatedly, including inside a
/// caller-owned transaction. New pins append in the order their writes occur.
pub fn ensure_schema(db: &Connection) -> Result<()> {
    db.execute_batch(" \
        CREATE TABLE IF NOT EXISTS katran_pinned_order_state( \
          singleton INTEGER PRIMARY KEY CHECK(singleton=1), generation INTEGER NOT NULL); \
        INSERT OR IGNORE INTO katran_pinned_order_state VALUES(1,0); \
        CREATE TABLE IF NOT EXISTS katran_pinned_order( \
          event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE, \
          position INTEGER NOT NULL); \
        CREATE INDEX IF NOT EXISTS katran_pinned_order_rank ON katran_pinned_order(position,event_id); \
        CREATE TRIGGER IF NOT EXISTS katran_pinned_order_added AFTER INSERT ON katran_pinned_order BEGIN \
          UPDATE katran_pinned_order_state SET generation=generation+1 WHERE singleton=1; END; \
        CREATE TRIGGER IF NOT EXISTS katran_pinned_order_removed AFTER DELETE ON katran_pinned_order BEGIN \
          UPDATE katran_pinned_order_state SET generation=generation+1 WHERE singleton=1; END; \
        CREATE TRIGGER IF NOT EXISTS katran_pinned_order_pin_insert AFTER INSERT ON katran_properties \
        WHEN NEW.pinned=1 BEGIN \
          INSERT INTO katran_pinned_order(event_id,position) \
          VALUES(NEW.event_id,(SELECT COALESCE(MAX(position),0)+1 FROM katran_pinned_order)); END; \
        CREATE TRIGGER IF NOT EXISTS katran_pinned_order_pin_update AFTER UPDATE OF pinned ON katran_properties \
        WHEN OLD.pinned=0 AND NEW.pinned=1 BEGIN \
          INSERT INTO katran_pinned_order(event_id,position) \
          VALUES(NEW.event_id,(SELECT COALESCE(MAX(position),0)+1 FROM katran_pinned_order)); END; \
        CREATE TRIGGER IF NOT EXISTS katran_pinned_order_unpin AFTER UPDATE OF pinned ON katran_properties \
        WHEN OLD.pinned=1 AND NEW.pinned=0 BEGIN \
          DELETE FROM katran_pinned_order WHERE event_id=NEW.event_id; END; \
        CREATE TRIGGER IF NOT EXISTS katran_pinned_order_property_delete AFTER DELETE ON katran_properties BEGIN \
          DELETE FROM katran_pinned_order WHERE event_id=OLD.event_id; END; \
        CREATE TRIGGER IF NOT EXISTS katran_pinned_order_event_delete AFTER DELETE ON events BEGIN \
          DELETE FROM katran_pinned_order WHERE event_id=OLD.id; END; \
        INSERT INTO katran_pinned_order(event_id,position) \
          SELECT event_id, (SELECT COALESCE(MAX(position),0) FROM katran_pinned_order) \
            + ROW_NUMBER() OVER (ORDER BY event_id DESC) \
          FROM katran_properties p WHERE p.pinned=1 \
            AND EXISTS(SELECT 1 FROM events e WHERE e.id=p.event_id) \
            AND NOT EXISTS(SELECT 1 FROM katran_pinned_order o WHERE o.event_id=p.event_id); \
    ")?;
    Ok(())
}

/// Changes whenever membership or ordering changes; include in keyset cursors.
pub fn generation(db: &Connection) -> Result<i64> {
    Ok(db.query_row(
        "SELECT generation FROM katran_pinned_order_state WHERE singleton=1",
        [],
        |r| r.get(0),
    )?)
}

/// Rank is unique among live pins; gaps are normal. Sort ascending by rank and
/// event ID and use both in keyset cursors. `None` means the event is not pinned.
pub fn rank(db: &Connection, id: i64) -> Result<Option<i64>> {
    Ok(db
        .query_row(
            "SELECT position FROM katran_pinned_order WHERE event_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Move one position in global row-major order. At either boundary returns
/// `moved=false` without changing generation. Missing and unpinned IDs are errors.
pub fn move_one(db: &Connection, id: i64, direction: Direction) -> Result<MoveResult> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let current = require_rank(&tx, id)?;
    let sql = match direction {
        Direction::Up => {
            "SELECT event_id,position FROM katran_pinned_order WHERE position<?1 ORDER BY position DESC,event_id DESC LIMIT 1"
        }
        Direction::Down => {
            "SELECT event_id,position FROM katran_pinned_order WHERE position>?1 ORDER BY position,event_id LIMIT 1"
        }
    };
    let neighbor: Option<(i64, i64)> = tx
        .query_row(sql, [current], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let (moved, new_rank) = if let Some((other, other_rank)) = neighbor {
        tx.execute("UPDATE katran_pinned_order SET position=CASE WHEN event_id=?1 THEN ?2 ELSE ?3 END WHERE event_id IN (?1,?4)", params![id, other_rank, current, other])?;
        tx.execute(
            "UPDATE katran_pinned_order_state SET generation=generation+1 WHERE singleton=1",
            [],
        )?;
        (true, other_rank)
    } else {
        (false, current)
    };
    let result = MoveResult {
        moved,
        rank: new_rank,
        generation: generation(&tx)?,
    };
    tx.commit()?;
    Ok(result)
}

fn require_rank(db: &Connection, id: i64) -> Result<i64> {
    match rank(db, id)? {
        Some(position) => Ok(position),
        None => {
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1)",
                [id],
                |r| r.get(0),
            )?;
            Err(if exists {
                ApiError::new("not_pinned", "Shelf record is not pinned")
            } else {
                ApiError::new("not_found", format!("Shelf record {id} does not exist"))
            })
        }
    }
}

/// Exchange two pinned positions atomically, including when hidden pins lie
/// between them. The same ID is a no-op after validation; missing or unpinned
/// IDs are errors, with no changes made.
pub fn swap(db: &Connection, id: i64, other_id: i64) -> Result<MoveResult> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let current = require_rank(&tx, id)?;
    let other = require_rank(&tx, other_id)?;
    let moved = id != other_id;
    if moved {
        tx.execute("UPDATE katran_pinned_order SET position=CASE WHEN event_id=?1 THEN ?2 ELSE ?3 END WHERE event_id IN (?1,?4)", params![id, other, current, other_id])?;
        tx.execute(
            "UPDATE katran_pinned_order_state SET generation=generation+1 WHERE singleton=1",
            [],
        )?;
    }
    let result = MoveResult {
        moved,
        rank: other,
        generation: generation(&tx)?,
    };
    tx.commit()?;
    Ok(result)
}

//! Persistent tag-slot settings and per-event assignments. Schema migration and
//! dispatcher wiring belong to the integrator; this module performs no UI work.
//! Unpin deletes the object's assignment in the same transaction; slot definitions
//! and their material settings remain independent and are never deleted by unpin.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

#[derive(Debug)]
pub(crate) enum StoreError {
    Invalid(&'static str),
    Database(rusqlite::Error),
}
impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Database(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for StoreError {}
type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Material {
    Solid,
    Dots,
    Lines,
    Grain,
}
impl Material {
    fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Dots => "dots",
            Self::Lines => "lines",
            Self::Grain => "grain",
        }
    }
    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "solid" => Ok(Self::Solid),
            "dots" => Ok(Self::Dots),
            "lines" => Ok(Self::Lines),
            "grain" => Ok(Self::Grain),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub number: u8,
    pub name: String,
    /// Opaque sRGB #RRGGBB.
    pub color: String,
    pub material: Material,
    /// Scale 1..=100, intensity 0..=100, angle 0..=359 degrees.
    pub scale: u8,
    pub intensity: u8,
    /// Base tag color opacity, independently of procedural texture strength.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    pub angle: u16,
    pub seed: u32,
}

fn default_opacity() -> u8 {
    55
}

fn valid_slot_number(number: u8) -> bool {
    (1..=9).contains(&number)
}
fn validate(slot: &Slot) -> Result<()> {
    if !valid_slot_number(slot.number) {
        return Err(StoreError::Invalid("slot must be 1..9"));
    }
    if slot.name.trim().is_empty()
        || slot.name.trim() != slot.name
        || slot.name.chars().count() > 64
        || slot.name.chars().any(char::is_control)
    {
        return Err(StoreError::Invalid(
            "slot name must be 1..64 non-control characters without outer whitespace",
        ));
    }
    let color = slot.color.as_bytes();
    if color.len() != 7 || color[0] != b'#' || !color[1..].iter().all(u8::is_ascii_hexdigit) {
        return Err(StoreError::Invalid("color must be #RRGGBB"));
    }
    if !(1..=100).contains(&slot.scale)
        || slot.intensity > 100
        || slot.opacity > 100
        || slot.angle > 359
    {
        return Err(StoreError::Invalid("material parameters out of range"));
    }
    Ok(())
}

/// Changes settings only; existing object assignments are unaffected.
pub(crate) fn save_slot(db: &Connection, slot: &Slot) -> Result<()> {
    validate(slot)?;
    db.execute("INSERT INTO tag_slots(number,name,color,material,scale,intensity,angle,seed,opacity) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(number) DO UPDATE SET name=excluded.name,color=excluded.color,material=excluded.material,scale=excluded.scale,intensity=excluded.intensity,angle=excluded.angle,seed=excluded.seed,opacity=excluded.opacity", params![slot.number,slot.name,slot.color,slot.material.as_str(),slot.scale,slot.intensity,slot.angle,slot.seed,slot.opacity])?;
    Ok(())
}

pub(crate) fn list_slots(db: &Connection) -> Result<Vec<Slot>> {
    let mut statement = db.prepare("SELECT number,name,color,material,scale,intensity,angle,seed,opacity FROM tag_slots ORDER BY number")?;
    let slots = statement
        .query_map([], |r| {
            let material: String = r.get(3)?;
            Ok(Slot {
                number: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                material: Material::parse(&material)?,
                scale: r.get(4)?,
                intensity: r.get(5)?,
                angle: r.get(6)?,
                seed: r.get(7)?,
                opacity: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for slot in &slots {
        validate(slot)?;
    }
    Ok(slots)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Applied,
    NotFound,
    NotPinned,
    Duplicate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct ItemOutcome {
    pub id: i64,
    pub outcome: Outcome,
}

/// `slot=None` clears; otherwise assignment replaces the previous slot.
/// Each input ID has an outcome. Repeated IDs are reported without reapplying.
/// Changes commit together; database failure rolls the entire batch back.
pub(crate) fn assign(
    db: &mut Connection,
    ids: &[i64],
    slot: Option<u8>,
) -> Result<Vec<ItemOutcome>> {
    if ids.is_empty() || ids.len() > 1000 {
        return Err(StoreError::Invalid("expected 1..1000 IDs"));
    }
    if slot.is_some_and(|n| !valid_slot_number(n)) {
        return Err(StoreError::Invalid("slot must be 1..9"));
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(number) = slot {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tag_slots WHERE number=?1)",
            [number],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(StoreError::Invalid("slot is not configured"));
        }
    }
    let mut seen = HashSet::new();
    let mut results = Vec::with_capacity(ids.len());
    for &id in ids {
        let outcome = if !seen.insert(id) {
            Outcome::Duplicate
        } else {
            let pinned: Option<bool> = tx.query_row("SELECT COALESCE(p.pinned,0) FROM events e LEFT JOIN katran_properties p ON p.event_id=e.id WHERE e.id=?1", [id], |r| r.get(0)).optional()?;
            match pinned {
                None => Outcome::NotFound,
                Some(false) => Outcome::NotPinned,
                Some(true) => {
                    if let Some(number) = slot {
                        tx.execute("INSERT INTO tag_slot_assignments(event_id,slot_number) VALUES(?1,?2) ON CONFLICT(event_id) DO UPDATE SET slot_number=excluded.slot_number", params![id,number])?;
                    } else {
                        tx.execute("DELETE FROM tag_slot_assignments WHERE event_id=?1", [id])?;
                    }
                    Outcome::Applied
                }
            }
        };
        results.push(ItemOutcome { id, outcome });
    }
    tx.commit()?;
    Ok(results)
}

/// Invoke after changing pins inside the very same transaction. Only IDs that
/// are now unpinned are affected; pinned IDs are never cleared accidentally.
/// The caller must propagate errors and roll back the encompassing transaction.
pub(crate) fn clear_after_unpin(db: &Connection, ids: &[i64]) -> rusqlite::Result<usize> {
    let mut deleted = 0;
    let mut statement = db.prepare("DELETE FROM tag_slot_assignments WHERE event_id=?1 AND NOT EXISTS (SELECT 1 FROM katran_properties p WHERE p.event_id=?1 AND p.pinned=1)")?;
    for id in ids {
        deleted += statement.execute([id])?;
    }
    Ok(deleted)
}

/// Defense in depth for incomplete/legacy state: unpinned objects expose no tag.
pub(crate) fn visible_slot(db: &Connection, id: i64) -> Result<Option<u8>> {
    Ok(db.query_row("SELECT a.slot_number FROM tag_slot_assignments a JOIN katran_properties p ON p.event_id=a.event_id AND p.pinned=1 WHERE a.event_id=?1", [id], |r| r.get(0)).optional()?)
}

/// Domain filter data; the caller intersects these IDs with its category/query.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn visible_ids(db: &Connection, slot: u8) -> Result<Vec<i64>> {
    if !valid_slot_number(slot) {
        return Err(StoreError::Invalid("slot must be 1..9"));
    }
    let mut stmt = db.prepare("SELECT a.event_id FROM tag_slot_assignments a JOIN katran_properties p ON p.event_id=a.event_id AND p.pinned=1 WHERE a.slot_number=?1 ORDER BY a.event_id DESC")?;
    let ids = stmt
        .query_map([slot], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

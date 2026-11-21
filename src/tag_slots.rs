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
    Conflict,
    Revision(&'static str),
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
            Self::Conflict => f.write_str("tag slots changed since they were read"),
            Self::Revision(message) => f.write_str(message),
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
    Flower,
    Star,
    Diamond,
    Grid,
}
impl Material {
    fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Dots => "dots",
            Self::Lines => "lines",
            Self::Grain => "grain",
            Self::Flower => "flower",
            Self::Star => "star",
            Self::Diamond => "diamond",
            Self::Grid => "grid",
        }
    }
    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "solid" => Ok(Self::Solid),
            "dots" => Ok(Self::Dots),
            "lines" => Ok(Self::Lines),
            "grain" => Ok(Self::Grain),
            "flower" => Ok(Self::Flower),
            "star" => Ok(Self::Star),
            "diamond" => Ok(Self::Diamond),
            "grid" => Ok(Self::Grid),
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
fn write_slot(db: &Connection, scope: &str, slot: &Slot) -> Result<()> {
    validate(slot)?;
    db.execute("INSERT INTO tag_scoped_slots(scope,number,name,color,material,scale,intensity,angle,seed,opacity) VALUES(?10,?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(scope,number) DO UPDATE SET name=excluded.name,color=excluded.color,material=excluded.material,scale=excluded.scale,intensity=excluded.intensity,angle=excluded.angle,seed=excluded.seed,opacity=excluded.opacity", params![slot.number,slot.name,slot.color,slot.material.as_str(),slot.scale,slot.intensity,slot.angle,slot.seed,slot.opacity,scope])?;
    Ok(())
}

pub(crate) fn save_slot(db: &Connection, scope: &str, slot: &Slot) -> Result<()> {
    validate(slot)?;
    let tx = rusqlite::Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let next = next_counter(&tx, scope)?;
    write_slot(&tx, scope, slot)?;
    write_counter(&tx, scope, next)?;
    tx.commit()?;
    Ok(())
}

/// Save only against the exact slot set selected by the editor. The check,
/// write, counter bump, and returned revision share one write transaction.
pub(crate) fn save_slot_checked(
    db: &Connection,
    scope: &str,
    slot: &Slot,
    expected_revision: &str,
) -> Result<String> {
    validate(slot)?;
    let tx = rusqlite::Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let slots = list_slots(&tx, scope)?;
    let next = next_counter(&tx, scope)?;
    if revision(&tx, scope, &slots)? != expected_revision {
        return Err(StoreError::Conflict);
    }
    write_slot(&tx, scope, slot)?;
    write_counter(&tx, scope, next)?;
    let slots = list_slots(&tx, scope)?;
    let result = revision(&tx, scope, &slots)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn list_slots(db: &Connection, scope: &str) -> Result<Vec<Slot>> {
    let mut statement = db.prepare("SELECT number,name,color,material,scale,intensity,angle,seed,opacity FROM tag_scoped_slots WHERE scope=?1 ORDER BY number")?;
    let slots = statement
        .query_map([scope], |r| {
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

fn counter(db: &Connection, scope: &str) -> Result<u64> {
    let key = format!("tag_slots_revision:{scope}");
    let raw: Option<String> = db
        .query_row(
            "SELECT document FROM preferences WHERE key=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(0);
    };
    if raw.is_empty()
        || (raw.len() > 1 && raw.starts_with('0'))
        || !raw.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(StoreError::Revision("invalid tag slot revision counter"));
    }
    raw.parse::<u64>()
        .map_err(|_| StoreError::Revision("invalid tag slot revision counter"))
}
fn next_counter(db: &Connection, scope: &str) -> Result<u64> {
    counter(db, scope)?
        .checked_add(1)
        .ok_or(StoreError::Revision("tag slot revision counter overflow"))
}
fn write_counter(db: &Connection, scope: &str, value: u64) -> Result<()> {
    let key = format!("tag_slots_revision:{scope}");
    db.execute("INSERT INTO preferences(key,document) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET document=excluded.document", params![key,value.to_string()])?;
    Ok(())
}
/// Opaque fingerprint combines definitions and the persistent change counter.
pub(crate) fn revision(db: &Connection, scope: &str, slots: &[Slot]) -> Result<String> {
    use sha2::{Digest, Sha256};
    let data = serde_json::to_vec(&(scope, counter(db, scope)?, slots))
        .expect("Slot serialization is infallible");
    Ok(format!("{:x}", Sha256::digest(data)))
}

/// `order[new - 1]` is the old slot number. Definitions and assignments move
/// together in one transaction, with the old-number snapshot checked first.
pub(crate) fn reorder(
    db: &mut Connection,
    scope: &str,
    order: &[u8],
    expected_revision: &str,
) -> Result<(Vec<Slot>, String)> {
    let mut seen = [false; 10];
    if order.len() != 9 {
        return Err(StoreError::Invalid(
            "order must be a permutation of slots 1..9",
        ));
    }
    for &number in order {
        if !valid_slot_number(number) || seen[number as usize] {
            return Err(StoreError::Invalid(
                "order must be a permutation of slots 1..9",
            ));
        }
        seen[number as usize] = true;
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let slots = list_slots(&tx, scope)?;
    if slots.len() != 9
        || slots
            .iter()
            .enumerate()
            .any(|(i, slot)| slot.number as usize != i + 1)
    {
        return Err(StoreError::Invalid("scope does not contain all nine slots"));
    }
    let next = next_counter(&tx, scope)?;
    if revision(&tx, scope, &slots)? != expected_revision {
        return Err(StoreError::Conflict);
    }
    let mut new_slots = Vec::with_capacity(9);
    for (index, &old_number) in order.iter().enumerate() {
        let mut slot = slots[old_number as usize - 1].clone();
        slot.number = (index + 1) as u8;
        write_slot(&tx, scope, &slot)?;
        new_slots.push(slot);
    }
    let mut cases = String::from("CASE slot_number");
    for (index, old_number) in order.iter().enumerate() {
        cases.push_str(&format!(" WHEN {old_number} THEN {}", index + 1));
    }
    cases.push_str(" ELSE slot_number END");
    if scope == "desktop" {
        tx.execute(
            &format!("UPDATE desktop_scoped_assignments SET slot_number={cases}"),
            [],
        )?;
    } else if scope == "pages" {
        tx.execute(&format!("UPDATE pages_pins SET slot_number={cases}"), [])?;
    } else {
        tx.execute(
            &format!("UPDATE tag_scoped_assignments SET slot_number={cases} WHERE scope=?1"),
            [scope],
        )?;
    }
    write_counter(&tx, scope, next)?;
    let new_revision = revision(&tx, scope, &new_slots)?;
    tx.commit()?;
    Ok((new_slots, new_revision))
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
    scope: &str,
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
            "SELECT EXISTS(SELECT 1 FROM tag_scoped_slots WHERE number=?1 AND scope=?2)",
            params![number, scope],
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
                Some(false) if scope != "images" => Outcome::NotPinned,
                Some(_) => {
                    if let Some(number) = slot {
                        let current: Option<u8> = tx
                            .query_row(
                                "SELECT slot_number FROM tag_scoped_assignments WHERE event_id=?1 AND scope=?2",
                                params![id,scope],
                                |r| r.get(0),
                            )
                            .optional()?;
                        if current == Some(number) {
                            tx.execute(
                                "DELETE FROM tag_scoped_assignments WHERE event_id=?1 AND scope=?2",
                                params![id, scope],
                            )?;
                        } else {
                            tx.execute("INSERT INTO tag_scoped_assignments(event_id,slot_number,scope) VALUES(?1,?2,?3) ON CONFLICT(scope,event_id) DO UPDATE SET slot_number=excluded.slot_number", params![id,number,scope])?;
                        }
                    } else {
                        tx.execute(
                            "DELETE FROM tag_scoped_assignments WHERE event_id=?1 AND scope=?2",
                            params![id, scope],
                        )?;
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

/// Invoke after changing pins inside the very same transaction. Images keep
/// their scoped tag when unpinned. Other scopes still require a pin.
/// The caller must propagate errors and roll back the encompassing transaction.
pub(crate) fn clear_after_unpin(db: &Connection, ids: &[i64]) -> rusqlite::Result<usize> {
    let mut deleted = 0;
    let mut statement = db.prepare("DELETE FROM tag_scoped_assignments WHERE event_id=?1 AND scope!='images' AND NOT EXISTS (SELECT 1 FROM katran_properties p WHERE p.event_id=?1 AND p.pinned=1)")?;
    for id in ids {
        deleted += statement.execute([id])?;
    }
    Ok(deleted)
}

/// Only the Images scope exposes tags independently of pins.
pub(crate) fn visible_slot(db: &Connection, scope: &str, id: i64) -> Result<Option<u8>> {
    Ok(db.query_row("SELECT a.slot_number FROM tag_scoped_assignments a LEFT JOIN katran_properties p ON p.event_id=a.event_id WHERE a.event_id=?1 AND a.scope=?2 AND (?2='images' OR p.pinned=1)", params![id,scope], |r| r.get(0)).optional()?)
}

/// Domain filter data; the caller intersects these IDs with its category/query.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn visible_ids(db: &Connection, scope: &str, slot: u8) -> Result<Vec<i64>> {
    if !valid_slot_number(slot) {
        return Err(StoreError::Invalid("slot must be 1..9"));
    }
    let mut stmt = db.prepare("SELECT a.event_id FROM tag_scoped_assignments a LEFT JOIN katran_properties p ON p.event_id=a.event_id WHERE a.slot_number=?1 AND a.scope=?2 AND (?2='images' OR p.pinned=1) ORDER BY a.event_id DESC")?;
    let ids = stmt
        .query_map(params![slot, scope], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

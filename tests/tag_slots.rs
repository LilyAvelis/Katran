#[path = "../src/tag_slots.rs"]
mod tag_slots;

use rusqlite::Connection;
use tag_slots::{
    Material, Outcome, Slot, assign, clear_after_unpin, list_slots, save_slot, visible_ids,
    visible_slot,
};

fn fixture(path: &std::path::Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;
        CREATE TABLE events(id INTEGER PRIMARY KEY);
        CREATE TABLE katran_properties(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,pinned INTEGER NOT NULL CHECK(pinned IN(0,1)));
        CREATE TABLE tag_slots(number INTEGER PRIMARY KEY CHECK(number BETWEEN 1 AND 9), name TEXT NOT NULL, color TEXT NOT NULL, material TEXT NOT NULL CHECK(material IN('solid','dots','lines','grain')), scale INTEGER NOT NULL CHECK(scale BETWEEN 1 AND 100), intensity INTEGER NOT NULL CHECK(intensity BETWEEN 0 AND 100), angle INTEGER NOT NULL CHECK(angle BETWEEN 0 AND 359), seed INTEGER NOT NULL CHECK(seed BETWEEN 0 AND 4294967295));
        CREATE TABLE tag_slot_assignments(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,slot_number INTEGER NOT NULL REFERENCES tag_slots(number));
        CREATE INDEX tag_slot_assignments_slot ON tag_slot_assignments(slot_number,event_id DESC);
        INSERT INTO events VALUES(1),(2),(3);
        INSERT INTO katran_properties VALUES(1,1),(2,0),(3,1);").unwrap();
    db
}
fn slot(number: u8) -> Slot {
    Slot {
        number,
        name: format!("Tag {number}"),
        color: "#1122AA".into(),
        material: Material::Dots,
        scale: 22,
        intensity: 45,
        angle: 30,
        seed: 12345,
    }
}

#[test]
fn unpin_clears_assignment_and_preserves_slot_settings() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("test.sqlite");
    let mut db = fixture(&path);
    save_slot(&db, &slot(1)).unwrap();
    save_slot(&db, &slot(2)).unwrap();
    let results = assign(&mut db, &[1, 2, 99, 1, 3], Some(1)).unwrap();
    assert_eq!(
        results.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        vec![
            Outcome::Applied,
            Outcome::NotPinned,
            Outcome::NotFound,
            Outcome::Duplicate,
            Outcome::Applied
        ]
    );
    assert_eq!(visible_ids(&db, 1).unwrap(), vec![3, 1]);
    assert_eq!(
        assign(&mut db, &[1], Some(2)).unwrap()[0].outcome,
        Outcome::Applied
    );
    assert_eq!(visible_slot(&db, 1).unwrap(), Some(2));
    {
        let tx = db.transaction().unwrap();
        tx.execute("UPDATE katran_properties SET pinned=0 WHERE event_id=1", [])
            .unwrap();
        assert_eq!(clear_after_unpin(&tx, &[1, 3]).unwrap(), 1);
        tx.commit().unwrap();
    }
    assert_eq!(visible_slot(&db, 1).unwrap(), None);
    assert!(visible_ids(&db, 2).unwrap().is_empty());
    assert_eq!(
        assign(&mut db, &[1], Some(1)).unwrap()[0].outcome,
        Outcome::NotPinned
    );
    assert_eq!(
        assign(&mut db, &[1], None).unwrap()[0].outcome,
        Outcome::NotPinned
    );
    drop(db);
    let mut db = Connection::open(path).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=ON; UPDATE katran_properties SET pinned=1 WHERE event_id=1;",
    )
    .unwrap();
    assert_eq!(list_slots(&db).unwrap(), vec![slot(1), slot(2)]);
    assert_eq!(visible_slot(&db, 1).unwrap(), None);
    assert_eq!(
        assign(&mut db, &[1], None).unwrap()[0].outcome,
        Outcome::Applied
    );
    assert_eq!(visible_slot(&db, 1).unwrap(), None);
    assert_eq!(visible_slot(&db, 3).unwrap(), Some(1));
}

#[test]
fn unpin_and_assignment_clear_roll_back_together() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(&root.path().join("test.sqlite"));
    save_slot(&db, &slot(1)).unwrap();
    assign(&mut db, &[1], Some(1)).unwrap();
    {
        let tx = db.transaction().unwrap();
        tx.execute("UPDATE katran_properties SET pinned=0 WHERE event_id=1", [])
            .unwrap();
        assert_eq!(clear_after_unpin(&tx, &[1]).unwrap(), 1);
        tx.rollback().unwrap();
    }
    assert_eq!(visible_slot(&db, 1).unwrap(), Some(1));
}

#[test]
fn validation_and_slot_edits_do_not_damage_assignments() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(&root.path().join("test.sqlite"));
    let mut settings = slot(1);
    save_slot(&db, &settings).unwrap();
    assign(&mut db, &[1], Some(1)).unwrap();
    settings.material = Material::Grain;
    settings.seed = u32::MAX;
    save_slot(&db, &settings).unwrap();
    assert_eq!(list_slots(&db).unwrap(), vec![settings.clone()]);
    assert_eq!(visible_slot(&db, 1).unwrap(), Some(1));
    settings.scale = 0;
    assert!(save_slot(&db, &settings).is_err());
    settings.scale = 1;
    settings.color = "red".into();
    assert!(save_slot(&db, &settings).is_err());
    assert!(assign(&mut db, &[], Some(1)).is_err());
    assert!(assign(&mut db, &vec![1; 1001], Some(1)).is_err());
    assert!(assign(&mut db, &[1], Some(9)).is_err());
    assert_eq!(visible_slot(&db, 1).unwrap(), Some(1));
}

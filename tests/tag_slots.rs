// Some included helpers are exercised through the dispatcher API tests.
#[allow(dead_code)]
#[path = "../src/tag_slots.rs"]
mod tag_slots;

use rusqlite::Connection;
use tag_slots::{
    Material, Outcome, Slot, assign, clear_after_unpin, list_slots, reorder, revision, save_slot,
    visible_ids, visible_slot,
};

fn fixture(path: &std::path::Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;
        CREATE TABLE preferences(key TEXT PRIMARY KEY, document TEXT NOT NULL);
        CREATE TABLE events(id INTEGER PRIMARY KEY);
        CREATE TABLE katran_properties(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,pinned INTEGER NOT NULL CHECK(pinned IN(0,1)));
        CREATE TABLE tag_scoped_slots(scope TEXT NOT NULL, number INTEGER NOT NULL CHECK(number BETWEEN 1 AND 9), name TEXT NOT NULL, color TEXT NOT NULL, material TEXT NOT NULL CHECK(material IN('solid','dots','lines','grain')), scale INTEGER NOT NULL CHECK(scale BETWEEN 1 AND 100), intensity INTEGER NOT NULL CHECK(intensity BETWEEN 0 AND 100), angle INTEGER NOT NULL CHECK(angle BETWEEN 0 AND 359), seed INTEGER NOT NULL CHECK(seed BETWEEN 0 AND 4294967295), opacity INTEGER NOT NULL DEFAULT 55 CHECK(opacity BETWEEN 0 AND 100), PRIMARY KEY(scope,number));
        CREATE TABLE tag_scoped_assignments(scope TEXT NOT NULL, event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,slot_number INTEGER NOT NULL, PRIMARY KEY(scope,event_id), FOREIGN KEY(scope,slot_number) REFERENCES tag_scoped_slots(scope,number));
        CREATE INDEX tag_scoped_assignments_slot ON tag_scoped_assignments(scope,slot_number,event_id DESC);
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
        opacity: 72,
        angle: 30,
        seed: 12345,
    }
}

#[test]
fn unpin_clears_assignment_and_preserves_slot_settings() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("test.sqlite");
    let mut db = fixture(&path);
    save_slot(&db, "images", &slot(1)).unwrap();
    save_slot(&db, "images", &slot(2)).unwrap();
    let results = assign(&mut db, "images", &[1, 2, 99, 1, 3], Some(1)).unwrap();
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
    assert_eq!(visible_ids(&db, "images", 1).unwrap(), vec![3, 1]);
    assert_eq!(
        assign(&mut db, "images", &[1], Some(2)).unwrap()[0].outcome,
        Outcome::Applied
    );
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(2));
    {
        let tx = db.transaction().unwrap();
        tx.execute("UPDATE katran_properties SET pinned=0 WHERE event_id=1", [])
            .unwrap();
        assert_eq!(clear_after_unpin(&tx, &[1, 3]).unwrap(), 1);
        tx.commit().unwrap();
    }
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), None);
    assert!(visible_ids(&db, "images", 2).unwrap().is_empty());
    assert_eq!(
        assign(&mut db, "images", &[1], Some(1)).unwrap()[0].outcome,
        Outcome::NotPinned
    );
    assert_eq!(
        assign(&mut db, "images", &[1], None).unwrap()[0].outcome,
        Outcome::NotPinned
    );
    drop(db);
    let mut db = Connection::open(path).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=ON; UPDATE katran_properties SET pinned=1 WHERE event_id=1;",
    )
    .unwrap();
    assert_eq!(list_slots(&db, "images").unwrap(), vec![slot(1), slot(2)]);
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), None);
    assert_eq!(
        assign(&mut db, "images", &[1], None).unwrap()[0].outcome,
        Outcome::Applied
    );
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), None);
    assert_eq!(visible_slot(&db, "images", 3).unwrap(), Some(1));
}

#[test]
fn unpin_and_assignment_clear_roll_back_together() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(&root.path().join("test.sqlite"));
    save_slot(&db, "images", &slot(1)).unwrap();
    assign(&mut db, "images", &[1], Some(1)).unwrap();
    {
        let tx = db.transaction().unwrap();
        tx.execute("UPDATE katran_properties SET pinned=0 WHERE event_id=1", [])
            .unwrap();
        assert_eq!(clear_after_unpin(&tx, &[1]).unwrap(), 1);
        tx.rollback().unwrap();
    }
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(1));
}

#[test]
fn validation_and_slot_edits_do_not_damage_assignments() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(&root.path().join("test.sqlite"));
    let mut settings = slot(1);
    save_slot(&db, "images", &settings).unwrap();
    assign(&mut db, "images", &[1], Some(1)).unwrap();
    settings.material = Material::Grain;
    settings.seed = u32::MAX;
    save_slot(&db, "images", &settings).unwrap();
    assert_eq!(list_slots(&db, "images").unwrap(), vec![settings.clone()]);
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(1));
    settings.scale = 0;
    assert!(save_slot(&db, "images", &settings).is_err());
    settings.scale = 1;
    settings.color = "red".into();
    assert!(save_slot(&db, "images", &settings).is_err());
    assert!(assign(&mut db, "images", &[], Some(1)).is_err());
    assert!(assign(&mut db, "images", &vec![1; 1001], Some(1)).is_err());
    assert!(assign(&mut db, "images", &[1], Some(9)).is_err());
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(1));
}

#[test]
fn reorder_keeps_full_tags_and_assignments_across_scopes_and_rolls_back() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(&root.path().join("test.sqlite"));
    db.execute_batch("CREATE TABLE desktop_scoped_assignments(asset_id INTEGER PRIMARY KEY, slot_number INTEGER NOT NULL);").unwrap();
    for scope in ["images", "text", "desktop"] {
        for number in 1..=9 {
            let mut tag = slot(number);
            tag.name = format!("{scope}-{number}");
            tag.opacity = 20 + number;
            save_slot(&db, scope, &tag).unwrap();
        }
    }
    assign(&mut db, "images", &[1], Some(1)).unwrap();
    assign(&mut db, "images", &[3], Some(2)).unwrap();
    assign(&mut db, "text", &[1], Some(1)).unwrap();
    db.execute("INSERT INTO desktop_scoped_assignments VALUES(100,1)", [])
        .unwrap();
    let original = list_slots(&db, "images").unwrap();
    let prior_revision = revision(&db, "images", &original).unwrap();
    let order = [2, 3, 1, 4, 5, 6, 7, 8, 9];
    let (changed, new_revision) = reorder(&mut db, "images", &order, &prior_revision).unwrap();
    let mut expected = original[0].clone();
    expected.number = 3;
    assert_eq!(changed[2], expected);
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(3));
    assert_eq!(visible_slot(&db, "images", 3).unwrap(), Some(1));
    assert_eq!(visible_slot(&db, "text", 1).unwrap(), Some(1));
    assert!(reorder(&mut db, "images", &order, &prior_revision).is_err());
    assert!(
        reorder(
            &mut db,
            "images",
            &[1, 1, 2, 3, 4, 5, 6, 7, 8],
            &new_revision
        )
        .is_err()
    );
    assert_eq!(list_slots(&db, "images").unwrap(), changed);
    db.execute_batch("CREATE TRIGGER fail_reorder BEFORE UPDATE ON tag_scoped_assignments BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(reorder(&mut db, "images", &order, &new_revision).is_err());
    assert_eq!(list_slots(&db, "images").unwrap(), changed);
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(3));
    db.execute_batch("DROP TRIGGER fail_reorder;").unwrap();
    let desktop_rev = revision(&db, "desktop", &list_slots(&db, "desktop").unwrap()).unwrap();
    reorder(&mut db, "desktop", &order, &desktop_rev).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT slot_number FROM desktop_scoped_assignments WHERE asset_id=100",
            [],
            |r| r.get::<_, u8>(0)
        )
        .unwrap(),
        3
    );
    drop(db);
    let db = Connection::open(root.path().join("test.sqlite")).unwrap();
    assert_eq!(list_slots(&db, "images").unwrap(), changed);
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(3));
}

#[test]
fn identical_tags_still_advance_persistent_revision_and_corruption_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("test.sqlite");
    let mut db = fixture(&path);
    for number in 1..=9 {
        let mut tag = slot(number);
        if number == 2 {
            tag.name = "Tag 1".into();
        }
        save_slot(&db, "images", &tag).unwrap();
    }
    assign(&mut db, "images", &[1], Some(1)).unwrap();
    assign(&mut db, "images", &[3], Some(2)).unwrap();
    let prior = revision(&db, "images", &list_slots(&db, "images").unwrap()).unwrap();
    let swap = [2, 1, 3, 4, 5, 6, 7, 8, 9];
    let (_, after) = reorder(&mut db, "images", &swap, &prior).unwrap();
    assert_ne!(prior, after);
    assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(2));
    assert_eq!(visible_slot(&db, "images", 3).unwrap(), Some(1));
    assert!(reorder(&mut db, "images", &swap, &prior).is_err());
    drop(db);
    let mut db = Connection::open(&path).unwrap();
    assert_eq!(
        revision(&db, "images", &list_slots(&db, "images").unwrap()).unwrap(),
        after
    );
    for invalid in ["01", "-1", "18446744073709551615", "18446744073709551616"] {
        db.execute(
            "UPDATE preferences SET document=?1 WHERE key='tag_slots_revision:images'",
            [invalid],
        )
        .unwrap();
        let before = list_slots(&db, "images").unwrap();
        assert!(matches!(
            save_slot(&db, "images", &slot(1)),
            Err(tag_slots::StoreError::Revision(_))
        ));
        assert_eq!(list_slots(&db, "images").unwrap(), before);
        assert!(matches!(
            reorder(&mut db, "images", &swap, &after),
            Err(tag_slots::StoreError::Revision(_))
        ));
        assert_eq!(list_slots(&db, "images").unwrap(), before);
        assert_eq!(visible_slot(&db, "images", 1).unwrap(), Some(2));
    }
}

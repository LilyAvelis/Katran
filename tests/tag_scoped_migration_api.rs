#[path = "common/migration_fixture.rs"]
mod migration_fixture;

use katran::{Command, Request, Service, interaction::ShelfTab, tag_slots::Slot};
use serde_json::Value;

fn call(service: &mut Service, command: Command) -> Value {
    let reply = service.execute(Request {
        api_version: 1,
        request_id: "scoped-tags".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

#[test]
fn legacy_v12_tags_survive_and_become_independent() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 100_000).unwrap();
    let id = call(
        &mut service,
        Command::IngestBytes {
            name: "fixture.png".into(),
            bytes: vec![1, 2, 3],
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut service, Command::TogglePins { ids: vec![id] });
    call(
        &mut service,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    drop(service);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    migration_fixture::remove_later_tables(&db);
    db.execute_batch("UPDATE tag_slots SET name='Legacy',color='#123456',material='grain',scale=24,intensity=31,angle=67,seed=198,opacity=76 WHERE number=2; PRAGMA user_version=12;").unwrap();
    db.execute(
        "INSERT INTO desktop_pins(asset_id,position,slot_number) VALUES(-999,1,2)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO tag_slot_assignments(event_id,slot_number) VALUES(?1,2)",
        [id],
    )
    .unwrap();
    drop(db);
    let mut service = Service::open(root.path(), 100_000).unwrap();
    let images = call(
        &mut service,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    let text = call(
        &mut service,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Text,
        },
    );
    assert_eq!(images["slots"], text["slots"]);
    assert_eq!(
        images["slots"][1],
        serde_json::json!({"number":2,"name":"Legacy","color":"#123456","material":"grain","scale":24,"intensity":31,"angle":67,"seed":198,"opacity":76})
    );
    assert_eq!(call(&mut service, Command::GetTagSlot { id })["slot"], 2);
    let mut slot: Slot = serde_json::from_value(images["slots"][1].clone()).unwrap();
    slot.name = "Images only".into();
    call(
        &mut service,
        Command::SaveTagSlotForTab {
            tab: ShelfTab::Images,
            slot,
        },
    );
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![id],
            slot: None,
        },
    );
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    assert_eq!(call(&mut service, Command::GetTagSlot { id })["slot"], 2);
    assert_eq!(
        call(&mut service, Command::ListTagSlots)["slots"][1]["name"],
        "Legacy"
    );
    drop(service);
    let mut service = Service::open(root.path(), 100_000).unwrap();
    assert_eq!(
        call(
            &mut service,
            Command::ListTagSlotsForTab {
                tab: ShelfTab::Images
            }
        )["slots"][1]["name"],
        "Images only"
    );
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    let snapshot = call(
        &mut service,
        Command::BrowseTabSnapshot {
            tab: ShelfTab::Images,
            query: String::new(),
            limit: 50,
        },
    );
    assert_eq!(snapshot["slots"][1]["name"], "Images only");
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT slot_number FROM desktop_scoped_assignments WHERE asset_id=-999",
            [],
            |r| r.get::<_, u8>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT slot_number FROM tag_slot_assignments WHERE event_id=?1",
            [id],
            |r| r.get::<_, u8>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(
        db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
}

#[test]
fn failed_migration_rolls_back_before_retry() {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path(), 100_000).unwrap();
    drop(service);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    migration_fixture::remove_later_tables(&db);
    db.execute_batch("UPDATE tag_slots SET name='Preserved',opacity=73 WHERE number=2; CREATE TABLE tag_scoped_assignments(collision INTEGER); PRAGMA user_version=12;").unwrap();
    drop(db);
    assert!(Service::open(root.path(), 100_000).is_err());
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        12
    );
    assert_eq!(
        db.query_row(
            "SELECT name,opacity FROM tag_slots WHERE number=2",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u8>(1)?))
        )
        .unwrap(),
        ("Preserved".to_owned(), 73)
    );
    assert!(
        !db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='tag_scoped_slots')",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    db.execute_batch("DROP TABLE tag_scoped_assignments;")
        .unwrap();
    drop(db);
    let mut service = Service::open(root.path(), 100_000).unwrap();
    let slots = call(
        &mut service,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    assert_eq!(slots["slots"][1]["name"], "Preserved");
    assert_eq!(slots["slots"][1]["opacity"], 73);
}

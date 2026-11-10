use katran::delete_shelf::delete_events;
use rusqlite::Connection;

fn fixture() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE blobs(hash TEXT PRIMARY KEY); CREATE TABLE events(id INTEGER PRIMARY KEY, blob_hash TEXT REFERENCES blobs(hash),path TEXT NOT NULL DEFAULT ''); INSERT INTO blobs VALUES ('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'); INSERT INTO events(id,blob_hash) VALUES(1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'),(2,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'),(3,NULL);").unwrap();
    db
}
fn count(db: &Connection) -> i64 {
    db.query_row("SELECT count(*) FROM events", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn group_requires_confirmation_and_uses_exact_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("blobs")).unwrap();
    let mut db = fixture();
    let pending = delete_events(&mut db, temp.path(), &[1, 2, 1], false).unwrap();
    assert!(pending.confirmation_required);
    assert_eq!(pending.ids, vec![1, 2]);
    assert_eq!(count(&db), 3);
    db.execute("INSERT INTO events(id,blob_hash) VALUES(4,NULL)", [])
        .unwrap();
    assert_eq!(
        delete_events(&mut db, temp.path(), &pending.ids, true)
            .unwrap()
            .deleted,
        2
    );
    assert_eq!(count(&db), 2);
}

#[test]
fn invalid_batch_is_atomic_and_duplicates_are_single_item() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("blobs")).unwrap();
    let mut db = fixture();
    assert!(delete_events(&mut db, temp.path(), &[1, 99], true).is_err());
    assert_eq!(count(&db), 3);
    assert!(delete_events(&mut db, temp.path(), &[], true).is_err());
    assert_eq!(
        delete_events(&mut db, temp.path(), &[1, 1], false)
            .unwrap()
            .deleted,
        1
    );
}

#[test]
fn removal_preserves_source_and_shared_blobs() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("original.txt");
    let blob = temp
        .path()
        .join("blobs")
        .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    std::fs::write(&source, b"source").unwrap();
    std::fs::create_dir(temp.path().join("blobs")).unwrap();
    std::fs::write(&blob, b"blob").unwrap();
    let mut db = fixture();
    delete_events(&mut db, temp.path(), &[1], false).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), b"source");
    assert_eq!(std::fs::read(&blob).unwrap(), b"blob");
    assert_eq!(
        db.query_row("SELECT blob_hash FROM events WHERE id=2", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM blobs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        delete_events(&mut db, temp.path(), &[2], false)
            .unwrap()
            .cleanup_pending
            .is_empty()
    );
    assert!(!blob.exists());
    assert!(source.exists());
}

#[test]
fn shelf_deletion_keeps_successful_clipboard_snapshot() {
    use katran::copy_out::{ClipboardSink, commit, prepare};
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    struct Sink;
    impl ClipboardSink for Sink {
        fn write(&mut self, _: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
            Ok((1, vec![]))
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let blobs = temp.path().join("blobs");
    std::fs::create_dir(&blobs).unwrap();
    let hash = format!("{:x}", Sha256::digest(b"pending paste"));
    let blob = blobs.join(&hash);
    std::fs::write(&blob, b"pending paste").unwrap();
    let event = katran::Event {
        kind: Default::default(),
        text_preview: None,
        delete_policy: Default::default(),
        origin: Default::default(),
        revision: "synthetic".into(),
        id: 1,
        detected_at_ms: 0,
        modified_at_ms: None,
        source: "test".into(),
        name: "image.png".into(),
        storage: "cached".into(),
        path: blob.clone(),
        size: 13,
        available: true,
    };
    let plan = prepare(temp.path(), &[event]).unwrap();
    commit(&plan, &mut Sink).unwrap();
    let export = plan.paths[0].clone();
    drop(plan);
    let mut db = fixture();
    db.execute("INSERT INTO blobs(hash) VALUES(?1)", [&hash])
        .unwrap();
    db.execute(
        "UPDATE events SET blob_hash=?1 WHERE blob_hash IS NOT NULL",
        [&hash],
    )
    .unwrap();
    db.execute("DELETE FROM blobs WHERE hash<>?1", [&hash])
        .unwrap();
    delete_events(&mut db, temp.path(), &[1, 2], true).unwrap();
    assert!(!blob.exists());
    assert_eq!(std::fs::read(export).unwrap(), b"pending paste");
}

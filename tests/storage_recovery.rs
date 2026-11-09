use katran::{
    origin::Origin,
    storage::{self, ByteIngest},
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;

fn fixture() -> (tempfile::TempDir, Connection) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("blobs")).unwrap();
    let db = Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE blobs(hash TEXT PRIMARY KEY,size INTEGER NOT NULL); CREATE TABLE events(id INTEGER PRIMARY KEY,detected_at_ms INTEGER NOT NULL,source TEXT NOT NULL,name TEXT NOT NULL,path TEXT NOT NULL,size INTEGER NOT NULL,blob_hash TEXT REFERENCES blobs(hash),origin_json TEXT NOT NULL DEFAULT '{}');").unwrap();
    (root, db)
}
fn ingest(db: &mut Connection, root: &std::path::Path, bytes: &[u8]) -> i64 {
    storage::ingest_bytes(
        db,
        root,
        1000,
        ByteIngest {
            name: "item",
            source: "test",
            bytes,
            origin: &Origin::default(),
        },
    )
    .unwrap()
}
#[test]
fn stale_stage_and_final_orphan_recover_without_touching_originals() {
    let (root, mut db) = fixture();
    let hash = format!("{:x}", Sha256::digest(b"hello"));
    fs::write(
        root.path().join("blobs").join(format!("{hash}.tmp")),
        b"partial",
    )
    .unwrap();
    let orphan = format!("{:x}", Sha256::digest(b"orphan"));
    fs::write(root.path().join("blobs").join(&orphan), b"orphan").unwrap();
    fs::write(root.path().join("blobs/keep.txt"), b"original").unwrap();
    storage::initialize(&db, root.path()).unwrap();
    assert!(!root.path().join("blobs").join(orphan).exists());
    assert_eq!(
        fs::read(root.path().join("blobs/keep.txt")).unwrap(),
        b"original"
    );
    ingest(&mut db, root.path(), b"hello");
    assert_eq!(
        fs::read(root.path().join("blobs").join(hash)).unwrap(),
        b"hello"
    );
}
#[test]
fn missing_referenced_blob_is_repaired_and_deleted_ids_are_never_reused() {
    let (root, mut db) = fixture();
    storage::initialize(&db, root.path()).unwrap();
    let first = ingest(&mut db, root.path(), b"hello");
    let path: String = db
        .query_row("SELECT path FROM events WHERE id=?1", [first], |r| r.get(0))
        .unwrap();
    fs::remove_file(&path).unwrap();
    storage::initialize(&db, root.path()).unwrap();
    assert!(!storage::query_events(&db, "", 10, None).unwrap()[0].available);
    let second = ingest(&mut db, root.path(), b"hello");
    assert!(std::path::Path::new(&path).is_file());
    db.execute("DELETE FROM events", []).unwrap();
    storage::initialize(&db, root.path()).unwrap();
    assert!(ingest(&mut db, root.path(), b"different") > second);
}
#[test]
fn migration_preserves_metadata_and_search_precedes_limit_and_cursor() {
    let (root, db) = fixture();
    for id in 1..=1010 {
        let name = if id <= 3 { "needle_%" } else { "newer" };
        db.execute("INSERT INTO events(id,detected_at_ms,source,name,path,size,origin_json) VALUES(?1,1,'test',?2,'missing',0,?3)",rusqlite::params![id,name,"{\"source_domain\":\"example.org\"}"]).unwrap();
    }
    storage::initialize(&db, root.path()).unwrap();
    let events = storage::query_events(&db, "needle_%", 2, None).unwrap();
    assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), vec![3, 2]);
    assert_eq!(
        storage::query_events(&db, "needle_%", 2, Some(2)).unwrap()[0].id,
        1
    );
    assert_eq!(
        storage::query_events(&db, "example.org", 1, None).unwrap()[0].id,
        1010
    );
    assert_eq!(
        events[0].origin.source_domain.as_deref(),
        Some("example.org")
    );
}

#[test]
fn deleting_last_event_then_ingesting_new_bytes_copies_the_new_object() {
    use katran::{Request, Service, copy_out};
    use serde_json::json;
    fn call(service: &mut Service, command: serde_json::Value) -> serde_json::Value {
        let response = service.execute(
            serde_json::from_value::<Request>(
                json!({"api_version":1,"request_id":"regression","command":command}),
            )
            .unwrap(),
        );
        assert_eq!(response["ok"], true, "{response}");
        response["result"].clone()
    }
    struct Sink(Vec<Vec<u8>>);
    impl copy_out::ClipboardSink for Sink {
        fn write(
            &mut self,
            paths: &[std::path::PathBuf],
        ) -> std::result::Result<(u32, Vec<String>), String> {
            self.0 = paths.iter().map(|path| fs::read(path).unwrap()).collect();
            Ok((1, vec![]))
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1000).unwrap();
    let old = call(
        &mut service,
        json!({"type":"ingest_bytes","name":"same.bin","source":"test","bytes":[1,2,3]}),
    );
    let old_id = old["id"].as_i64().unwrap();
    call(
        &mut service,
        json!({"type":"delete_events","ids":[old_id],"confirmed":true}),
    );
    drop(service);
    let mut service = Service::open(root.path(), 1000).unwrap();
    let new = call(
        &mut service,
        json!({"type":"ingest_bytes","name":"same.bin","source":"test","bytes":[9,8,7]}),
    );
    let new_id = new["id"].as_i64().unwrap();
    assert!(new_id > old_id);
    let stale = service.execute(
        serde_json::from_value::<Request>(
            json!({"api_version":1,"request_id":"stale","command":{"type":"get","id":old_id}}),
        )
        .unwrap(),
    );
    assert_eq!(stale["error"]["code"], "not_found");
    let event: katran::Event =
        serde_json::from_value(call(&mut service, json!({"type":"get","id":new_id}))).unwrap();
    let plan = copy_out::prepare(root.path(), &[event]).unwrap();
    let mut sink = Sink(vec![]);
    assert_eq!(copy_out::commit(&plan, &mut sink).unwrap().count, 1);
    assert_eq!(sink.0, vec![vec![9, 8, 7]]);
}

#[test]
fn unicode_search_folds_cyrillic_names_and_origins() {
    let (root, db) = fixture();
    storage::initialize(&db, root.path()).unwrap();
    db.execute("INSERT INTO events(detected_at_ms,source,name,path,size,origin_json) VALUES(1,'Захват','Изображение.PNG','missing',0,?1)",["{\"source_app\":\"Приложение\"}"]).unwrap();
    assert_eq!(
        storage::query_events(&db, "изОБРАжение", 1, None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        storage::query_events(&db, "приЛОЖЕНИЕ", 1, None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        storage::query_events(&db, "захВАТ", 1, None).unwrap().len(),
        1
    );
}

#[test]
fn stale_stage_does_not_block_retry_without_restart_and_corrupt_blob_is_explicit() {
    let (root, mut db) = fixture();
    storage::initialize(&db, root.path()).unwrap();
    let hash = format!("{:x}", Sha256::digest(b"hello"));
    fs::write(
        root.path().join("blobs").join(format!("{hash}.tmp")),
        b"interrupted",
    )
    .unwrap();
    let id = ingest(&mut db, root.path(), b"hello");
    fs::write(root.path().join("blobs").join(hash), b"corrupt").unwrap();
    let error = storage::ingest_bytes(
        &mut db,
        root.path(),
        1000,
        ByteIngest {
            name: "again",
            source: "test",
            bytes: b"hello",
            origin: &Origin::default(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "integrity_error");
    assert_eq!(
        storage::query_events(&db, "", 100, None)
            .unwrap()
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec![id]
    );
}

#[test]
fn legacy_references_inside_cache_survive_startup_and_maintenance() {
    let (root, db) = fixture();
    let hash = format!("{:x}", Sha256::digest(b"original"));
    let directory = root.path().join("blobs");
    for (id, name) in [(1, hash.clone()), (2, format!("{hash}.tmp"))] {
        let path = directory.join(name);
        fs::write(&path, b"original").unwrap();
        db.execute("INSERT INTO events(id,detected_at_ms,source,name,path,size) VALUES(?1,1,'legacy','original',?2,8)",rusqlite::params![id,path.to_str().unwrap()]).unwrap();
    }
    storage::initialize(&db, root.path()).unwrap();
    storage::initialize(&db, root.path()).unwrap();
    for name in [hash.clone(), format!("{hash}.tmp")] {
        assert_eq!(fs::read(directory.join(name)).unwrap(), b"original");
    }
    assert_eq!(
        storage::get_event(&db, 1).unwrap().revision,
        format!("ref:{hash}")
    );
}

#[test]
fn migration_preserves_preexisting_reference_hash_and_identity_does_not_drift() {
    let (root, db) = fixture();
    db.execute_batch("ALTER TABLE events ADD COLUMN reference_hash TEXT;")
        .unwrap();
    let original = root.path().join("original.bin");
    fs::write(&original, b"before").unwrap();
    let original_hash = format!("{:x}", Sha256::digest(b"before"));
    db.execute("INSERT INTO events(id,detected_at_ms,source,name,path,size,reference_hash) VALUES(7,1,'legacy','original',?1,6,?2)",rusqlite::params![original.to_str().unwrap(),original_hash]).unwrap();
    storage::initialize(&db, root.path()).unwrap();
    fs::write(&original, b"after!").unwrap();
    storage::initialize(&db, root.path()).unwrap();
    assert_eq!(
        storage::get_event(&db, 7).unwrap().revision,
        format!("ref:{original_hash}")
    );
}

#[test]
fn ingestion_never_reclaims_a_legacy_reference_at_its_staging_path() {
    let (root, mut db) = fixture();
    let hash = format!("{:x}", Sha256::digest(b"new bytes"));
    let staging = root.path().join("blobs").join(format!("{hash}.tmp"));
    fs::write(&staging, b"legacy original").unwrap();
    db.execute("INSERT INTO events(id,detected_at_ms,source,name,path,size) VALUES(1,1,'legacy','original',?1,15)",[staging.to_str().unwrap()]).unwrap();
    storage::initialize(&db, root.path()).unwrap();
    let error = storage::ingest_bytes(
        &mut db,
        root.path(),
        1000,
        ByteIngest {
            name: "new",
            source: "test",
            bytes: b"new bytes",
            origin: &Origin::default(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "integrity_error");
    assert_eq!(fs::read(staging).unwrap(), b"legacy original");
}

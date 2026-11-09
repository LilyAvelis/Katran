use katran::origin::{Origin, app_from_executable, domain_from_cf_html};

#[test]
fn metadata_persists_and_existing_schema_migrates_without_inventing_origin() {
    use katran::{Command, Request, Service};
    let root = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE blobs(hash TEXT PRIMARY KEY,size INTEGER NOT NULL);CREATE TABLE events(id INTEGER PRIMARY KEY,detected_at_ms INTEGER NOT NULL,source TEXT NOT NULL,name TEXT NOT NULL,path TEXT NOT NULL,size INTEGER NOT NULL,blob_hash TEXT REFERENCES blobs(hash));INSERT INTO events VALUES(1,0,'clipboard_file','old.png','missing.png',0,NULL);").unwrap();
    drop(db);
    let mut api = Service::open(root.path(), 100).unwrap();
    let request = |command| Request {
        api_version: 1,
        request_id: "origin-test".into(),
        command,
    };
    assert_eq!(
        api.execute(request(Command::Get { id: 1 }))["result"]["origin"],
        serde_json::json!({})
    );
    let r = api.execute(request(Command::IngestWithOrigin {
        origin: Origin {
            source_app: Some("Zen".into()),
            source_domain: Some("example.com".into()),
        },
        command: Box::new(Command::IngestBytes {
            name: "image.bin".into(),
            bytes: vec![1],
            source: "test".into(),
        }),
    }));
    assert_eq!(r["ok"], true);
    let id = r["result"]["id"].as_i64().unwrap();
    drop(api);
    let mut api = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        api.execute(request(Command::Get { id }))["result"]["origin"]["source_domain"],
        "example.com"
    );
    let plain = api.execute(request(Command::IngestBytes {
        name: "other.bin".into(),
        bytes: vec![2],
        source: "test".into(),
    }));
    assert_eq!(plain["result"]["origin"], serde_json::json!({}));
}

#[test]
fn synthetic_clipboard_header_keeps_only_domain() {
    let data = b"Version:1.0\r\nStartHTML:0000000100\r\nSourceURL:https://user:secret@Example.COM:8443/private?q=secret#fragment\r\n<html>body</html>";
    assert_eq!(domain_from_cf_html(data).as_deref(), Some("example.com"));
}

#[test]
fn absence_or_untrusted_locations_remain_unknown() {
    for data in [
        "Version:1.0\r\n<html>\nSourceURL:https://fake.example/\n</html>",
        "Version:1.0\r\nSourceURL:file:///C:/private/secret.png\r\n<html>",
        "https://example.com/plain-text",
        "Version:1.0\r\nSourceURL:not a URL\r\n<html>",
    ] {
        assert_eq!(domain_from_cf_html(data.as_bytes()), None);
    }
    assert_eq!(domain_from_cf_html(&[255, 254]), None);
}

#[test]
fn process_names_and_stream_labels() {
    assert_eq!(
        app_from_executable(r"C:\Apps\Zen\zen.exe").as_deref(),
        Some("Zen")
    );
    assert_eq!(
        app_from_executable(r"C:\Windows\explorer.exe").as_deref(),
        Some("Explorer")
    );
    assert_eq!(
        app_from_executable("Telegram.EXE").as_deref(),
        Some("Telegram")
    );
    assert_eq!(app_from_executable("custom.exe").as_deref(), Some("custom"));
    assert_eq!(app_from_executable(""), None);
    assert_eq!(Origin::default().label(), None);
    let origin = Origin {
        source_app: Some("Zen".into()),
        source_domain: Some("example.com".into()),
    };
    assert_eq!(origin.label().as_deref(), Some("Zen · example.com"));
    let encoded = serde_json::to_string(&origin).unwrap();
    assert!(!encoded.contains("SourceURL"));
    assert_eq!(
        serde_json::from_str::<Origin>("{}").unwrap(),
        Origin::default()
    );
}

use katran::sharex::SharexScanner;
use std::fs;

#[test]
fn stable_files_are_offered_once_and_updates_wait_again() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("capture.PNG");
    fs::write(&path, b"synthetic image").unwrap();
    let path = fs::canonicalize(path).unwrap();
    let mut scanner = SharexScanner::default();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert_eq!(scanner.scan(root.path()).unwrap(), vec![path.clone()]);
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    fs::write(&path, b"changed synthetic image with different size").unwrap();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert_eq!(scanner.scan(root.path()).unwrap(), vec![path.clone()]);
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert_eq!(
        fs::read(path).unwrap(),
        b"changed synthetic image with different size"
    );
}

#[test]
fn nested_images_only_and_empty_files_wait_for_content() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("2026/11");
    fs::create_dir_all(&nested).unwrap();
    let image = nested.join("capture.webp");
    fs::write(&image, b"").unwrap();
    fs::write(nested.join("notes.txt"), b"ignore").unwrap();
    let mut scanner = SharexScanner::default();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    fs::write(&image, b"synthetic").unwrap();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert_eq!(
        scanner.scan(root.path()).unwrap(),
        vec![fs::canonicalize(&image).unwrap()]
    );
    fs::remove_file(&image).unwrap();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
}

#[test]
fn invalid_roots_are_explicit_errors_without_advancing_observations() {
    let root = tempfile::tempdir().unwrap();
    let image = root.path().join("capture.jpg");
    fs::write(&image, b"synthetic").unwrap();
    let mut scanner = SharexScanner::default();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert!(scanner.scan(&root.path().join("missing")).is_err());
    assert!(scanner.scan(&image).is_err());
    assert_eq!(
        scanner.scan(root.path()).unwrap(),
        vec![fs::canonicalize(image).unwrap()]
    );
}

#[test]
fn failed_receipt_rolls_back_event_and_retries_without_resurrection() {
    use katran::{Command, Request, Service, settings::Settings};
    fn call(api: &mut Service, command: Command) -> serde_json::Value {
        let reply = api.execute(Request {
            api_version: 1,
            request_id: "sharex-atomic".into(),
            command,
        });
        assert_eq!(reply["ok"], true, "{reply}");
        reply["result"].clone()
    }
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("synthetic.png"), b"synthetic").unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let mut settings = Settings::defaults(100_000);
    settings.sharex_folder = Some(folder.path().to_path_buf());
    call(&mut api, Command::SetSettings { settings });
    call(&mut api, Command::ScanSharex);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_sharex_receipt BEFORE INSERT ON sharex_seen BEGIN SELECT RAISE(FAIL, 'synthetic receipt failure'); END;").unwrap();
    let failed = call(&mut api, Command::ScanSharex);
    assert_eq!(failed["items"][0]["ok"], false);
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0, "receipt failure must roll back its event");
    db.execute_batch("DROP TRIGGER reject_sharex_receipt")
        .unwrap();
    assert!(
        call(&mut api, Command::ScanSharex)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let succeeded = call(&mut api, Command::ScanSharex);
    assert_eq!(succeeded["items"][0]["ok"], true);
    let id = succeeded["items"][0]["event"]["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![id],
            confirmed: true,
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    for _ in 0..3 {
        assert!(
            call(&mut api, Command::ScanSharex)["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        count, 0,
        "deleted screenshots must not return after restart"
    );
}

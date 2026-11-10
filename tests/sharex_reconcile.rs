use katran::{Command, Request, Service, interaction::ShelfTab, origin::Origin};
fn call(api: &mut Service, command: Command) -> serde_json::Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "reconcile".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn browse(api: &mut Service, query: &str, after_id: Option<i64>) -> serde_json::Value {
    call(
        api,
        Command::BrowseShelf {
            query: query.into(),
            limit: 1,
            after_id,
        },
    )
}
#[test]
fn live_projection_hides_clipboard_and_missing_references_without_deleting_history() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("original.png");
    std::fs::write(&path, b"synthetic original").unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let mut settings = katran::settings::Settings::defaults(100_000);
    settings.sharex_folder = Some(source.path().to_path_buf());
    call(&mut api, Command::SetSettings { settings });

    let clip = call(
        &mut api,
        Command::IngestWithOrigin {
            origin: Origin {
                source_app: Some("sHaReX".into()),
                source_domain: None,
            },
            command: Box::new(Command::IngestBytes {
                name: "clipboard.png".into(),
                bytes: b"owned history".to_vec(),
                source: "clipboard".into(),
            }),
        },
    );
    // Seed the pre-1.1.1 source name to exercise non-destructive legacy projection.
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute(
        "UPDATE events SET source='clipboard_image' WHERE id=?1",
        [clip["id"].as_i64().unwrap()],
    )
    .unwrap();
    drop(db);
    let first = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "sharex".into(),
        },
    );
    std::fs::write(&path, b"synthetic updated original").unwrap();
    let latest = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "sharex".into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Screenshots,
        },
    );
    let page = browse(&mut api, "original", None);
    assert_eq!(page["events"][0]["id"], latest["id"]);
    assert_eq!(page["events"][0]["origin"]["source_app"], "ShareX folder");
    assert!(
        browse(&mut api, "", latest["id"].as_i64())["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    assert!(
        browse(&mut api, "", None)["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    assert_eq!(
        browse(&mut api, "", None)["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(path.is_file());
    std::fs::remove_file(&path).unwrap();
    assert!(
        browse(&mut api, "", None)["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for event in [first, latest, clip] {
        let kept = call(
            &mut api,
            Command::Get {
                id: event["id"].as_i64().unwrap(),
            },
        );
        assert_eq!(kept["id"], event["id"]);
        if kept["storage"] == "cached" {
            assert_eq!(kept["available"], true);
        }
    }
}
#[test]
fn external_removal_and_restoration_request_one_refresh_each() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("live.png");
    std::fs::write(&path, b"synthetic original").unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let mut settings = katran::settings::Settings::defaults(100_000);
    settings.sharex_folder = Some(folder.path().to_path_buf());
    call(&mut api, Command::SetSettings { settings });
    call(&mut api, Command::ScanSharex);
    call(&mut api, Command::ScanSharex);
    call(&mut api, Command::ScanSharex);
    std::fs::remove_file(&path).unwrap();
    let removed = call(&mut api, Command::ScanSharex);
    assert_eq!(removed["items"][0]["availability_changed"], true);
    assert!(
        call(&mut api, Command::ScanSharex)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    std::fs::write(&path, b"synthetic original").unwrap();
    let restored = call(&mut api, Command::ScanSharex);
    assert_eq!(restored["items"][0]["availability_changed"], true);
    assert!(
        call(&mut api, Command::ScanSharex)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(path.is_file());
}
#[test]
fn current_folder_only_and_reverted_content_gets_fresh_live_revision() {
    let root = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let path = first.path().join("live.png");
    std::fs::write(&path, b"first version").unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let mut settings = katran::settings::Settings::defaults(100_000);
    settings.sharex_folder = Some(first.path().to_path_buf());
    call(
        &mut api,
        Command::SetSettings {
            settings: settings.clone(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Screenshots,
        },
    );
    call(&mut api, Command::ScanSharex);
    call(&mut api, Command::ScanSharex);
    let original = browse(&mut api, "", None)["events"][0].clone();
    std::fs::write(&path, b"second version changed").unwrap();
    call(&mut api, Command::ScanSharex);
    call(&mut api, Command::ScanSharex);
    let updated = browse(&mut api, "", None)["events"][0].clone();
    assert_ne!(original["revision"], updated["revision"]);
    std::fs::write(&path, b"first version").unwrap();
    call(&mut api, Command::ScanSharex);
    call(&mut api, Command::ScanSharex);
    let reverted = browse(&mut api, "", None)["events"][0].clone();
    assert_eq!(original["revision"], reverted["revision"]);
    assert!(reverted["id"].as_i64().unwrap() > updated["id"].as_i64().unwrap());
    settings.sharex_folder = Some(second.path().to_path_buf());
    call(
        &mut api,
        Command::SetSettings {
            settings: settings.clone(),
        },
    );
    assert!(
        browse(&mut api, "", None)["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    settings.sharex_folder = None;
    call(&mut api, Command::SetSettings { settings });
    assert!(
        browse(&mut api, "", None)["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(path.is_file());
    assert_eq!(
        call(
            &mut api,
            Command::Get {
                id: original["id"].as_i64().unwrap()
            }
        )["id"],
        original["id"]
    );
}

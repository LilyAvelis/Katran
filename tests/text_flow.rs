use katran::{Command, Request, SelectionRef, Service, interaction::ShelfTab};
use serde_json::{Value, json};
fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "text-flow".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn text(api: &mut Service, body: &str) -> Value {
    call(
        api,
        Command::IngestText {
            text: body.into(),
            source: "test".into(),
        },
    )
}
#[test]
fn feature_visibility_search_and_file_transfer_share_the_domain() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100000).unwrap();
    let rejected = api.execute(Request {
        api_version: 1,
        request_id: "off".into(),
        command: Command::IngestText {
            text: "not captured".into(),
            source: "test".into(),
        },
    });
    assert_eq!(rejected["error"]["code"], "tab_disabled");
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let body = format!("{}\nneedle далеко в тексте", "a".repeat(600));
    let fragment = text(&mut api, &body);
    let id = fragment["id"].as_i64().unwrap();
    assert_eq!(fragment["kind"], "text_fragment");
    let ordinary = call(
        &mut api,
        Command::IngestBytes {
            name: "ordinary.txt".into(),
            bytes: b"file".to_vec(),
            source: "test".into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelf {
            query: "needle".into(),
            limit: 1,
            after_id: None,
        },
    );
    assert_eq!(page["events"][0]["id"], id);
    assert!(
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["id"] != ordinary["id"])
    );
    let files = call(
        &mut api,
        Command::Import {
            ids: vec![id],
            destination: output.path().into(),
        },
    );
    assert_eq!(files["items"][0]["ok"], true);
    assert_eq!(
        std::fs::read_to_string(files["items"][0]["path"].as_str().unwrap()).unwrap(),
        body
    );
    let drag = call(
        &mut api,
        Command::DragEventsChecked {
            items: vec![
                SelectionRef {
                    id,
                    revision: fragment["revision"].as_str().unwrap().into(),
                },
                SelectionRef {
                    id: ordinary["id"].as_i64().unwrap(),
                    revision: ordinary["revision"].as_str().unwrap().into(),
                },
            ],
        },
    );
    assert_eq!(drag["count"], 2);
    call(
        &mut api,
        Command::CancelPreparedDrag {
            token: drag["drag_token"].as_str().unwrap().into(),
        },
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: false,
        },
    );
    assert_eq!(call(&mut api, Command::Interaction)["tab"], "folder");
    let empty_folder = tempfile::tempdir().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: empty_folder.path().into(),
        },
    );
    assert!(
        call(
            &mut api,
            Command::BrowseShelf {
                query: "needle".into(),
                limit: 10,
                after_id: None
            }
        )["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(call(&mut api, Command::GetText { id })["text"], body);
    drop(api);
    let mut api = Service::open(root.path(), 100000).unwrap();
    assert_eq!(call(&mut api, Command::GetSettings)["text_enabled"], false);
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    assert_eq!(
        call(&mut api, Command::Tabs)["visible_tabs"],
        json!(["images", "screenshots", "downloads", "text", "files"])
    );
}

#[test]
fn legacy_saved_tab_order_appends_disabled_text_and_shared_content_is_reclaimed() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let first = text(&mut api, "same Unicode 🐄");
    let second = text(&mut api, "same Unicode 🐄");
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM text_contents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(db);
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![
                first["id"].as_i64().unwrap(),
                second["id"].as_i64().unwrap(),
            ],
            confirmed: true,
        },
    );
    drop(api);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM text_contents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute(
        "UPDATE preferences SET document=?1 WHERE key='general'",
        [r#"{"tab_order":["downloads","images","screenshots"],"hidden_tabs":[]}"#],
    )
    .unwrap();
    drop(db);
    let mut api = Service::open(root.path(), 10000).unwrap();
    let settings = call(&mut api, Command::GetSettings);
    assert_eq!(
        settings["tab_order"],
        json!([
            "downloads",
            "images",
            "screenshots",
            "text",
            "links",
            "tables",
            "pods",
            "files"
        ])
    );
    assert_eq!(settings["links_enabled"], false);
    assert_eq!(
        call(&mut api, Command::Tabs)["visible_tabs"],
        json!(["downloads", "images", "screenshots", "files"])
    );
}

#[test]
fn asynchronous_text_copy_cannot_publish_before_freshness_and_cancellation_checks() {
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    struct Sink(Arc<Mutex<Vec<String>>>);
    impl katran::text_fragments::ClipboardSink for Sink {
        fn write(&self, text: &str) -> katran::Result<u32> {
            self.0.lock().unwrap().push(text.into());
            Ok(42)
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let writes = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(Sink(writes.clone())));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let unrepresentable = text(&mut api, "one\0two");
    let refused = api.execute(Request {
        api_version: 1,
        request_id: "nul".into(),
        command: Command::CopyText {
            items: vec![SelectionRef {
                id: unrepresentable["id"].as_i64().unwrap(),
                revision: unrepresentable["revision"].as_str().unwrap().into(),
            }],
            separator: String::new(),
        },
    });
    assert_eq!(refused["error"]["code"], "clipboard_unrepresentable");
    let event = text(&mut api, "must remain private until commit");
    let id = event["id"].as_i64().unwrap();
    let selected = || {
        vec![SelectionRef {
            id,
            revision: event["revision"].as_str().unwrap().into(),
        }]
    };
    let start = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopyText {
                items: selected(),
                separator: "\n".into(),
            }),
        },
    );
    let cancelled = serde_json::from_value(start["job_id"].clone()).unwrap();
    call(&mut api, Command::CancelJob { job_id: cancelled });
    assert!(writes.lock().unwrap().is_empty());
    let start = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopyText {
                items: selected(),
                separator: "\n".into(),
            }),
        },
    );
    let job_id = serde_json::from_value(start["job_id"].clone()).unwrap();
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![id],
            confirmed: true,
        },
    );
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let status = call(&mut api, Command::PollJob { job_id });
        if matches!(
            status["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(writes.lock().unwrap().is_empty());
}

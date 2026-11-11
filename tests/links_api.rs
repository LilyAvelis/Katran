use katran::{
    Command, Request, SelectionRef, Service,
    drag_out::{Outcome, Preview, Runner},
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "link-test".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let r = reply(api, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
fn enabled(api: &mut Service, tab: ShelfTab) {
    call(api, Command::SetTabEnabled { tab, enabled: true });
}
fn link(api: &mut Service, url: &str) -> Value {
    call(
        api,
        Command::IngestLink {
            url: url.into(),
            source: "test".into(),
        },
    )
}
fn item(value: &Value) -> SelectionRef {
    SelectionRef {
        id: value["id"].as_i64().unwrap(),
        revision: value["revision"].as_str().unwrap().into(),
    }
}
struct Sink(Arc<Mutex<Vec<String>>>);
impl katran::text_fragments::ClipboardSink for Sink {
    fn write(&self, text: &str) -> katran::Result<u32> {
        self.0.lock().unwrap().push(text.into());
        Ok(77)
    }
}
struct FakeDrag(Arc<Mutex<Vec<(String, String)>>>);
impl Runner for FakeDrag {
    fn run(&mut self, paths: &[PathBuf], _: Option<&Preview>, _: u64) -> Result<Outcome, String> {
        for p in paths {
            self.0.lock().unwrap().push((
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(p).unwrap(),
            ));
        }
        Ok(Outcome {
            dropped: false,
            warnings: vec![],
        })
    }
}
struct FakeOpen(Arc<Mutex<Vec<PathBuf>>>);
impl katran::open_file::Launcher for FakeOpen {
    fn launch(
        &self,
        path: &std::path::Path,
        _: katran::open_file::Operation,
    ) -> katran::Result<()> {
        self.0.lock().unwrap().push(path.into());
        Ok(())
    }
}
#[test]
fn link_is_optional_exact_searchable_and_survives_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 2_000_000).unwrap();
    assert_eq!(
        reply(
            &mut api,
            Command::IngestLink {
                url: "https://x.test".into(),
                source: "test".into()
            }
        )["error"]["code"],
        "tab_disabled"
    );
    enabled(&mut api, ShelfTab::Text);
    let old = call(
        &mut api,
        Command::IngestText {
            text: "https://пример.рф/🐄?a=1".into(),
            source: "test".into(),
        },
    );
    enabled(&mut api, ShelfTab::Links);
    let url = "https://пример.рф/🐄?a=1#x";
    let event = link(&mut api, url);
    let id = item(&event).id;
    assert_eq!(event["kind"], "link_fragment");
    assert_eq!(call(&mut api, Command::GetLink { id })["url"], url);
    assert_eq!(call(&mut api, Command::GetDocumentText { id })["text"], url);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Links,
        },
    );
    let found = call(
        &mut api,
        Command::BrowseShelf {
            query: "пример".into(),
            limit: 20,
            after_id: None,
        },
    );
    assert_eq!(found["events"].as_array().unwrap().len(), 1);
    let opened = Arc::new(Mutex::new(Vec::new()));
    api.set_file_launcher(Box::new(FakeOpen(opened.clone())));
    call(
        &mut api,
        Command::OpenFile {
            id,
            operation: katran::open_file::Operation::Open,
        },
    );
    assert_eq!(opened.lock().unwrap()[0].to_string_lossy(), url);
    drop(api);
    let mut api = Service::open(root.path(), 2_000_000).unwrap();
    assert_eq!(call(&mut api, Command::GetLink { id })["url"], url);
    assert_eq!(
        call(&mut api, Command::Get { id: item(&old).id })["kind"],
        "text_fragment"
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Links,
            enabled: false,
        },
    );
    assert_eq!(call(&mut api, Command::GetLink { id })["url"], url);
    enabled(&mut api, ShelfTab::Links);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Links,
        },
    );
    let links = call(
        &mut api,
        Command::BrowseShelf {
            query: "".into(),
            limit: 20,
            after_id: None,
        },
    );
    assert!(
        links["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == id)
    );
    assert_eq!(call(&mut api, Command::GetLink { id })["url"], url);
}
#[test]
fn copy_order_drag_group_and_mixed_selection_are_revision_checked() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 2_000_000).unwrap();
    enabled(&mut api, ShelfTab::Links);
    enabled(&mut api, ShelfTab::Text);
    let a = link(&mut api, "https://example.com/a");
    let b = link(&mut api, "https://example.com/b");
    let t = call(
        &mut api,
        Command::IngestText {
            text: "hello".into(),
            source: "test".into(),
        },
    );
    let writes = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(Sink(writes.clone())));
    let ids = vec![item(&a).id, item(&b).id];
    call(&mut api, Command::SetVisible { ids });
    call(
        &mut api,
        Command::Select {
            id: item(&b).id,
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut api,
        Command::Select {
            id: item(&a).id,
            mode: SelectionMode::Toggle,
        },
    );
    let copied = call(&mut api, Command::CopySelectedObjects);
    assert_eq!(copied["kind"], "link_fragment");
    assert_eq!(
        *writes.lock().unwrap(),
        vec!["https://example.com/b\nhttps://example.com/a"]
    );
    let drag = Arc::new(Mutex::new(Vec::new()));
    api.set_drag_runner(Box::new(FakeDrag(drag.clone())));
    let plan = call(
        &mut api,
        Command::DragEventsChecked {
            items: vec![item(&b), item(&a)],
        },
    );
    assert_eq!(plan["count"], 1);
    call(
        &mut api,
        Command::RunPreparedDrag {
            token: plan["drag_token"].as_str().unwrap().into(),
            owner_window: 0,
        },
    );
    assert_eq!(
        *drag.lock().unwrap(),
        vec![(
            "links.txt".into(),
            "https://example.com/b\nhttps://example.com/a".into()
        )]
    );
    drag.lock().unwrap().clear();
    let plan = call(
        &mut api,
        Command::DragEventsChecked {
            items: vec![item(&a), item(&t)],
        },
    );
    assert_eq!(plan["count"], 2);
    call(
        &mut api,
        Command::RunPreparedDrag {
            token: plan["drag_token"].as_str().unwrap().into(),
            owner_window: 0,
        },
    );
    assert_eq!(drag.lock().unwrap().len(), 2);
    let mut stale = item(&a);
    stale.revision = "stale".into();
    assert_eq!(
        reply(
            &mut api,
            Command::DragEventsChecked {
                items: vec![item(&b), stale]
            }
        )["ok"],
        false
    );
}

#[test]
fn async_link_copy_cancel_and_deleted_selection_never_publish() {
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    enabled(&mut api, ShelfTab::Links);
    let writes = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(Sink(writes.clone())));
    let event = link(&mut api, "https://example.test/a");
    let id = item(&event).id;
    let start = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopySmart {
                items: vec![item(&event)],
            }),
        },
    );
    let job_id = serde_json::from_value(start["job_id"].clone()).unwrap();
    call(&mut api, Command::CancelJob { job_id });
    assert!(writes.lock().unwrap().is_empty());
    let start = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopySmart {
                items: vec![item(&event)],
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
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let state = call(&mut api, Command::PollJob { job_id });
        if matches!(
            state["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(writes.lock().unwrap().is_empty());
}

#[test]
fn range_copy_uses_visible_order() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    enabled(&mut api, ShelfTab::Links);
    let a = link(&mut api, "https://example.test/a");
    let b = link(&mut api, "https://example.test/b");
    let c = link(&mut api, "https://example.test/c");
    let writes = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(Sink(writes.clone())));
    call(
        &mut api,
        Command::SetVisible {
            ids: vec![item(&c).id, item(&a).id, item(&b).id],
        },
    );
    call(
        &mut api,
        Command::Select {
            id: item(&c).id,
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut api,
        Command::Select {
            id: item(&b).id,
            mode: SelectionMode::Range,
        },
    );
    call(&mut api, Command::CopySelectedObjects);
    assert_eq!(
        *writes.lock().unwrap(),
        vec!["https://example.test/c\nhttps://example.test/a\nhttps://example.test/b"]
    );
}

#[test]
fn async_grouped_drag_checks_every_selected_event_before_publication() {
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    enabled(&mut api, ShelfTab::Links);
    let a = link(&mut api, "https://example.test/a");
    let b = link(&mut api, "https://example.test/b");
    let start = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::DragEventsChecked {
                items: vec![item(&a), item(&b)],
            }),
        },
    );
    let job_id = serde_json::from_value(start["job_id"].clone()).unwrap();
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![item(&b).id],
            confirmed: true,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = call(&mut api, Command::PollJob { job_id });
        if matches!(
            status["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            assert_ne!(status["state"], "succeeded");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn migration_from_v4_keeps_text_and_creates_links_metadata() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    enabled(&mut api, ShelfTab::Text);
    let old = call(
        &mut api,
        Command::IngestText {
            text: "https://example.test/old".into(),
            source: "test".into(),
        },
    );
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![item(&old).id],
        },
    );
    call(
        &mut api,
        Command::SetPinnedProperties {
            id: item(&old).id,
            display_name: Some("Kept name".into()),
            opener: None,
        },
    );
    drop(api);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE link_fragments; PRAGMA user_version=4;")
        .unwrap();
    drop(db);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Get { id: item(&old).id })["kind"],
        "text_fragment"
    );
    enabled(&mut api, ShelfTab::Links);
    assert_eq!(
        link(&mut api, "https://example.test/new")["kind"],
        "link_fragment"
    );
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        8
    );
    assert_eq!(
        call(&mut api, Command::GetPinnedProperties { id: item(&old).id })["properties"]["display_name"],
        "Kept name"
    );
    assert_eq!(
        call(&mut api, Command::GetPinnedProperties { id: item(&old).id })["properties"]["pinned"],
        true
    );
    assert!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("before-v8"))
    );
}

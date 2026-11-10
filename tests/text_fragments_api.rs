use katran::{Command, Request, SelectionRef, Service, interaction::ShelfTab};
use serde_json::Value;
use std::sync::{Arc, Mutex};

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "text-test".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let reply = reply(api, command);
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
fn enable(api: &mut Service) {
    call(
        api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
}
fn ingest(api: &mut Service, text: &str) -> Value {
    call(
        api,
        Command::IngestText {
            text: text.into(),
            source: "test".into(),
        },
    )
}
fn item(event: &Value) -> SelectionRef {
    SelectionRef {
        id: event["id"].as_i64().unwrap(),
        revision: event["revision"].as_str().unwrap().into(),
    }
}
struct FakeClipboard(Arc<Mutex<Vec<String>>>);
impl katran::text_fragments::ClipboardSink for FakeClipboard {
    fn write(&self, text: &str) -> katran::Result<u32> {
        self.0.lock().unwrap().push(text.to_owned());
        Ok(123)
    }
}

#[test]
fn unicode_preserves_distinct_arrivals_and_survives_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 4_000_000).unwrap();
    let writes = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(FakeClipboard(writes.clone())));
    assert_eq!(
        reply(
            &mut api,
            Command::IngestText {
                text: "x".into(),
                source: "test".into()
            }
        )["ok"],
        false
    );
    enable(&mut api);
    let original = "\n  Привет 🐄\r\n第二 строка  \n";
    let first = ingest(&mut api, original);
    let second = ingest(&mut api, original);
    assert_ne!(first["id"], second["id"]);
    assert_eq!(first["revision"], second["revision"]);
    assert_eq!(first["kind"], "text_fragment");
    assert!(first["name"].as_str().unwrap().ends_with(".txt"));
    assert_eq!(
        call(
            &mut api,
            Command::GetText {
                id: item(&first).id
            }
        )["text"],
        original
    );
    assert!(writes.lock().unwrap().is_empty());
    drop(api);
    let mut api = Service::open(root.path(), 4_000_000).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::GetText {
                id: item(&second).id
            }
        )["text"],
        original
    );
}

#[test]
fn merge_is_ordered_deduplicated_and_all_or_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 4_000_000).unwrap();
    enable(&mut api);
    let a = ingest(&mut api, "один\n");
    let b = ingest(&mut api, "🐄 два");
    let merged = call(
        &mut api,
        Command::MergeText {
            items: vec![item(&b), item(&a), item(&b)],
            separator: " | ".into(),
        },
    );
    assert_eq!(merged["merged"], true);
    assert_eq!(merged["text"], "🐄 два | один\n");
    assert_eq!(merged["items"].as_array().unwrap().len(), 2);
    let mut stale = item(&a);
    stale.revision = "blob:stale".into();
    let file = call(
        &mut api,
        Command::IngestBytes {
            name: "other.bin".into(),
            bytes: vec![1],
            source: "test".into(),
        },
    );
    let failed = call(
        &mut api,
        Command::MergeText {
            items: vec![item(&b), stale, item(&file)],
            separator: "\n".into(),
        },
    );
    assert_eq!(failed["merged"], false);
    assert!(failed.get("text").is_none());
    assert_eq!(failed["items"][0]["ok"], true);
    assert_eq!(failed["items"][1]["ok"], false);
    assert_eq!(failed["items"][2]["ok"], false);
}

#[test]
fn explicit_copy_only_after_success_and_limits_are_checked() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 4_000_000).unwrap();
    enable(&mut api);
    let writes = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(FakeClipboard(writes.clone())));
    assert_eq!(
        reply(
            &mut api,
            Command::IngestText {
                text: String::new(),
                source: "test".into()
            }
        )["ok"],
        false
    );
    assert_eq!(
        reply(
            &mut api,
            Command::IngestText {
                text: "x".repeat(1_048_577),
                source: "test".into()
            }
        )["ok"],
        false
    );
    let a = ingest(&mut api, "A");
    let b = ingest(&mut api, "B");
    let mut stale = item(&a);
    stale.revision = "stale".into();
    call(
        &mut api,
        Command::CopyText {
            items: vec![item(&b), stale],
            separator: "\n".into(),
        },
    );
    assert!(writes.lock().unwrap().is_empty());
    call(
        &mut api,
        Command::CopyText {
            items: vec![item(&b), item(&a)],
            separator: ":".into(),
        },
    );
    assert_eq!(*writes.lock().unwrap(), vec!["B:A"]);
    assert_eq!(
        reply(
            &mut api,
            Command::MergeText {
                items: vec![item(&a)],
                separator: "x".repeat(4097)
            }
        )["ok"],
        false
    );
}

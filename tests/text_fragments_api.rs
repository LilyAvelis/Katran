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
fn obsidian_draft_applies_repeatedly_when_opening_text_preview() {
    use katran::open_file::{Launcher, Operation};
    use std::{fs, path::Path};

    struct FakeLauncher(Arc<Mutex<Vec<String>>>);
    impl Launcher for FakeLauncher {
        fn launch(&self, path: &Path, operation: Operation) -> katran::Result<()> {
            assert_eq!(operation, Operation::Open);
            self.0.lock().unwrap().push(path.to_string_lossy().into());
            Ok(())
        }
    }

    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 4_000_000).unwrap();
    let launches = Arc::new(Mutex::new(Vec::new()));
    api.set_file_launcher(Box::new(FakeLauncher(launches.clone())));
    enable(&mut api);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    let original = ingest(&mut api, "Original note");
    let id = item(&original).id;
    call(&mut api, Command::TogglePins { ids: vec![id] });
    call(
        &mut api,
        Command::SetPinnedTags {
            id,
            tags: vec!["Preserved".into()],
        },
    );

    let draft = call(
        &mut api,
        Command::OpenFile {
            id,
            operation: Operation::Open,
        },
    );
    let path = Path::new(draft["draft_path"].as_str().unwrap());
    assert!(
        path.canonicalize()
            .unwrap()
            .starts_with(root.path().join("Katran Text").canonicalize().unwrap())
    );
    assert_eq!(fs::read_to_string(path).unwrap(), "Original note");
    let launched = launches.lock().unwrap()[0].clone();
    let uri = url::Url::parse(&launched).unwrap();
    assert_eq!(uri.scheme(), "obsidian");
    assert_eq!(uri.query_pairs().next().unwrap().1, path.to_string_lossy());

    fs::write(path, "# Edited note\nПривет 🐳").unwrap();
    call(
        &mut api,
        Command::OpenFile {
            id,
            operation: Operation::Open,
        },
    );
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "# Edited note\nПривет 🐳"
    );
    let saved = call(&mut api, Command::ApplyTextDraft { id });
    assert_eq!(saved["id"], id);
    assert_eq!(saved["text_draft_applied"], true);
    assert_ne!(saved["revision"], original["revision"]);
    assert_eq!(
        call(&mut api, Command::GetText { id })["text"],
        "# Edited note\nПривет 🐳"
    );
    let updated = call(&mut api, Command::GetPinnedProperties { id });
    assert_eq!(updated["properties"]["pinned"], true);
    assert_eq!(updated["tags"][0]["display"], "Preserved");

    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: katran::interaction::SelectionMode::Replace,
        },
    );
    let open_with_space = |api: &mut Service| {
        call(
            api,
            Command::RunAction {
                id: "preview".into(),
            },
        )
    };
    fs::write(path, "Second edits in the same note").unwrap();
    let preview = open_with_space(&mut api);
    assert_eq!(preview["preview"], id);
    assert_eq!(preview["text_draft_applied"], true);
    fs::write(path, "Third edit").unwrap();
    let close = open_with_space(&mut api);
    assert!(close["preview"].is_null());
    assert_eq!(
        call(&mut api, Command::GetText { id })["text"],
        "Second edits in the same note"
    );
    assert_eq!(open_with_space(&mut api)["text_draft_applied"], true);
    assert_eq!(
        call(&mut api, Command::GetText { id })["text"],
        "Third edit"
    );
    open_with_space(&mut api);
    fs::write(path, [255]).unwrap();
    let preview = open_with_space(&mut api);
    assert_eq!(preview["preview"], id);
    assert_eq!(preview["text_draft_warning"]["code"], "invalid_text_draft");
    assert_eq!(
        call(&mut api, Command::GetText { id })["text"],
        "Third edit"
    );
    assert_eq!(fs::read(path).unwrap(), [255]);
}

#[test]
fn unicode_reuses_exact_arrival_and_preserves_metadata_after_restart() {
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
    let id = item(&first).id;
    call(&mut api, Command::TogglePins { ids: vec![id] });
    call(
        &mut api,
        Command::SetPinnedProperties {
            id,
            display_name: Some("Kept name".into()),
            opener: None,
        },
    );
    call(
        &mut api,
        Command::SetPinnedTags {
            id,
            tags: vec!["Kept tag".into()],
        },
    );
    let second = ingest(&mut api, original);
    assert_eq!(first["id"], second["id"]);
    assert_eq!(first["revision"], second["revision"]);
    assert_ne!(first["id"], ingest(&mut api, &format!("{original} "))["id"]);
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
    assert_eq!(ingest(&mut api, original)["id"], id);
    let properties = call(&mut api, Command::GetPinnedProperties { id });
    assert_eq!(properties["properties"]["pinned"], true);
    assert_eq!(properties["properties"]["display_name"], "Kept name");
    assert_eq!(properties["tags"][0]["display"], "Kept tag");
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

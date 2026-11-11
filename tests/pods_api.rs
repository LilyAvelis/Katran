use katran::{Command, Request, SelectionRef, Service, interaction::ShelfTab};
use serde_json::Value;
use std::sync::{Arc, Mutex};

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "pods".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let r = reply(api, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
fn ingest(api: &mut Service, text: &str) -> Value {
    call(
        api,
        Command::IngestText {
            text: text.into(),
            source: "synthetic".into(),
        },
    )
}
fn browse(api: &mut Service, tab: ShelfTab) -> Vec<i64> {
    call(api, Command::SetTab { tab });
    call(
        api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    )["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect()
}
struct Sink(Arc<Mutex<Vec<String>>>);
impl katran::text_fragments::ClipboardSink for Sink {
    fn write(&self, text: &str) -> katran::Result<u32> {
        self.0.lock().unwrap().push(text.into());
        Ok(77)
    }
}

#[test]
fn pods_and_text_are_separate_deduplicated_pinned_and_persistent() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let disabled = reply(
        &mut api,
        Command::IngestText {
            text: "🐄".into(),
            source: "test".into(),
        },
    );
    assert_eq!(disabled["error"]["code"], "tab_disabled");
    let ordinary = ingest(&mut api, "hello 🐄");
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    let first = ingest(&mut api, "👨‍👩‍👧‍👦");
    let id = first["id"].as_i64().unwrap();
    call(&mut api, Command::TogglePins { ids: vec![id] });
    let second = ingest(&mut api, "👨‍👩‍👧‍👦");
    assert_eq!(second["id"], id);
    assert_eq!(
        call(&mut api, Command::GetPinnedProperties { id })["properties"]["pinned"],
        true
    );
    assert_eq!(browse(&mut api, ShelfTab::Pods), vec![id]);
    assert_eq!(
        browse(&mut api, ShelfTab::Text),
        vec![ordinary["id"].as_i64().unwrap()]
    );
    let copied = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(Sink(copied.clone())));
    let result = call(
        &mut api,
        Command::CopyText {
            items: vec![SelectionRef {
                id,
                revision: first["revision"].as_str().unwrap().into(),
            }],
            separator: "".into(),
        },
    );
    assert_eq!(result["clipboard_sequence"], 77);
    assert_eq!(copied.lock().unwrap().as_slice(), &["👨‍👩‍👧‍👦"]);
    drop(api);
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    assert_eq!(browse(&mut api, ShelfTab::Pods), vec![id]);
    assert_eq!(call(&mut api, Command::GetText { id })["text"], "👨‍👩‍👧‍👦");
    assert_eq!(
        call(&mut api, Command::GetPinnedProperties { id })["properties"]["pinned"],
        true
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: false,
        },
    );
    assert_eq!(
        reply(
            &mut api,
            Command::IngestText {
                text: "🐄".into(),
                source: "test".into()
            }
        )["error"]["code"],
        "tab_disabled"
    );
    assert_eq!(
        browse(&mut api, ShelfTab::Text),
        vec![ordinary["id"].as_i64().unwrap()]
    );
    assert_eq!(call(&mut api, Command::GetText { id })["text"], "👨‍👩‍👧‍👦");
}

#[test]
fn concurrent_prepared_duplicates_preserve_pinned_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    let initial = ingest(&mut api, "👍🏽");
    let id = initial["id"].as_i64().unwrap();
    call(&mut api, Command::TogglePins { ids: vec![id] });
    let mut jobs = Vec::new();
    for _ in 0..2 {
        let job = call(
            &mut api,
            Command::BeginJob {
                command: Box::new(Command::IngestText {
                    text: "👍🏽".into(),
                    source: "synthetic".into(),
                }),
            },
        );
        jobs.push(serde_json::from_value(job["job_id"].clone()).unwrap());
    }
    for job_id in jobs {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let state = call(&mut api, Command::PollJob { job_id });
            if state["state"] == "succeeded" {
                assert_eq!(state["result"]["id"], id);
                break;
            }
            assert_ne!(state["state"], "failed", "{state}");
            assert!(std::time::Instant::now() < deadline, "{state}");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    assert_eq!(browse(&mut api, ShelfTab::Pods), vec![id]);
    assert_eq!(
        call(&mut api, Command::GetPinnedProperties { id })["properties"]["pinned"],
        true
    );
    assert_eq!(
        call(&mut api, Command::Get { id })["revision"],
        initial["revision"]
    );
}

#[test]
fn pinned_tag_names_and_filter_remain_category_local() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    for tab in [ShelfTab::Text, ShelfTab::Pods] {
        call(&mut api, Command::SetTabEnabled { tab, enabled: true });
    }
    let text_id = ingest(&mut api, "ordinary text 🐄")["id"].as_i64().unwrap();
    let pod_id = ingest(&mut api, "🐄")["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![text_id, pod_id],
        },
    );
    call(
        &mut api,
        Command::SetPinnedTags {
            id: text_id,
            tags: vec!["Text tag".into(), "Shared".into()],
        },
    );
    call(
        &mut api,
        Command::SetPinnedTags {
            id: pod_id,
            tags: vec!["Pod tag".into(), "Shared".into()],
        },
    );
    for (tab, only, expected, excluded) in [
        (ShelfTab::Pods, "pod tag", pod_id, "text tag"),
        (ShelfTab::Text, "text tag", text_id, "pod tag"),
    ] {
        call(&mut api, Command::SetTab { tab });
        let tags = call(&mut api, Command::PinnedTags);
        let keys: Vec<_> = tags["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["key"].as_str().unwrap())
            .collect();
        assert!(keys.contains(&only), "{tags}");
        assert!(keys.contains(&"shared"), "{tags}");
        assert!(!keys.contains(&excluded), "{tags}");
        call(
            &mut api,
            Command::FilterPinnedTag {
                tag: Some("SHARED".into()),
            },
        );
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 100,
                cursor: None,
            },
        );
        let events = page["events"].as_array().unwrap();
        assert_eq!(events.len(), 1, "{page}");
        assert_eq!(events[0]["id"], expected);
    }
}

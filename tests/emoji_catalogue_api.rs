use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
fn call(api: &mut Service, command: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "emoji".into(),
        command,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn full_catalogue_is_read_only_and_copies_selected_entries_to_pods() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1000000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let first = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 1000,
            cursor: None,
        },
    );
    assert_eq!(first["total"], 3781);
    assert_eq!(first["events"][0]["text_preview"], "😀");
    assert_eq!(first["events"][0]["category"]["group"], "Smileys & Emotion");
    assert_eq!(first["dividers"][0]["level"], "group");
    assert_eq!(first["dividers"][0]["before"], 0);
    assert_eq!(first["dividers"][1]["level"], "subgroup");
    let mut cursor = first["next_cursor"].as_str().map(str::to_owned);
    let mut total = first["events"].as_array().unwrap().len();
    while let Some(c) = cursor {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 1000,
                cursor: Some(c),
            },
        );
        total += page["events"].as_array().unwrap().len();
        cursor = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(total as u64, first["total"].as_u64().unwrap());
    let found = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "cow face".into(),
            limit: 100,
            cursor: None,
        },
    );
    let events = found["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["text_preview"] == "🐮"));
    let scotland = "🏴\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}";
    let rare = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: scotland.into(),
            limit: 10,
            cursor: None,
        },
    );
    assert!(
        rare["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["text_preview"] == scotland)
    );
    assert!(katran::pods::is_pod(scotland));
    let two: Vec<i64> = first["events"]
        .as_array()
        .unwrap()
        .iter()
        .take(2)
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    let description = call(&mut api, Command::DescribePod { id: two[0] });
    assert_eq!(description["text"], first["events"][0]["text_preview"]);
    call(&mut api, Command::SetVisible { ids: two.clone() });
    for (n, id) in two.iter().enumerate() {
        call(
            &mut api,
            Command::Select {
                id: *id,
                mode: if n == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
    for command in [
        Command::DeleteSelection {
            ids: two.clone(),
            confirmed: true,
        },
        Command::DeleteEvents {
            ids: two.clone(),
            confirmed: true,
        },
    ] {
        let response = api.execute(Request {
            api_version: 1,
            request_id: "read-only".into(),
            command,
        });
        assert_eq!(response["error"]["code"], "read_only", "{response}");
    }
    let pin = api.execute(Request {
        api_version: 1,
        request_id: "read-only-pin".into(),
        command: Command::TogglePins { ids: two.clone() },
    });
    assert_eq!(pin["error"]["code"], "read_only");
    let result = call(&mut api, Command::CopyEmojiSelectionToPods);
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["ok"] == true),
        "{result}"
    );
    let check = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 1000,
            cursor: None,
        },
    );
    assert_eq!(check["total"], first["total"]);
    call(&mut api, Command::SetEmojiCatalogue { enabled: false });
    let saved = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(saved["events"].as_array().unwrap().len(), 2, "{saved}");
}

#[test]
fn categories_are_not_selected_and_page_boundaries_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1_000_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    let mut groups = std::collections::HashSet::new();
    let mut subgroups = 0;
    let mut total = 0;
    loop {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 7,
                cursor,
            },
        );
        let events = page["events"].as_array().unwrap();
        for item in events {
            let text = item["text_preview"].as_str().unwrap();
            let stripped = text.replace('\u{fe0f}', "");
            assert!(seen.insert(stripped), "duplicate presentation: {text}");
            total += 1;
        }
        for divider in page["dividers"].as_array().unwrap() {
            assert_eq!(divider["type"], "category_divider");
            assert!(divider["before"].as_u64().unwrap() < events.len() as u64);
            assert!(divider.get("id").is_none());
            if divider["level"] == "group" {
                groups.insert(divider["label"].as_str().unwrap().to_owned());
            } else {
                subgroups += 1;
            }
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(total, 3781);
    assert_eq!(groups.len(), 9);
    assert_eq!(subgroups, 98);
}

#[test]
fn range_selection_and_search_cross_category_dividers() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1_000_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "face".into(),
            limit: 1000,
            cursor: None,
        },
    );
    let events = page["events"].as_array().unwrap();
    let boundary = (1..events.len())
        .find(|&i| events[i]["category"]["subgroup"] != events[i - 1]["category"]["subgroup"])
        .unwrap();
    let ids: Vec<i64> = events.iter().map(|e| e["id"].as_i64().unwrap()).collect();
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    call(
        &mut api,
        Command::Select {
            id: ids[boundary - 1],
            mode: SelectionMode::Replace,
        },
    );
    let selection = call(
        &mut api,
        Command::Select {
            id: ids[boundary],
            mode: SelectionMode::Range,
        },
    );
    assert_eq!(selection["selected"].as_array().unwrap().len(), 2);
    let copied = call(&mut api, Command::CopyEmojiSelectionToPods);
    assert_eq!(copied["items"].as_array().unwrap().len(), 2);
    assert!(
        copied["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["ok"] == true)
    );
}

#[test]
fn category_filter_is_validated_and_pagination_is_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1_000_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let categories = call(&mut api, Command::EmojiCategories);
    assert_eq!(categories["groups"].as_array().unwrap().len(), 9);
    let full = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: None,
        },
    );
    let stale_cursor = full["next_cursor"].as_str().unwrap().to_owned();
    let id = full["events"][0]["id"].as_i64().unwrap();
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let selected = call(
        &mut api,
        Command::SetEmojiCategory {
            group: Some("Animals & Nature".into()),
        },
    );
    assert_eq!(selected["emoji_category"], "Animals & Nature");
    assert!(selected["selected"].as_array().unwrap().is_empty());
    let bad = api.execute(Request {
        api_version: 1,
        request_id: "stale-page".into(),
        command: Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: Some(stale_cursor),
        },
    });
    assert_eq!(bad["error"]["code"], "invalid_cursor", "{bad}");
    let mut cursor = None;
    let mut count = 0;
    loop {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 7,
                cursor,
            },
        );
        let entries = page["events"].as_array().unwrap();
        assert!(
            entries
                .iter()
                .all(|e| e["category"]["group"] == "Animals & Nature")
        );
        count += entries.len();
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(count as u64, page["total"].as_u64().unwrap());
            break;
        }
    }
    assert!(count > 100);
    let search = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "cat".into(),
            limit: 100,
            cursor: None,
        },
    );
    assert!(search["total"].as_u64().unwrap() > 0);
    assert!(
        search["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["category"]["group"] == "Animals & Nature")
    );
    let invalid = api.execute(Request {
        api_version: 1,
        request_id: "invalid-group".into(),
        command: Command::SetEmojiCategory {
            group: Some("not real".into()),
        },
    });
    assert_eq!(invalid["error"]["code"], "invalid_category");
    call(
        &mut api,
        Command::SetEmojiCategory {
            group: Some("People & Body".into()),
        },
    );
    let mut cursor = None;
    let mut people = std::collections::HashSet::new();
    loop {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 1000,
                cursor,
            },
        );
        for event in page["events"].as_array().unwrap() {
            assert_eq!(event["category"]["group"], "People & Body");
            assert!(people.insert(event["id"].as_i64().unwrap()));
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(people.len() as u64, page["total"].as_u64().unwrap());
            break;
        }
    }
    assert_eq!(people.len(), 2261);
    call(&mut api, Command::SetEmojiCategory { group: None });
    let reset = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: None,
        },
    );
    assert_eq!(reset["total"], 3781);
}

#[derive(Default)]
struct OutputState {
    text: String,
    sequence: u32,
    sent: usize,
    drag_text: Option<String>,
    drag_files: Vec<String>,
    fail_copy: bool,
}
struct Clipboard(std::sync::Arc<std::sync::Mutex<OutputState>>);
impl katran::text_fragments::ClipboardSink for Clipboard {
    fn write(&self, text: &str) -> katran::Result<u32> {
        let mut state = self.0.lock().unwrap();
        if state.fail_copy {
            return Err(katran::ApiError {
                code: "synthetic_copy_failure".into(),
                message: "clipboard unavailable".into(),
            });
        }
        state.text = text.into();
        state.sequence = 42;
        Ok(42)
    }
}
struct Overlay(std::sync::Arc<std::sync::Mutex<OutputState>>);
impl katran::overlay::Host for Overlay {
    fn foreground(&mut self) -> katran::overlay::Result<u64> {
        Ok(7)
    }
    fn identity(&mut self, hwnd: u64) -> katran::overlay::Result<katran::overlay::Target> {
        Ok(katran::overlay::Target {
            hwnd,
            pid: hwnd as u32,
            process_started: hwnd,
        })
    }
    fn work_area(&mut self, _: u64) -> katran::overlay::Result<katran::overlay::WorkArea> {
        Ok(katran::overlay::WorkArea {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        })
    }
    fn wait_modifiers_released(&mut self) -> katran::overlay::Result<()> {
        Ok(())
    }
    fn activate(&mut self, _: u64) -> katran::overlay::Result<()> {
        Ok(())
    }
    fn send_paste(
        &mut self,
        _: katran::overlay::Target,
        sequence: u32,
    ) -> katran::overlay::Result<()> {
        let mut state = self.0.lock().unwrap();
        assert_eq!(sequence, state.sequence);
        state.sent += 1;
        Ok(())
    }
}
struct Drag(std::sync::Arc<std::sync::Mutex<OutputState>>);
impl katran::drag_out::Runner for Drag {
    fn run(
        &mut self,
        _: &[std::path::PathBuf],
        _: Option<&katran::drag_out::Preview>,
        _: u64,
    ) -> Result<katran::drag_out::Outcome, String> {
        panic!("drag must include Unicode text")
    }
    fn run_with_text(
        &mut self,
        paths: &[std::path::PathBuf],
        _: Option<&katran::drag_out::Preview>,
        _: u64,
        text: Option<&str>,
    ) -> Result<katran::drag_out::Outcome, String> {
        let mut state = self.0.lock().unwrap();
        state.drag_text = text.map(str::to_owned);
        state.drag_files = paths
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        Ok(katran::drag_out::Outcome {
            dropped: true,
            warnings: vec![],
        })
    }
}
#[test]
fn copy_action_imports_catalogue_selection_and_reports_clipboard_outcomes() {
    use std::sync::{Arc, Mutex};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let state = Arc::new(Mutex::new(OutputState::default()));
    api.set_text_clipboard_sink(Box::new(Clipboard(state.clone())));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 2,
            cursor: None,
        },
    );
    let events = page["events"].as_array().unwrap();
    let ids: Vec<i64> = events.iter().map(|e| e["id"].as_i64().unwrap()).collect();
    let expected: String = events
        .iter()
        .map(|e| e["text_preview"].as_str().unwrap())
        .collect();
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    for (index, id) in ids.iter().enumerate() {
        call(
            &mut api,
            Command::Select {
                id: *id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
    let result = call(&mut api, Command::RunAction { id: "copy".into() });
    assert_eq!(result["emoji_copy"], true);
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["ok"] == true)
    );
    assert_eq!(result["clipboard"]["status"], "copied");
    assert_eq!(state.lock().unwrap().text, expected);

    state.lock().unwrap().fail_copy = true;
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::RunAction { id: "copy".into() }),
        },
    );
    assert_eq!(job["state"], "succeeded");
    let result = job["result"].clone();
    assert_eq!(result["emoji_copy"], true);
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["ok"] == true)
    );
    assert_eq!(result["clipboard"]["status"], "failed");
    assert_eq!(
        result["clipboard"]["error"]["code"],
        "synthetic_copy_failure"
    );
    assert_eq!(state.lock().unwrap().text, expected);
    call(&mut api, Command::SetEmojiCatalogue { enabled: false });
    let saved = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let saved_texts: Vec<_> = saved["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["text_preview"].as_str())
        .collect();
    for event in events {
        assert!(saved_texts.contains(&event["text_preview"].as_str().unwrap()));
    }
}

#[test]
fn copy_action_reports_partial_import_and_skips_clipboard() {
    use std::sync::{Arc, Mutex};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 4).unwrap();
    let state = Arc::new(Mutex::new(OutputState::default()));
    api.set_text_clipboard_sink(Box::new(Clipboard(state.clone())));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 2,
            cursor: None,
        },
    );
    let ids: Vec<_> = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["id"].as_i64().unwrap())
        .collect();
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    for (index, id) in ids.into_iter().enumerate() {
        call(
            &mut api,
            Command::Select {
                id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
    let result = call(&mut api, Command::CopySelectionAction);
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items.iter().filter(|item| item["ok"] == true).count(),
        1,
        "{result}"
    );
    assert_eq!(
        items
            .iter()
            .filter(|item| item["error"]["code"] == "quota_exceeded")
            .count(),
        1
    );
    assert_eq!(result["clipboard"]["status"], "skipped");
    assert!(state.lock().unwrap().text.is_empty());
}

#[test]
fn catalogue_copy_paste_and_drag_share_exact_unicode_selection() {
    use katran::{
        SelectionRef,
        overlay::{SummonReceipt, Target, WorkArea},
    };
    use std::sync::{Arc, Mutex};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let state = Arc::new(Mutex::new(OutputState::default()));
    api.set_text_clipboard_sink(Box::new(Clipboard(state.clone())));
    api.set_overlay_host(Box::new(Overlay(state.clone())));
    api.set_drag_runner(Box::new(Drag(state.clone())));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 2,
            cursor: None,
        },
    );
    let items: Vec<SelectionRef> = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| SelectionRef {
            id: event["id"].as_i64().unwrap(),
            revision: event["revision"].as_str().unwrap().into(),
        })
        .collect();
    let expected: String = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["text_preview"].as_str().unwrap())
        .collect();
    call(
        &mut api,
        Command::SetVisible {
            ids: items.iter().map(|e| e.id).collect(),
        },
    );
    for (index, item) in items.iter().enumerate() {
        call(
            &mut api,
            Command::Select {
                id: item.id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
    let copied = call(&mut api, Command::CopySelectedObjects);
    assert_eq!(copied["text_copied"], true);
    assert_eq!(state.lock().unwrap().text, expected);
    call(
        &mut api,
        Command::SummonCapturedOverlay {
            owner_window: 8,
            capture: SummonReceipt {
                previous: Target {
                    hwnd: 7,
                    pid: 7,
                    process_started: 7,
                },
                work_area: WorkArea {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                },
            },
        },
    );
    call(&mut api, Command::BeginPastePrevious { owner_window: 8 });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let result = call(&mut api, Command::CompletePastePrevious { owner_window: 8 });
        if result["state"] != "copying" {
            assert_eq!(result["state"], "sent");
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(state.lock().unwrap().sent, 1);
    assert_eq!(state.lock().unwrap().text, expected);
    let ready = call(
        &mut api,
        Command::DragEventsChecked {
            items: items.clone(),
        },
    );
    call(
        &mut api,
        Command::RunPreparedDrag {
            token: ready["drag_token"].as_str().unwrap().into(),
            owner_window: 0,
        },
    );
    let result = state.lock().unwrap();
    assert_eq!(result.drag_text.as_deref(), Some(expected.as_str()));
    assert_eq!(result.drag_files, vec![expected]);
    drop(result);
    // A failed clipboard publication must never send a paste to the target.
    state.lock().unwrap().fail_copy = true;
    call(
        &mut api,
        Command::SummonCapturedOverlay {
            owner_window: 8,
            capture: SummonReceipt {
                previous: Target {
                    hwnd: 7,
                    pid: 7,
                    process_started: 7,
                },
                work_area: WorkArea {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                },
            },
        },
    );
    call(&mut api, Command::BeginPastePrevious { owner_window: 8 });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let response = api.execute(Request {
            api_version: 1,
            request_id: "failed-paste".into(),
            command: Command::CompletePastePrevious { owner_window: 8 },
        });
        if response["result"]["state"] != "copying" {
            assert_eq!(response["error"]["code"], "copy_failed");
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(state.lock().unwrap().sent, 1);
    state.lock().unwrap().fail_copy = false;
    let previous_text = state.lock().unwrap().text.clone();
    let mut stale = items[0].clone();
    stale.revision = "stale".into();
    let stale_copy = api.execute(Request {
        api_version: 1,
        request_id: "stale-copy".into(),
        command: Command::CopySmart {
            items: vec![stale.clone()],
        },
    });
    assert_eq!(stale_copy["ok"], false);
    assert_eq!(state.lock().unwrap().text, previous_text);
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "stale".into(),
        command: Command::DragEventsChecked { items: vec![stale] },
    });
    assert_eq!(reply["ok"], false);
    let saved = call(&mut api, Command::CopyEmojiSelectionToPods);
    assert!(
        saved["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["ok"] == true)
    );
}

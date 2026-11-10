use katran::{
    Command, Request, SelectionRef, Service,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
struct Sink(Arc<Mutex<Vec<String>>>);
impl katran::text_fragments::ClipboardSink for Sink {
    fn write(&self, s: &str) -> katran::Result<u32> {
        self.0.lock().unwrap().push(s.into());
        Ok(21)
    }
}
fn call(api: &mut Service, command: Command) -> Value {
    let v = api.execute(Request {
        api_version: 1,
        request_id: "copy".into(),
        command,
    });
    assert_eq!(v["ok"], true, "{v}");
    v["result"].clone()
}
#[test]
fn ctrl_order_and_shift_visible_order_drive_plain_text_copy() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let output = Arc::new(Mutex::new(vec![]));
    api.set_text_clipboard_sink(Box::new(Sink(output.clone())));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let mut ids = vec![];
    for s in ["A", "B", "C"] {
        ids.push(
            call(
                &mut api,
                Command::IngestText {
                    text: s.into(),
                    source: "test".into(),
                },
            )["id"]
                .as_i64()
                .unwrap(),
        );
    }
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    for (id, mode) in [
        (ids[2], SelectionMode::Replace),
        (ids[0], SelectionMode::Toggle),
        (ids[1], SelectionMode::Toggle),
    ] {
        call(&mut api, Command::Select { id, mode });
    }
    call(&mut api, Command::RunAction { id: "copy".into() });
    assert_eq!(output.lock().unwrap().last().unwrap(), "C\n\nA\n\nB");
    call(
        &mut api,
        Command::Select {
            id: ids[2],
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut api,
        Command::Select {
            id: ids[0],
            mode: SelectionMode::Range,
        },
    );
    call(&mut api, Command::RunAction { id: "copy".into() });
    assert_eq!(output.lock().unwrap().last().unwrap(), "A\n\nB\n\nC");
    call(
        &mut api,
        Command::SetVisible {
            ids: vec![ids[1], ids[2], ids[0]],
        },
    );
    call(&mut api, Command::RunAction { id: "copy".into() });
    assert_eq!(output.lock().unwrap().last().unwrap(), "B\n\nC\n\nA");
    call(
        &mut api,
        Command::Select {
            id: ids[0],
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut api,
        Command::Select {
            id: ids[2],
            mode: SelectionMode::Toggle,
        },
    );
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    call(&mut api, Command::RunAction { id: "copy".into() });
    assert_eq!(output.lock().unwrap().last().unwrap(), "A\n\nC");
    let items: Vec<SelectionRef> = ids
        .iter()
        .map(|id| SelectionRef {
            id: *id,
            revision: call(&mut api, Command::Get { id: *id })["revision"]
                .as_str()
                .unwrap()
                .into(),
        })
        .collect();
    let drag = call(&mut api, Command::DragEventsChecked { items });
    assert_eq!(drag["count"], 3);
    call(
        &mut api,
        Command::CancelPreparedDrag {
            token: drag["drag_token"].as_str().unwrap().into(),
        },
    );
    assert_eq!(output.lock().unwrap().len(), 4);
}
#[test]
fn existing_markdown_file_is_read_without_becoming_a_text_fragment() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("note.md");
    std::fs::write(&file, "# Заголовок\n\n**слово**").unwrap();
    let mut api = Service::open(&root.path().join("shelf"), 10000).unwrap();
    let e = call(
        &mut api,
        Command::IngestPath {
            path: file.clone(),
            source: "test".into(),
        },
    );
    let id = e["id"].as_i64().unwrap();
    assert_eq!(e["kind"], "file_reference");
    assert_eq!(
        call(&mut api, Command::GetDocumentText { id })["text"],
        "# Заголовок\n\n**слово**"
    );
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    assert_eq!(call(&mut api, Command::TogglePreview)["preview"], id);
    std::fs::write(file, "changed").unwrap();
    let r = api.execute(Request {
        api_version: 1,
        request_id: "changed".into(),
        command: Command::GetDocumentText { id },
    });
    assert_eq!(r["error"]["code"], "source_changed");
}

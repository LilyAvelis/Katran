use katran::{Command, Request, Service, interaction::SelectionMode};
use serde_json::Value;
fn call(s: &mut Service, c: Command) -> Value {
    s.execute(Request {
        api_version: 1,
        request_id: "test".into(),
        command: c,
    })["result"]
        .clone()
}
#[test]
fn selection_actions_and_preview_via_api() {
    let root = tempfile::tempdir().unwrap();
    let mut s = Service::open(root.path(), 10000).unwrap();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(400, 200)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let e = call(
        &mut s,
        Command::IngestBytes {
            name: "test.png".into(),
            bytes: bytes.into_inner(),
            source: "test".into(),
        },
    );
    let id = e["id"].as_i64().unwrap();
    call(
        &mut s,
        Command::SetVisible {
            ids: vec![id, 20, 30, 40],
        },
    );
    call(
        &mut s,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let state = call(
        &mut s,
        Command::Navigate {
            delta: 2,
            mode: SelectionMode::Range,
        },
    );
    assert_eq!(state["selected"], serde_json::json!([id, 20, 30]));
    let state = call(
        &mut s,
        Command::Select {
            id: 40,
            mode: SelectionMode::Toggle,
        },
    );
    assert_eq!(state["selected"].as_array().unwrap().len(), 4);
    let state = call(
        &mut s,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    assert_eq!(state["selected"], serde_json::json!([id]));
    assert_eq!(
        call(
            &mut s,
            Command::RunAction {
                id: "preview".into()
            }
        )["preview"],
        id
    );
    assert_eq!(
        call(&mut s, Command::RunAction { id: "list".into() })["view"],
        "list"
    );
    let p = call(&mut s, Command::PreviewImage { id, max_edge: 128 });
    assert_eq!(p["width"], 128);
    assert_eq!(p["height"], 64);
    assert_eq!(
        call(&mut s, Command::RunAction { id: "save".into() })["ids"],
        serde_json::json!([id])
    );
}

#[test]
fn clear_selection_preserves_cursor_and_resets_range_anchor_via_api() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    call(
        &mut api,
        Command::SetVisible {
            ids: vec![10, 20, 30, 40],
        },
    );
    call(
        &mut api,
        Command::Select {
            id: 10,
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut api,
        Command::Select {
            id: 30,
            mode: SelectionMode::Range,
        },
    );
    let state = call(&mut api, Command::ClearSelection);
    assert_eq!(state["selected"], serde_json::json!([]));
    assert_eq!(state["cursor"], 30);
    assert_eq!(state["visible"], serde_json::json!([10, 20, 30, 40]));
    let next = call(
        &mut api,
        Command::Select {
            id: 40,
            mode: SelectionMode::Range,
        },
    );
    assert_eq!(next["selected"], serde_json::json!([30, 40]));
}

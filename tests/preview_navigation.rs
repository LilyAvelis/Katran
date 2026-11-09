use katran::{Command, Request, Service};
fn call(s: &mut Service, c: Command) -> serde_json::Value {
    s.execute(Request {
        api_version: 1,
        request_id: "preview-nav".into(),
        command: c,
    })["result"]
        .clone()
}

#[test]
fn drag_rejects_empty_or_stale_selection_before_native_adapter() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "synthetic.png".into(),
            bytes: vec![1],
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    for (items, expected) in [
        (vec![], "invalid_argument"),
        (
            vec![katran::SelectionRef {
                id,
                revision: "obsolete".into(),
            }],
            "stale_selection",
        ),
    ] {
        let response = api.execute(Request {
            api_version: 1,
            request_id: "drag-validation".into(),
            command: Command::DragEventsChecked { items },
        });
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], expected);
    }
    assert!(!root.path().join("drag-cache").exists());
}
#[test]
fn space_toggle_and_preview_arrows_share_domain_state() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    for n in 0..3 {
        call(
            &mut api,
            Command::IngestBytes {
                name: format!("image{n}.png"),
                bytes: vec![n],
                source: "test".into(),
            },
        );
    }
    call(&mut api, Command::SetVisible { ids: vec![3, 2, 1] });
    assert_eq!(call(&mut api, Command::TogglePreview)["preview"], 3);
    assert_eq!(
        call(&mut api, Command::NavigatePreview { delta: 1 })["preview"],
        2
    );
    assert_eq!(
        call(&mut api, Command::NavigatePreview { delta: -1 })["preview"],
        3
    );
    assert_eq!(
        call(&mut api, Command::NavigatePreview { delta: 20 })["preview"],
        1
    );
    assert!(call(&mut api, Command::TogglePreview)["preview"].is_null());
}

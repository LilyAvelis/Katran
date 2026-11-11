use katran::{Command, Request, Service};
use serde_json::{Value, json};

fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "enter-shortcuts".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
fn binding(api: &mut Service, id: &str) -> Value {
    call(api, Command::Shortcuts)
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap()["binding"]
        .clone()
}
#[test]
fn new_defaults_and_legacy_comma_migration_preserve_other_overrides() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1000).unwrap();
    assert_eq!(binding(&mut api, "paste_previous"), "Enter");
    assert_eq!(binding(&mut api, "open"), "Alt Enter");
    drop(api);
    std::fs::write(root.path().join("shortcuts.json"), serde_json::to_vec(&json!({"overrides":{"paste_previous":"Comma","open":"Enter","capture":"S","gallery":"Ctrl 1","list":"Ctrl 2"}})).unwrap()).unwrap();
    let mut api = Service::open(root.path(), 1000).unwrap();
    assert_eq!(binding(&mut api, "paste_previous"), "Enter");
    assert_eq!(binding(&mut api, "open"), "Alt Enter");
    assert_eq!(binding(&mut api, "capture"), "S");
    assert_eq!(binding(&mut api, "gallery"), "Ctrl 1");
    assert_eq!(binding(&mut api, "list"), "Ctrl 2");
    call(
        &mut api,
        Command::SetShortcut {
            id: "paste_previous".into(),
            binding: Some("Comma".into()),
            replace: false,
            reset: false,
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 1000).unwrap();
    assert_eq!(binding(&mut api, "paste_previous"), "Comma");
    assert_eq!(binding(&mut api, "open"), "Alt Enter");
    let saved: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("shortcuts.json")).unwrap())
            .unwrap();
    assert_eq!(saved["version"], 1);
}
#[test]
fn migration_respects_disabled_custom_and_conflicting_assignments() {
    for (fixture, expected_paste, expected_open) in [
        (
            json!({"paste_previous":null,"open":"F8"}),
            json!(null),
            json!("F8"),
        ),
        (
            json!({"paste_previous":"Alt Enter"}),
            json!("Enter"),
            json!("Alt Enter"),
        ),
        (
            json!({"paste_previous":"Ctrl P","gallery":"Enter"}),
            json!("Ctrl P"),
            json!("Alt Enter"),
        ),
        (
            json!({"gallery":"Enter","list":"Alt Enter"}),
            json!(null),
            json!(null),
        ),
        (
            json!({"paste_previous":"Alt Enter / F8"}),
            json!("Alt Enter / F8"),
            json!(null),
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("shortcuts.json"),
            serde_json::to_vec(&json!({"overrides":fixture})).unwrap(),
        )
        .unwrap();
        let mut api = Service::open(root.path(), 1000).unwrap();
        assert_eq!(binding(&mut api, "paste_previous"), expected_paste);
        assert_eq!(binding(&mut api, "open"), expected_open);
    }
}

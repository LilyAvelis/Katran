use katran::{Command, Request, Service};
use serde_json::{Value, json};

#[test]
fn failed_shortcut_save_preserves_live_binding_and_preferences() {
    let root = tempfile::tempdir().unwrap();
    let mut shortcuts = katran::shortcuts::Shortcuts::default();
    let before = shortcuts.binding("save");
    // Block only the disposable fixture's staging path, without changing ACLs.
    std::fs::create_dir(root.path().join("shortcuts.json.tmp")).unwrap();
    let error = shortcuts
        .set(
            root.path(),
            "save",
            Some("Ctrl Shift H".into()),
            false,
            false,
        )
        .unwrap_err();
    assert_eq!(error.code, "storage_error");
    assert_eq!(shortcuts.binding("save"), before);
    assert!(!root.path().join("shortcuts.json").exists());
}

fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "keys-test".into(),
        command,
    })
}
#[test]
fn dispatcher_shortcuts_persist_and_reserved_keys_do_not_mutate() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::SetShortcut {
                id: "save".into(),
                binding: Some("Ctrl Shift H".into()),
                replace: false,
                reset: false
            }
        )["ok"],
        true
    );
    for binding in [
        "ArrowDown",
        "Shift ArrowRight",
        "Escape",
        "Ctrl K",
        "Ctrl Shift Comma",
    ] {
        assert_eq!(
            call(
                &mut api,
                Command::SetShortcut {
                    id: "save".into(),
                    binding: Some(binding.into()),
                    replace: true,
                    reset: false
                }
            )["error"]["code"],
            "reserved_shortcut"
        );
    }
    let conflict = call(
        &mut api,
        Command::SetShortcut {
            id: "gallery".into(),
            binding: Some("Ctrl Shift H".into()),
            replace: false,
            reset: false,
        },
    );
    assert_eq!(conflict["error"]["code"], "shortcut_conflict");
    assert_eq!(
        call(
            &mut api,
            Command::SetShortcut {
                id: "gallery".into(),
                binding: Some("Ctrl Shift H".into()),
                replace: true,
                reset: false
            }
        )["ok"],
        true
    );
    drop(api);
    let mut api = Service::open(root.path(), 100).unwrap();
    let catalogue = call(&mut api, Command::Shortcuts);
    let rows = catalogue["result"].as_array().unwrap();
    assert_eq!(
        rows.iter().find(|r| r["id"] == "gallery").unwrap()["binding"],
        "Ctrl Shift H"
    );
    assert_eq!(
        rows.iter().find(|r| r["id"] == "save").unwrap()["binding"],
        json!(null)
    );
}

use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;

fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "folder-standard".into(),
        command,
    })
}

#[test]
fn standard_folders_action_is_folder_only_rebindable_and_persistent() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();

    assert_eq!(
        call(
            &mut api,
            Command::SetTab {
                tab: ShelfTab::Images,
            },
        )["ok"],
        true
    );
    assert_eq!(
        call(
            &mut api,
            Command::RunAction {
                id: "folder_standard".into(),
            },
        )["error"]["code"],
        "unknown_action"
    );

    let shortcuts = call(&mut api, Command::Shortcuts);
    let standard = shortcuts["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "folder_standard")
        .unwrap();
    assert_eq!(standard["default_binding"], "Ctrl Y");
    assert_eq!(standard["binding"], "Ctrl Y");
    assert_eq!(standard["rebindable"], true);

    assert_eq!(
        call(&mut api, Command::SetTab { tab: ShelfTab::All },)["ok"],
        true
    );
    assert_eq!(
        call(
            &mut api,
            Command::RunAction {
                id: "folder_standard".into(),
            },
        )["result"]["intent"],
        "folder_standard"
    );

    let changed = call(
        &mut api,
        Command::SetShortcut {
            id: "folder_standard".into(),
            binding: Some("Ctrl+Shift+Y".into()),
            replace: false,
            reset: false,
        },
    );
    assert_eq!(changed["ok"], true, "{changed}");
    drop(api);

    let mut api = Service::open(root.path(), 100_000).unwrap();
    let shortcuts = call(&mut api, Command::Shortcuts);
    let standard = shortcuts["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "folder_standard")
        .unwrap();
    assert_eq!(standard["binding"], "Ctrl Shift Y");
    assert_ne!(standard["binding"], "Ctrl Y");
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    assert_eq!(
        call(
            &mut api,
            Command::RunAction {
                id: "folder_standard".into(),
            },
        )["result"]["intent"],
        "folder_standard"
    );
}

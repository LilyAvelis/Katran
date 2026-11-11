use katran::{Command, Request, SelectionRef, Service};
use serde_json::Value;
#[cfg(windows)]
#[path = "common/source_api.rs"]
mod source_api;

fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "transfer".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn selection(event: &Value) -> SelectionRef {
    SelectionRef {
        id: event["id"].as_i64().unwrap(),
        revision: event["revision"].as_str().unwrap().into(),
    }
}
#[test]
fn pinned_cut_is_copy_and_intent_is_consumed() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1024 * 1024).unwrap();
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "sample.png".into(),
            bytes: vec![1, 2, 3],
            source: "synthetic".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::StageTransfer {
            items: vec![selection(&event)],
            cut: true,
        },
    );
    // Pinning after staging still protects the source at the effect boundary.
    call(&mut api, Command::TogglePins { ids: vec![id] });
    let receipt = call(&mut api, Command::PasteToFolder);
    assert_eq!(receipt["items"][0]["status"], "pinned_copy");
    assert!(std::path::Path::new(receipt["items"][0]["path"].as_str().unwrap()).is_file());
    assert_eq!(call(&mut api, Command::Get { id })["id"], id);
    let again = api.execute(Request {
        api_version: 1,
        request_id: "again".into(),
        command: Command::PasteToFolder,
    });
    assert_eq!(again["ok"], false);
}

#[cfg(windows)]
#[test]
fn owned_cut_moves_only_after_copy_and_unpinned_check() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1024 * 1024).unwrap();
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "sample.png".into(),
            bytes: vec![1, 2, 3],
            source: "synthetic".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::StageTransfer {
            items: vec![selection(&event)],
            cut: true,
        },
    );
    let result = call(&mut api, Command::PasteToFolder);
    assert_eq!(result["items"][0]["status"], "moved", "{result}");
    assert_eq!(
        std::fs::read(result["items"][0]["path"].as_str().unwrap()).unwrap(),
        vec![1, 2, 3]
    );
    let missing = api.execute(Request {
        api_version: 1,
        request_id: "missing".into(),
        command: Command::Get { id },
    });
    assert_eq!(missing["ok"], false);
}

#[cfg(windows)]
#[test]
fn managed_download_move_and_mixed_pin_same_folder_are_reported() {
    use katran::source_policy::SourceId;
    let root = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let source = downloads.path().join("synthetic.bin");
    std::fs::write(&source, b"synthetic original").unwrap();
    let local = target.path().join("already.txt");
    std::fs::write(&local, b"already there").unwrap();
    let mut api = Service::open(root.path(), 1024 * 1024).unwrap();
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings.sharex_folder = None;
    settings.downloads_enabled = true;
    settings.downloads_folder = Some(downloads.path().into());
    call(&mut api, Command::SetSettings { settings });
    let download = source_api::wait_event(&mut api, SourceId::Downloads, &source);
    let local_event = call(
        &mut api,
        Command::IngestPath {
            path: local.clone(),
            source: "manual".into(),
        },
    );
    let pinned = call(
        &mut api,
        Command::IngestBytes {
            name: "pinned.png".into(),
            bytes: vec![9],
            source: "synthetic".into(),
        },
    );
    let pinned_id = pinned["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![pinned_id],
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: target.path().into(),
        },
    );
    call(
        &mut api,
        Command::StageTransfer {
            items: vec![
                selection(&download),
                selection(&local_event),
                selection(&pinned),
            ],
            cut: true,
        },
    );
    let result = call(&mut api, Command::PasteToFolder);
    assert_eq!(result["items"][0]["status"], "moved", "{result}");
    assert_eq!(result["items"][0]["original_removed"], true);
    assert!(!source.exists());
    assert_eq!(
        std::fs::read(result["items"][0]["path"].as_str().unwrap()).unwrap(),
        b"synthetic original"
    );
    assert_eq!(result["items"][1]["status"], "same_folder");
    assert_eq!(std::fs::read(local).unwrap(), b"already there");
    assert_eq!(result["items"][2]["status"], "pinned_copy");
    assert_eq!(
        call(&mut api, Command::Get { id: pinned_id })["id"],
        pinned_id
    );
}

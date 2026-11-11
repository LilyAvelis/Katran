use katran::{
    Command, Request, Service, clipboard_files,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::path::PathBuf;

struct NoClipboard;
impl clipboard_files::Source for NoClipboard {
    fn sequence(&self) -> u32 {
        panic!("Direct transfer must not read clipboard sequence")
    }
    fn files(&self) -> katran::Result<Vec<PathBuf>> {
        panic!("Direct transfer must not read clipboard files")
    }
}
fn request(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "direct".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let response = request(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn select(api: &mut Service, ids: &[i64]) {
    call(api, Command::SetVisible { ids: ids.to_vec() });
    for (index, id) in ids.iter().enumerate() {
        call(
            api,
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
}

#[test]
fn missing_folder_keeps_selection_and_never_reads_clipboard() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "keep.png".into(),
            bytes: vec![1],
            source: "synthetic".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    select(&mut api, &[id]);
    let result = request(&mut api, Command::TransferSelection);
    assert_eq!(result["error"]["code"], "folder_not_selected");
    let state = call(&mut api, Command::Interaction);
    assert_eq!(state["selected"][0], id);
    assert_eq!(state["tab"], "images");
    assert!(std::path::Path::new(event["path"].as_str().unwrap()).exists());
}

#[cfg(windows)]
#[test]
fn direct_move_commits_verified_destination_without_clipboard() {
    let root=tempfile::tempdir().unwrap();let folder=tempfile::tempdir().unwrap();
    let mut api=Service::open(root.path(),1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let event=call(&mut api,Command::IngestBytes{name:"move.png".into(),bytes:vec![4,5,6],source:"synthetic".into()});
    let id=event["id"].as_i64().unwrap();
    call(&mut api,Command::ChooseFolder{path:folder.path().into()});
    call(&mut api,Command::SetTab{tab:ShelfTab::Images});
    select(&mut api,&[id]);
    let result=call(&mut api,Command::TransferSelection);
    assert_eq!(result["items"][0]["status"],"moved","{result}");
    assert_eq!(std::fs::read(folder.path().join("move.png")).unwrap(),vec![4,5,6]);
    assert_eq!(request(&mut api,Command::Get{id})["ok"],false);
}

#[test]
fn pinned_and_missing_sources_are_reported_individually() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let pinned = call(
        &mut api,
        Command::IngestBytes {
            name: "pinned.png".into(),
            bytes: vec![1],
            source: "synthetic".into(),
        },
    );
    let missing = call(
        &mut api,
        Command::IngestBytes {
            name: "missing.png".into(),
            bytes: vec![2],
            source: "synthetic".into(),
        },
    );
    let pinned_id = pinned["id"].as_i64().unwrap();
    let missing_id = missing["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![pinned_id],
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    select(&mut api, &[pinned_id, missing_id]);
    std::fs::remove_file(missing["path"].as_str().unwrap()).unwrap();
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["direction"], "to_folder");
    assert_eq!(result["interaction"]["tab"], "folder");
    assert_eq!(result["items"][0]["status"], "pinned_copy");
    assert_eq!(result["items"][1]["status"], "failed");
    assert_eq!(
        call(&mut api, Command::Get { id: pinned_id })["id"],
        pinned_id
    );
}

#[test]
fn folder_import_reports_unsupported_and_stale_and_opens_successful_category() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("note.txt", "text"),
        ("unknown.pdf", "pdf"),
        ("stale.txt", "old"),
    ] {
        std::fs::write(folder.path().join(name), contents).unwrap();
    }
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
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
    let lookup = |name: &str| {
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["name"] == name)
            .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    let ids = [
        lookup("note.txt"),
        lookup("unknown.pdf"),
        lookup("stale.txt"),
    ];
    select(&mut api, &ids);
    std::fs::write(folder.path().join("stale.txt"), b"changed length").unwrap();
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["direction"], "from_folder");
    assert_eq!(result["interaction"]["tab"], "text");
    assert_eq!(result["items"][0]["event"]["kind"], "text");
    assert_eq!(result["items"][1]["error"]["code"], "unsupported_import");
    assert_eq!(result["items"][2]["error"]["code"], "stale_selection");
    assert_eq!(
        std::fs::read_to_string(folder.path().join("note.txt")).unwrap(),
        "text"
    );
}

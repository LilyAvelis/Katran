use katran::{
    Command, Request, Service, clipboard_files,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::path::PathBuf;

fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "files".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

struct NoClipboard;
impl clipboard_files::Source for NoClipboard {
    fn sequence(&self) -> u32 {
        panic!("Files direct transfer must not read Clipboard")
    }
    fn files(&self) -> katran::Result<Vec<PathBuf>> {
        panic!("Files direct transfer must not read Clipboard")
    }
}

fn select(api: &mut Service, id: i64) {
    call(api, Command::SetVisible { ids: vec![id] });
    call(
        api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
}

#[test]
fn files_pointer_is_visible_durable_and_deletion_leaves_original() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let path = originals.path().join("sample.bin");
    std::fs::write(&path, b"synthetic file").unwrap();
    let mut api = Service::open(data.path(), 1_000_000).unwrap();
    assert_eq!(
        call(&mut api, Command::Tabs)["visible_tabs"]
            .as_array()
            .unwrap()
            .last()
            .unwrap(),
        "files"
    );
    let added = call(
        &mut api,
        Command::AddFiles {
            paths: vec![path.clone()],
        },
    );
    assert_eq!(added["items"][0]["ok"], true, "{added}");
    let event = &added["items"][0]["event"];
    let id = event["id"].as_i64().unwrap();
    assert_eq!(event["storage"], "reference");
    assert_eq!(event["source"], "files");
    let again = call(
        &mut api,
        Command::AddFiles {
            paths: vec![path.clone()],
        },
    );
    assert_eq!(again["items"][0]["event"]["id"], id);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
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
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["id"], id);
    assert_eq!(std::fs::read(&path).unwrap(), b"synthetic file");
    drop(api);
    let mut api = Service::open(data.path(), 1_000_000).unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
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
    assert_eq!(page["events"][0]["id"], id);
    let removed = call(
        &mut api,
        Command::DeleteSelection {
            ids: vec![id],
            confirmed: false,
        },
    );
    assert_eq!(removed["deleted"], 1, "{removed}");
    assert_eq!(std::fs::read(path).unwrap(), b"synthetic file");
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    assert!(page["events"].as_array().unwrap().is_empty());
}

#[cfg(windows)]
#[test]
fn files_copy_retains_pointer_and_move_removes_it_after_verified_copy() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let copy_folder = tempfile::tempdir().unwrap();
    let move_folder = tempfile::tempdir().unwrap();
    let source = originals.path().join("object.bin");
    std::fs::write(&source, b"synthetic transfer").unwrap();
    let mut api = Service::open(data.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let added = call(
        &mut api,
        Command::AddFiles {
            paths: vec![source.clone()],
        },
    );
    let id = added["items"][0]["event"]["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: copy_folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    select(&mut api, id);
    let copied = call(&mut api, Command::CopySelection);
    assert_eq!(copied["items"][0]["status"], "copied", "{copied}");
    assert_eq!(std::fs::read(&source).unwrap(), b"synthetic transfer");
    assert_eq!(
        std::fs::read(copy_folder.path().join("object.bin")).unwrap(),
        b"synthetic transfer"
    );
    assert_eq!(call(&mut api, Command::Get { id })["id"], id);
    call(
        &mut api,
        Command::ChooseFolder {
            path: move_folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    select(&mut api, id);
    let moved = call(&mut api, Command::TransferSelection);
    assert_eq!(moved["items"][0]["status"], "moved", "{moved}");
    assert!(!source.exists());
    assert_eq!(
        std::fs::read(move_folder.path().join("object.bin")).unwrap(),
        b"synthetic transfer"
    );
    let missing = api.execute(Request {
        api_version: 1,
        request_id: "moved".into(),
        command: Command::Get { id },
    });
    assert_eq!(missing["ok"], false);
}

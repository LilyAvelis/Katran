use katran::{
    Command, Request, Service, clipboard_files,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "files-transfer-edges".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

struct NoClipboard;
impl clipboard_files::Source for NoClipboard {
    fn sequence(&self) -> u32 {
        panic!("direct transfer read Clipboard")
    }
    fn files(&self) -> katran::Result<Vec<PathBuf>> {
        panic!("direct transfer read Clipboard")
    }
}

fn select(api: &mut Service, folder: &Path, ids: &[i64]) {
    call(
        api,
        Command::ChooseFolder {
            path: folder.into(),
        },
    );
    call(
        api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
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
fn pinned_external_alias_makes_files_move_copy_only() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = originals.path().join("alias.bin");
    std::fs::write(&source, b"synthetic alias").unwrap();
    let mut api = Service::open(data.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let alias = call(
        &mut api,
        Command::IngestPath {
            path: source.clone(),
            source: "synthetic".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut api, Command::TogglePins { ids: vec![alias] });
    let id = call(
        &mut api,
        Command::AddFiles {
            paths: vec![source.clone()],
        },
    )["items"][0]["event"]["id"]
        .as_i64()
        .unwrap();
    assert_ne!(id, alias);
    select(&mut api, folder.path(), &[id]);
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["items"][0]["status"], "pinned_copy", "{result}");
    assert_eq!(std::fs::read(source).unwrap(), b"synthetic alias");
    assert_eq!(
        std::fs::read(folder.path().join("alias.bin")).unwrap(),
        b"synthetic alias"
    );
    assert_eq!(call(&mut api, Command::Get { id })["id"], id);
    assert_eq!(call(&mut api, Command::Get { id: alias })["id"], alias);
}

#[cfg(windows)]
#[test]
fn move_batch_removes_only_successful_pointer_and_preserves_failed_one() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let good = originals.path().join("good.bin");
    let missing = originals.path().join("missing.bin");
    std::fs::write(&good, b"synthetic good").unwrap();
    std::fs::write(&missing, b"synthetic missing").unwrap();
    let mut api = Service::open(data.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let added = call(
        &mut api,
        Command::AddFiles {
            paths: vec![good.clone(), missing.clone()],
        },
    );
    let good_id = added["items"][0]["event"]["id"].as_i64().unwrap();
    let missing_id = added["items"][1]["event"]["id"].as_i64().unwrap();
    std::fs::remove_file(&missing).unwrap();
    select(&mut api, folder.path(), &[good_id, missing_id]);
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["items"][0]["status"], "moved", "{result}");
    assert_eq!(result["items"][1]["status"], "failed", "{result}");
    assert!(!good.exists());
    assert_eq!(
        std::fs::read(folder.path().join("good.bin")).unwrap(),
        b"synthetic good"
    );
    let gone = api.execute(Request {
        api_version: 1,
        request_id: "gone".into(),
        command: Command::Get { id: good_id },
    });
    assert_eq!(gone["ok"], false);
    assert_eq!(
        call(&mut api, Command::Get { id: missing_id })["available"],
        false
    );
}

use katran::{
    Command, Request, Service, clipboard_files,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::path::PathBuf;

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "files-failures".into(),
        command,
    })
}

fn call(api: &mut Service, command: Command) -> Value {
    let response = reply(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
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

fn service(data: &std::path::Path) -> Service {
    let mut api = Service::open(data, 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    api
}

fn add(api: &mut Service, paths: Vec<PathBuf>) -> Vec<i64> {
    let result = call(api, Command::AddFiles { paths });
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            assert_eq!(item["ok"], true, "{item}");
            item["event"]["id"].as_i64().unwrap()
        })
        .collect()
}

fn choose_files_folder(api: &mut Service, folder: &std::path::Path) {
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
fn missing_original_remains_as_unavailable_pointer_and_transfer_reports_failure() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = originals.path().join("vanished.bin");
    std::fs::write(&source, b"synthetic original").unwrap();

    let mut api = service(data.path());
    let id = add(&mut api, vec![source.clone()])[0];
    std::fs::remove_file(&source).unwrap();

    let unavailable = call(&mut api, Command::Get { id });
    assert_eq!(unavailable["id"], id);
    assert_eq!(unavailable["source"], "files");
    assert_eq!(unavailable["storage"], "reference");
    assert_eq!(unavailable["available"], false);

    choose_files_folder(&mut api, folder.path());
    select(&mut api, &[id]);
    let moved = call(&mut api, Command::TransferSelection);
    assert_eq!(moved["items"].as_array().unwrap().len(), 1, "{moved}");
    assert_eq!(moved["items"][0]["id"], id);
    assert_eq!(moved["items"][0]["status"], "failed");
    assert_eq!(moved["items"][0]["error"]["code"], "file_unavailable");
    assert_eq!(call(&mut api, Command::Get { id })["available"], false);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn copy_collision_uses_numbered_name_and_preserves_both_existing_files() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = originals.path().join("collision.bin");
    std::fs::write(&source, b"synthetic source").unwrap();
    std::fs::write(folder.path().join("collision.bin"), b"keep existing").unwrap();

    let mut api = service(data.path());
    let id = add(&mut api, vec![source.clone()])[0];
    choose_files_folder(&mut api, folder.path());
    select(&mut api, &[id]);

    let copied = call(&mut api, Command::CopySelection);
    assert_eq!(copied["items"].as_array().unwrap().len(), 1, "{copied}");
    assert_eq!(copied["items"][0]["id"], id);
    assert_eq!(copied["items"][0]["status"], "copied");
    let numbered = PathBuf::from(copied["items"][0]["path"].as_str().unwrap());
    assert_eq!(numbered.file_name().unwrap(), "collision (1).bin");
    assert_eq!(
        numbered.parent().unwrap(),
        folder.path().canonicalize().unwrap()
    );
    assert_eq!(std::fs::read(numbered).unwrap(), b"synthetic source");
    assert_eq!(
        std::fs::read(folder.path().join("collision.bin")).unwrap(),
        b"keep existing"
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"synthetic source");
    assert_eq!(call(&mut api, Command::Get { id })["id"], id);
}

#[test]
fn mixed_batch_reports_each_outcome_and_keeps_failed_pointer() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let good = originals.path().join("good.bin");
    let missing = originals.path().join("missing.bin");
    std::fs::write(&good, b"synthetic good").unwrap();
    std::fs::write(&missing, b"synthetic then removed").unwrap();

    let mut api = service(data.path());
    let ids = add(&mut api, vec![good.clone(), missing.clone()]);
    std::fs::remove_file(&missing).unwrap();
    choose_files_folder(&mut api, folder.path());
    select(&mut api, &ids);

    let copied = call(&mut api, Command::CopySelection);
    assert_eq!(copied["direction"], "to_folder", "{copied}");
    let items = copied["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{copied}");
    assert_eq!(items[0]["id"], ids[0]);
    assert_eq!(items[0]["status"], "copied");
    assert_eq!(items[1]["id"], ids[1]);
    assert_eq!(items[1]["status"], "failed");
    assert_eq!(items[1]["error"]["code"], "file_unavailable");
    assert_eq!(
        std::fs::read(folder.path().join("good.bin")).unwrap(),
        b"synthetic good"
    );
    assert_eq!(std::fs::read(&good).unwrap(), b"synthetic good");
    assert!(!folder.path().join("missing.bin").exists());
    assert_eq!(call(&mut api, Command::Get { id: ids[0] })["id"], ids[0]);
    let still_missing = call(&mut api, Command::Get { id: ids[1] });
    assert_eq!(still_missing["id"], ids[1]);
    assert_eq!(still_missing["available"], false);
}

#[test]
fn pinned_files_move_materializes_copy_and_retains_source_and_pointer() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = originals.path().join("pinned.bin");
    std::fs::write(&source, b"synthetic pinned original").unwrap();

    let mut api = service(data.path());
    let id = add(&mut api, vec![source.clone()])[0];
    let pin = call(&mut api, Command::TogglePins { ids: vec![id] });
    assert_eq!(pin["items"][0]["pinned"], true, "{pin}");
    choose_files_folder(&mut api, folder.path());
    select(&mut api, &[id]);

    let moved = call(&mut api, Command::TransferSelection);
    assert_eq!(moved["items"].as_array().unwrap().len(), 1, "{moved}");
    assert_eq!(moved["items"][0]["id"], id);
    assert_eq!(moved["items"][0]["status"], "pinned_copy");
    assert_eq!(moved["items"][0]["destination_created"], true);
    assert_eq!(moved["items"][0]["original_removed"], false);
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"synthetic pinned original"
    );
    assert_eq!(
        std::fs::read(folder.path().join("pinned.bin")).unwrap(),
        b"synthetic pinned original"
    );
    let retained = call(&mut api, Command::Get { id });
    assert_eq!(retained["id"], id);
    assert_eq!(retained["available"], true);
    assert_eq!(
        call(&mut api, Command::GetPinnedProperties { id })["properties"]["pinned"],
        true
    );
}

use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::{Value, json};
fn call(api: &mut Service, command: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "workspace".into(),
        command,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn folder_history_session_and_pinned_navigation_via_dispatcher() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("one.txt"), "synthetic").unwrap();
    std::fs::create_dir(folder.path().join("hidden-directory")).unwrap();
    let mut api = Service::open(root.path(), 100000).unwrap();
    let id = call(
        &mut api,
        Command::IngestBytes {
            name: "own.png".into(),
            bytes: vec![1, 2],
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut api, Command::TogglePins { ids: vec![id] });
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    assert_eq!(
        call(&mut api, Command::SelectTabPosition { position: 1 })["pinned_only"],
        true
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
    assert_eq!(page["events"][0]["properties"]["pinned"], true);
    call(
        &mut api,
        Command::SetPinnedTags {
            id,
            tags: vec!["Keep".into()],
        },
    );
    call(
        &mut api,
        Command::FilterPinnedTag {
            tag: Some("KEEP".into()),
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
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert!(page["events"][0]["id"].as_i64().unwrap() < 0);
    call(&mut api, Command::ToggleFavoriteFolder);
    call(&mut api, Command::ToggleTab);
    assert_eq!(call(&mut api, Command::Interaction)["pinned_only"], false);
    assert_eq!(call(&mut api, Command::ToggleTab)["tab"], "folder");
    drop(api);
    let mut api = Service::open(root.path(), 100000).unwrap();
    assert_eq!(call(&mut api, Command::CurrentFolder), json!({"path":null}));
    assert_eq!(
        call(&mut api, Command::FolderHistory { query: "".into() })["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(&mut api, Command::ClearFolderHistory { confirmed: true });
    assert_eq!(
        call(&mut api, Command::FavoriteFolders { query: "".into() })["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn selected_folder_favorite_can_be_removed_after_directory_disappears() {
    let root = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("favorite");
    std::fs::create_dir(&path).unwrap();
    let mut api = Service::open(root.path(), 100000).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::ToggleFavoriteFolderPath { path: path.clone() }
        )["favorite"],
        true
    );
    assert_eq!(
        call(
            &mut api,
            Command::ToggleFavoriteFolderPath { path: path.clone() }
        )["favorite"],
        false
    );
    assert_eq!(
        call(
            &mut api,
            Command::ToggleFavoriteFolderPath { path: path.clone() }
        )["favorite"],
        true
    );
    let saved = std::path::PathBuf::from(
        call(&mut api, Command::FavoriteFolders { query: "".into() })["items"][0]["path"]
            .as_str()
            .unwrap(),
    );
    std::fs::remove_dir(&path).unwrap();
    assert_eq!(
        call(&mut api, Command::ToggleFavoriteFolderPath { path: saved })["favorite"],
        false
    );
    assert!(
        call(&mut api, Command::FavoriteFolders { query: "".into() })["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn folder_preview_snapshot_via_read_only_dispatcher() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("sample.md");
    std::fs::write(&path, "# Synthetic").unwrap();
    drop(Service::open(root.path(), 100000).unwrap());
    let mut api = Service::open_read_only(root.path()).unwrap();
    let revision = katran::preview::revision(&path, None);
    assert_eq!(
        call(
            &mut api,
            Command::ReadDocumentPath {
                path: path.clone(),
                revision: revision.clone()
            }
        )["text"],
        "# Synthetic"
    );
    std::fs::write(&path, "different content").unwrap();
    let r = api.execute(Request {
        api_version: 1,
        request_id: "stale".into(),
        command: Command::ReadDocumentPath { path, revision },
    });
    assert_eq!(r["ok"], false);
    assert_eq!(r["error"]["code"], "source_changed");
}

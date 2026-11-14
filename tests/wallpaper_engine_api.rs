#[path = "common/source_api.rs"]
mod source_api;
use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
    source_policy::SourceId,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn call(api: &mut Service, command: Command) -> Value {
    let v = api.execute(Request {
        api_version: 1,
        request_id: "video-wallpaper".into(),
        command,
    });
    assert_eq!(v["ok"], true, "{v}");
    v["result"].clone()
}
fn select_and_check(api: &mut Service, id: i64) {
    call(api, Command::SetVisible { ids: vec![id] });
    call(
        api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let actions = call(api, Command::Actions);
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "use_as_video_desktop_wallpaper"),
        "{actions}"
    );
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "use_as_video_desktop_wallpaper_silent"),
        "{actions}"
    );
}
#[test]
fn video_wallpaper_selection_contract() {
    let profile = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("sample.mp4");
    std::fs::write(&path, b"synthetic video bytes").unwrap();
    let mut api = Service::open(profile.path(), 1_000_000).unwrap();
    let empty = api.execute(Request {
        api_version: 1,
        request_id: "empty".into(),
        command: Command::UseSelectedVideoAsDesktopWallpaper,
    });
    assert_eq!(empty["error"]["code"], "invalid_selection", "{empty}");

    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().to_owned(),
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
    select_and_check(&mut api, page["events"][0]["id"].as_i64().unwrap());

    let added = call(
        &mut api,
        Command::AddFiles {
            paths: vec![path.clone()],
        },
    );
    let id = added["items"][0]["event"]["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    select_and_check(&mut api, id);
    std::fs::write(&path, b"changed synthetic video").unwrap();
    let stale = api.execute(Request {
        api_version: 1,
        request_id: "stale".into(),
        command: Command::UseSelectedVideoAsDesktopWallpaper,
    });
    assert_eq!(stale["ok"], false, "{stale}");

    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings.downloads_folder = Some(folder.path().to_owned());
    settings.downloads_enabled = true;
    call(&mut api, Command::SetSettings { settings });
    let item = source_api::wait_event(&mut api, SourceId::Downloads, &path);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Downloads,
        },
    );
    select_and_check(&mut api, item["id"].as_i64().unwrap());

    let bytes = b"synthetic managed video";
    let digest = format!("{:x}", Sha256::digest(bytes));
    let dir = profile.path().join("video-backgrounds");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{digest}.mp4")), bytes).unwrap();
    std::fs::write(dir.join(format!("{digest}.json")), serde_json::json!({"id":digest,"name":"No extension","codec":"h264","width":160,"height":90,"duration":1.0}).to_string()).unwrap();
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings.hidden_tabs.retain(|t| *t != ShelfTab::Desktop);
    call(&mut api, Command::SetSettings { settings });
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Desktop,
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
    select_and_check(&mut api, page["events"][0]["id"].as_i64().unwrap());
}

#[test]
fn silent_wallpaper_rejects_empty_selection() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let response = api.execute(Request {
        api_version: 1,
        request_id: "silent-empty".into(),
        command: Command::UseSelectedVideoAsSilentDesktopWallpaper,
    });
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(response["error"]["code"], "invalid_selection", "{response}");
}

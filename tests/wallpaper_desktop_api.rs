use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "wallpaper-desktop".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

#[test]
fn wallpaper_video_is_a_stable_selectable_desktop_asset_and_active_is_protected() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("wallpaper-videos");
    std::fs::create_dir(&dir).unwrap();
    let bytes = b"synthetic video with audio";
    let id = format!("{:x}", Sha256::digest(bytes));
    std::fs::write(dir.join(format!("{id}.webm")), bytes).unwrap();
    std::fs::write(
        dir.join(format!("{id}.json")),
        json!({"id":id,"name":"Original without extension","extension":"webm"}).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("active"), &id).unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    let event_id = page["events"][0]["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::SetVisible {
            ids: vec![event_id],
        },
    );
    call(
        &mut api,
        Command::Select {
            id: event_id,
            mode: SelectionMode::Replace,
        },
    );
    assert!(
        call(&mut api, Command::Actions)
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "use_as_video_desktop_wallpaper")
    );
    let result = call(
        &mut api,
        Command::DeleteSelection {
            ids: vec![event_id],
            confirmed: true,
        },
    );
    assert_eq!(result["deleted"], 0);
    assert_eq!(result["items"][0]["error"]["code"], "background_in_use");
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(page["events"][0]["id"], event_id);
    std::fs::remove_file(dir.join("active")).unwrap();
    let result = call(
        &mut api,
        Command::DeleteSelection {
            ids: vec![event_id],
            confirmed: true,
        },
    );
    assert_eq!(result["deleted"], 1);
}

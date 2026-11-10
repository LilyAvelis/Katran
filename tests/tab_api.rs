#[path = "common/source_api.rs"]
mod source_api;
use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
    settings::Settings,
};
use serde_json::Value;

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "tabs".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let response = reply(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn browse(api: &mut Service, query: &str, limit: u32, after_id: Option<i64>) -> Value {
    call(
        api,
        Command::BrowseShelf {
            query: query.into(),
            limit,
            after_id,
        },
    )
}
fn ingest(api: &mut Service, name: &str, source: &str) -> i64 {
    call(
        api,
        Command::IngestBytes {
            name: name.into(),
            bytes: vec![1, 2, 3],
            source: source.into(),
        },
    )["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn tabs_filter_before_pagination_and_preserve_hidden_selection() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let first = ingest(&mut api, "needle-first.png", "test");
    let second = ingest(&mut api, "needle-second.png", "test");
    ingest(&mut api, "needle.txt", "test");
    let shots = tempfile::tempdir().unwrap();
    let mut settings = Settings::defaults(100_000);
    settings.downloads_enabled = false;
    settings.sharex_folder = Some(shots.path().to_path_buf());
    settings.downloads_enabled = false;
    call(&mut api, Command::SetSettings { settings });
    let path = shots.path().join("needle-screen.png");
    std::fs::write(&path, [1, 2, 3]).unwrap();
    let screenshot =
        source_api::wait_event(&mut api, katran::source_policy::SourceId::Sharex, &path)["id"]
            .as_i64()
            .unwrap();
    ingest(&mut api, "unrelated.png", "test");
    assert_eq!(call(&mut api, Command::Interaction)["tab"], "images");
    let page = browse(&mut api, "needle", 1, None);
    assert_eq!(page["events"][0]["id"], second);
    let page = browse(&mut api, "needle", 1, page["next_cursor"].as_i64());
    assert_eq!(page["events"][0]["id"], first);
    assert!(
        browse(&mut api, "needle", 1, page["next_cursor"].as_i64())["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(
        &mut api,
        Command::SetVisible {
            ids: vec![first, second],
        },
    );
    call(
        &mut api,
        Command::Select {
            id: first,
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Screenshots,
        },
    );
    assert_eq!(
        browse(&mut api, "needle", 10, None)["events"][0]["id"],
        screenshot
    );
    let state = call(
        &mut api,
        Command::SetVisible {
            ids: vec![screenshot],
        },
    );
    assert_eq!(state["selected"], serde_json::json!([first]));
    assert_eq!(call(&mut api, Command::ToggleTab)["tab"], "all");
    assert_eq!(
        browse(&mut api, "needle", 10, None)["events"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(call(&mut api, Command::ToggleTab)["tab"], "screenshots");
    assert_eq!(
        reply(
            &mut api,
            Command::BrowseShelf {
                query: "".into(),
                limit: 0,
                after_id: None
            }
        )["error"]["code"],
        "invalid_argument"
    );
}

#[test]
fn sharex_scan_is_stable_durable_and_deletion_keeps_original() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let original = folder.path().join("synthetic.png");
    image::DynamicImage::new_rgb8(3, 2).save(&original).unwrap();
    let bytes = std::fs::read(&original).unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let mut settings = Settings::defaults(100_000);
    settings.downloads_enabled = false;
    settings.sharex_folder = Some(folder.path().to_path_buf());
    settings.downloads_enabled = false;
    call(&mut api, Command::SetSettings { settings });
    let scan = source_api::wait_event(&mut api, katran::source_policy::SourceId::Sharex, &original);
    let id = scan["id"].as_i64().unwrap();
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert!(source_api::settle(&mut api, katran::source_policy::SourceId::Sharex).is_empty());
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Screenshots,
        },
    );
    assert_eq!(
        browse(&mut api, "", 10, None)["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![id],
            confirmed: true,
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert!(source_api::settle(&mut api, katran::source_policy::SourceId::Sharex).is_empty());
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    assert!(
        browse(&mut api, "", 10, None)["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    let mut settings = Settings::defaults(100_000);
    settings.downloads_enabled = false;
    settings.sharex_folder = Some(folder.path().join("missing"));
    call(&mut api, Command::SetSettings { settings });
    assert_eq!(
        reply(&mut api, Command::ScanSharex)["ok"],
        true,
        "scheduling succeeds; worker failure is asynchronous"
    );
    source_api::poll_until(&mut api, "invalid source failure", |_, batch| {
        batch["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["source_id"] == "sharex" && s["state"] == "failed" && s["error"].is_string())
    });
    let mut settings = Settings::defaults(100_000);
    settings.downloads_enabled = false;
    settings.sharex_folder = Some("relative-folder".into());
    assert_eq!(
        reply(&mut api, Command::SetSettings { settings })["error"]["code"],
        "invalid_settings"
    );
}

#[test]
fn tab_shortcuts_and_reserved_keys_use_shared_catalogue() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let catalogue = call(&mut api, Command::Shortcuts);
    let rows = catalogue.as_array().unwrap();
    for (id, binding, rebindable) in [
        ("images", "", true),
        ("screenshots", "", true),
        ("tab_toggle", "Tab", false),
        ("commands", "Ctrl K", false),
        ("settings", "Ctrl Comma", true),
    ] {
        let row = rows.iter().find(|row| row["id"] == id).unwrap();
        assert_eq!(row["binding"].as_str().unwrap_or(""), binding);
        assert_eq!(row["rebindable"], rebindable);
    }
    for binding in ["Ctrl K", "Tab"] {
        assert_eq!(
            reply(
                &mut api,
                Command::SetShortcut {
                    id: "settings".into(),
                    binding: Some(binding.into()),
                    replace: true,
                    reset: false
                }
            )["error"]["code"],
            "reserved_shortcut"
        );
    }
    call(
        &mut api,
        Command::SetShortcut {
            id: "settings".into(),
            binding: Some("Ctrl L".into()),
            replace: false,
            reset: false,
        },
    );
}

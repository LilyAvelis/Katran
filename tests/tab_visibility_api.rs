use katran::{Command, Request, Service, interaction::ShelfTab, settings::Settings};
use serde_json::{Value, json};

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "visibility".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let r = reply(api, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
fn setup() -> (tempfile::TempDir, Service) {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let mut settings = Settings::defaults(10000);
    settings.downloads_folder = Some(root.path().join("synthetic-downloads"));
    settings.sharex_folder = Some(root.path().join("synthetic-shots"));
    std::fs::create_dir(settings.downloads_folder.as_ref().unwrap()).unwrap();
    std::fs::create_dir(settings.sharex_folder.as_ref().unwrap()).unwrap();
    call(&mut api, Command::SetSettings { settings });
    (root, api)
}

#[test]
fn switches_persist_compact_digits_and_never_reenter_disabled_tab() {
    let (root, mut api) = setup();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let tabs = call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Images,
            enabled: false,
        },
    );
    assert_eq!(tabs["visible_tabs"], json!(["screenshots", "downloads"]));
    assert_eq!(tabs["tabs"][1]["position"], Value::Null);
    assert_eq!(call(&mut api, Command::Interaction)["tab"], "folder");
    assert_eq!(call(&mut api, Command::ToggleTab)["tab"], "screenshots");
    assert_eq!(
        call(&mut api, Command::SelectTabPosition { position: 2 })["tab"],
        "downloads"
    );
    assert_eq!(
        reply(
            &mut api,
            Command::SetTab {
                tab: ShelfTab::Images
            }
        )["error"]["code"],
        "tab_disabled"
    );
    assert_eq!(
        reply(&mut api, Command::SelectTabPosition { position: 3 })["error"]["code"],
        "invalid_tab_position"
    );
    assert_eq!(
        reply(
            &mut api,
            Command::SetTabEnabled {
                tab: ShelfTab::All,
                enabled: false
            }
        )["error"]["code"],
        "fixed_tab"
    );
    // Drag positions use the compact visible list, preserving hidden tab order.
    call(
        &mut api,
        Command::MoveTab {
            tab: ShelfTab::Downloads,
            index: 0,
        },
    );
    assert_eq!(
        call(&mut api, Command::Tabs)["visible_tabs"],
        json!(["downloads", "screenshots"])
    );
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Tabs)["visible_tabs"],
        json!(["downloads", "screenshots"])
    );
    assert_eq!(call(&mut api, Command::Interaction)["tab"], "folder");
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Downloads,
            enabled: false,
        },
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Screenshots,
            enabled: false,
        },
    );
    assert_eq!(call(&mut api, Command::ToggleTab)["tab"], "folder");
    call(
        &mut api,
        Command::ToggleTabEnabled {
            tab: ShelfTab::Images,
        },
    );
    assert_eq!(
        call(&mut api, Command::Tabs)["visible_tabs"],
        json!(["images"])
    );
    assert_eq!(call(&mut api, Command::ToggleTab)["tab"], "images");
}

#[test]
fn image_switch_blocks_api_ingestion_and_hides_history_without_deleting_it() {
    let (_root, mut api) = setup();
    let image = call(
        &mut api,
        Command::IngestBytes {
            name: "saved.png".into(),
            bytes: vec![1, 2, 3],
            source: "test".into(),
        },
    );
    let file = call(
        &mut api,
        Command::IngestBytes {
            name: "saved.txt".into(),
            bytes: vec![4],
            source: "test".into(),
        },
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Images,
            enabled: false,
        },
    );
    let folder = tempfile::tempdir().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let empty = call(
        &mut api,
        Command::BrowseShelf {
            query: "".into(),
            limit: 1,
            after_id: None,
        },
    );
    assert!(empty["events"].as_array().unwrap().is_empty());
    let page = call(
        &mut api,
        Command::SearchPage {
            query: "".into(),
            limit: 1,
            after_id: None,
        },
    );
    assert_eq!(page["events"][0]["id"], file["id"]);
    let next = call(
        &mut api,
        Command::SearchPage {
            query: "".into(),
            limit: 1,
            after_id: file["id"].as_i64(),
        },
    );
    assert_eq!(next["events"][0]["id"], image["id"]);
    assert_eq!(
        call(
            &mut api,
            Command::Get {
                id: image["id"].as_i64().unwrap()
            }
        )["id"],
        image["id"]
    );
    let command = Command::IngestBytes {
        name: "new.png".into(),
        bytes: vec![9],
        source: "test".into(),
    };
    assert_eq!(reply(&mut api, command)["error"]["code"], "tab_disabled");
    let command = Command::IngestBytes {
        name: "new.png".into(),
        bytes: vec![9],
        source: "test".into(),
    };
    assert_eq!(
        reply(
            &mut api,
            Command::BeginJob {
                command: Box::new(command)
            }
        )["error"]["code"],
        "tab_disabled"
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Images,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelf {
            query: "".into(),
            limit: 10,
            after_id: None,
        },
    );
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["id"], image["id"]);
}

#[test]
fn disabled_sources_have_no_workers_and_actions_can_restore_them() {
    let (_root, mut api) = setup();
    for tab in [ShelfTab::Screenshots, ShelfTab::Downloads] {
        call(
            &mut api,
            Command::SetTabEnabled {
                tab,
                enabled: false,
            },
        );
    }
    let status = call(&mut api, Command::EnsureSources);
    for source in status["sources"].as_array().unwrap() {
        assert_eq!(source["enabled"], false);
        assert_eq!(source["worker_count"], 0);
    }
    let actions = call(&mut api, Command::Actions);
    assert!(
        !actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "downloads")
    );
    assert!(
        !actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "toggle_downloads")
    );
    call(
        &mut api,
        Command::RunAction {
            id: "toggle_downloads".into(),
        },
    );
    assert!(
        call(&mut api, Command::Tabs)["visible_tabs"]
            .as_array()
            .unwrap()
            .contains(&json!("downloads"))
    );
    call(
        &mut api,
        Command::RunAction {
            id: "tabs_settings".into(),
        },
    );
    let state = call(&mut api, Command::Interaction);
    assert_eq!(state["settings_open"], true);
    assert_eq!(state["settings_section"], 2);
}

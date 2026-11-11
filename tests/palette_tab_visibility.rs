use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "palette-tabs".into(),
        command,
    })
}

fn call(api: &mut Service, command: Command) -> Value {
    let response = reply(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

#[test]
fn tab_navigation_stays_out_of_actions_but_keeps_routes_and_bindings() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    for tab in [ShelfTab::Text, ShelfTab::Links] {
        call(&mut api, Command::SetTabEnabled { tab, enabled: true });
    }
    call(
        &mut api,
        Command::SetShortcut {
            id: "toggle_links".into(),
            binding: Some("Alt L".into()),
            replace: false,
            reset: false,
        },
    );

    let actions = call(&mut api, Command::Actions);
    let action_ids: Vec<_> = actions
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|action| action["id"].as_str())
        .collect();
    for id in [
        "all",
        "images",
        "screenshots",
        "downloads",
        "text",
        "links",
        "toggle_images",
        "toggle_screenshots",
        "toggle_downloads",
        "toggle_text",
        "toggle_links",
    ] {
        assert!(
            !action_ids.contains(&id),
            "{id} should not appear in Actions"
        );
    }
    for position in 1..=9 {
        assert!(
            !action_ids.contains(&format!("tab_position_{position}").as_str()),
            "numbered tab commands should not appear in Actions"
        );
    }
    assert!(action_ids.contains(&"tab_toggle"));
    assert!(action_ids.contains(&"tabs_settings"));

    let shortcuts = call(&mut api, Command::Shortcuts);
    let rows = shortcuts.as_array().unwrap();
    for id in [
        "all",
        "images",
        "screenshots",
        "downloads",
        "text",
        "links",
        "toggle_images",
        "toggle_screenshots",
        "toggle_downloads",
        "toggle_text",
        "toggle_links",
        "tab_position_1",
        "tab_position_9",
    ] {
        assert!(rows.iter().any(|row| row["id"] == id), "missing {id}");
    }
    assert_eq!(
        rows.iter()
            .find(|row| row["id"] == "tab_position_9")
            .unwrap()["binding"],
        "9"
    );
    assert_eq!(
        rows.iter().find(|row| row["id"] == "toggle_links").unwrap()["binding"],
        "Alt L"
    );

    for (id, tab) in [
        ("all", "folder"),
        ("images", "images"),
        ("screenshots", "screenshots"),
        ("downloads", "downloads"),
        ("text", "text"),
        ("links", "links"),
    ] {
        let state = call(&mut api, Command::RunAction { id: id.into() });
        assert_eq!(state["tab"], tab, "{id} route must remain live");
    }
    call(
        &mut api,
        Command::RunAction {
            id: "tab_position_1".into(),
        },
    );
    for id in [
        "toggle_images",
        "toggle_screenshots",
        "toggle_downloads",
        "toggle_text",
        "toggle_links",
    ] {
        let response = reply(&mut api, Command::RunAction { id: id.into() });
        assert_eq!(
            response["ok"], true,
            "{id} route must remain live: {response}"
        );
    }
    call(
        &mut api,
        Command::RunAction {
            id: "tab_toggle".into(),
        },
    );
    let settings = call(
        &mut api,
        Command::RunAction {
            id: "tabs_settings".into(),
        },
    );
    assert_eq!(settings["settings_open"], true);
    assert_eq!(settings["settings_section"], 2);
}

use katran::{Command, Request, Service};

fn reply(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "link-visual".into(),
        command,
    })
}

#[test]
fn link_visual_defaults_to_local_symbol_and_online_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert_eq!(
        reply(
            &mut api,
            Command::SetTabEnabled {
                tab: katran::interaction::ShelfTab::Links,
                enabled: true,
            },
        )["ok"],
        true
    );
    let added = reply(
        &mut api,
        Command::IngestLink {
            url: "https://app.notion.com/p/123".into(),
            source: "synthetic".into(),
        },
    );
    assert_eq!(added["ok"], true, "{added}");
    let id = added["result"]["id"].as_i64().unwrap();
    let visual = reply(&mut api, Command::GetLinkVisual { id });
    assert_eq!(visual["ok"], true, "{visual}");
    assert_eq!(visual["result"]["host"], "app.notion.com");
    assert_eq!(visual["result"]["site"], "notion");
    assert_eq!(visual["result"]["mark"], "NO");
    assert_eq!(visual["result"]["online"], false);
    assert!(visual["result"]["favicon_base64"].is_null());
    assert!(visual["result"]["warning"].is_null());

    let settings_reply = reply(&mut api, Command::GetSettings);
    let mut settings: katran::settings::Settings =
        serde_json::from_value(settings_reply["result"].clone()).unwrap();
    settings.link_favicons_online = true;
    assert_eq!(
        reply(&mut api, Command::SetSettings { settings })["ok"],
        true
    );
    let local = reply(
        &mut api,
        Command::IngestLink {
            url: "https://desk.local/path".into(),
            source: "synthetic".into(),
        },
    );
    let visual = reply(
        &mut api,
        Command::GetLinkVisual {
            id: local["result"]["id"].as_i64().unwrap(),
        },
    );
    assert_eq!(visual["ok"], true, "{visual}");
    assert_eq!(visual["result"]["online"], true);
    assert!(visual["result"]["favicon_base64"].is_null());
    assert!(visual["result"]["warning"].is_string());
}

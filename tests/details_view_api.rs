use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab, View},
    settings::{Language, Settings},
};
use serde_json::{Value, json};
fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "mini-stream".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
#[test]
fn details_is_a_per_tab_persisted_view_with_fixed_density() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let tabs = [
        ShelfTab::All,
        ShelfTab::Images,
        ShelfTab::Screenshots,
        ShelfTab::Downloads,
        ShelfTab::Text,
        ShelfTab::Links,
        ShelfTab::Tables,
        ShelfTab::Pods,
        ShelfTab::Files,
        ShelfTab::Desktop,
    ];
    for tab in tabs {
        if tab != ShelfTab::All {
            call(&mut api, Command::SetTabEnabled { tab, enabled: true });
        }
        call(&mut api, Command::SetTab { tab });
        assert_eq!(
            call(
                &mut api,
                Command::SetView {
                    view: View::Details
                }
            )["view"],
            "details"
        );
    }
    let before = call(&mut api, Command::GetSettings);
    call(&mut api, Command::AdjustDensity { delta: 5 });
    let after = call(&mut api, Command::GetSettings);
    assert_eq!(before, after);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(&mut api, Command::SetVisible { ids: vec![11, 22] });
    call(
        &mut api,
        Command::Select {
            id: 11,
            mode: SelectionMode::Replace,
        },
    );
    let state = call(&mut api, Command::SetView { view: View::List });
    assert_eq!(state["selected"], json!([11]));
    assert_eq!(state["cursor"], 11);
    assert_eq!(state["preview"], Value::Null);
    call(
        &mut api,
        Command::RunAction {
            id: "details".into(),
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    for tab in tabs {
        let state = call(&mut api, Command::SetTab { tab });
        assert_eq!(state["view"], "details", "tab {tab:?}");
    }
}
#[test]
fn action_localizes_and_legacy_preferences_are_still_readable() {
    let legacy: Settings = serde_json::from_value(json!({})).unwrap();
    assert_eq!(legacy.default_view, View::Gallery);
    assert_eq!(
        serde_json::from_str::<View>("\"list\"").unwrap(),
        View::List
    );
    assert!(serde_json::from_str::<View>("\"unknown_future_view\"").is_err());
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    for (language, label) in [
        (Language::English, "Mini stream"),
        (Language::Russian, "Мини-поток"),
    ] {
        let mut settings: Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
        settings.language = language;
        call(&mut api, Command::SetSettings { settings });
        let actions = call(&mut api, Command::Actions);
        assert_eq!(
            actions
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "details")
                .unwrap()["label"],
            label
        );
    }
}

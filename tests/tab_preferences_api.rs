use katran::{
    Command, Request, Service,
    interaction::{ShelfTab, View},
    sorting::SortMode,
};

fn call(api: &mut Service, command: Command) -> serde_json::Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "tab-preferences".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

#[test]
fn each_tab_restores_view_sort_and_density_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Links,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(&mut api, Command::AdjustDensity { delta: 2 });
    call(
        &mut api,
        Command::SetSort {
            mode: SortMode::Name,
            reverse: false,
            seed: None,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Links,
        },
    );
    assert_eq!(call(&mut api, Command::Interaction)["view"], "list");
    call(
        &mut api,
        Command::SetSort {
            mode: SortMode::Pinned,
            reverse: false,
            seed: None,
        },
    );
    call(&mut api, Command::AdjustDensity { delta: -2 });
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetView { view: View::List });
    call(&mut api, Command::AdjustDensity { delta: 1 });
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    call(&mut api, Command::SetView { view: View::List });
    call(
        &mut api,
        Command::SetSort {
            mode: SortMode::Size,
            reverse: true,
            seed: None,
        },
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Links,
            enabled: false,
        },
    );
    drop(api);

    let mut api = Service::open(root.path(), 10_000).unwrap();
    let folder = call(&mut api, Command::Interaction);
    assert_eq!(folder["view"], "gallery"); // startup tab remains Images
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    let folder = call(&mut api, Command::Interaction);
    assert_eq!(folder["view"], "list");
    assert_eq!(folder["sort"]["mode"], "size");
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let images = call(&mut api, Command::Interaction);
    assert_eq!(images["view"], "gallery");
    assert_eq!(images["sort"]["mode"], "name");
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Links,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Links,
        },
    );
    let links = call(&mut api, Command::Interaction);
    assert_eq!(links["view"], "list");
    assert_eq!(links["sort"]["mode"], "pinned");
    let settings = call(&mut api, Command::GetSettings);
    assert_eq!(settings["tab_preferences"]["images"]["gallery_columns"], 6);
    assert_eq!(settings["tab_preferences"]["links"]["stream_rows"], 1);
    assert_eq!(settings["tab_preferences"]["pods"]["view"], "list");
    assert_eq!(settings["tab_preferences"]["pods"]["stream_rows"], 4);
}

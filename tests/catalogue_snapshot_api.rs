use katran::{Command, Request, Service, interaction::ShelfTab, sorting::SortMode};
use serde_json::Value;

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "synthetic-snapshot".into(),
        command,
    })
}

fn call(api: &mut Service, command: Command) -> Value {
    let response = reply(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn image(api: &mut Service, name: &str) -> i64 {
    call(
        api,
        Command::IngestBytes {
            name: name.into(),
            bytes: name.as_bytes().to_vec(),
            source: "synthetic".into(),
        },
    )["id"]
        .as_i64()
        .unwrap()
}

fn snapshot(api: &mut Service, limit: u32) -> Value {
    call(
        api,
        Command::BrowseTabSnapshot {
            tab: ShelfTab::Images,
            query: String::new(),
            limit,
        },
    )
}

#[test]
fn read_only_snapshot_has_saved_sort_metadata_materials_and_explicit_more() {
    let root = tempfile::tempdir().unwrap();
    let mut writer = Service::open(root.path(), 100_000).unwrap();
    let alpha = image(&mut writer, "alpha.png");
    image(&mut writer, "charlie.png");
    image(&mut writer, "bravo.png");
    call(&mut writer, Command::TogglePins { ids: vec![alpha] });
    call(
        &mut writer,
        Command::SetPinnedProperties {
            id: alpha,
            display_name: Some("Synthetic title".into()),
            opener: None,
        },
    );
    call(
        &mut writer,
        Command::SetPinnedTags {
            id: alpha,
            tags: vec!["Synthetic tag".into()],
        },
    );
    call(
        &mut writer,
        Command::AssignTagSlot {
            ids: vec![alpha],
            slot: Some(2),
        },
    );
    call(
        &mut writer,
        Command::SetSort {
            mode: SortMode::Name,
            reverse: false,
            seed: None,
        },
    );
    let mut reader = Service::open_read_only(root.path()).unwrap();
    let page = snapshot(&mut reader, 2);
    assert_eq!(page["events"].as_array().unwrap().len(), 2);
    assert_eq!(page["events"][0]["id"], alpha);
    assert_eq!(page["events"][1]["name"], "bravo.png");
    assert_eq!(
        page["events"][0]["properties"]["display_name"],
        "Synthetic title"
    );
    assert_eq!(page["events"][0]["tags"][0]["key"], "synthetic tag");
    assert_eq!(page["slots"].as_array().unwrap().len(), 9);
    assert_eq!(
        page["assignments"],
        serde_json::json!([{"id":alpha,"slot":2}])
    );
    assert_eq!(page["has_more"], true);
    assert!(page["next_cursor"].is_null());
    assert_eq!(page["view"]["sort"]["mode"], "name");
    assert_eq!(page["view"]["pinned_only"], false);
    assert!(page["view"]["tag"].is_null());
    assert!(page["view"]["tag_slot"].is_null());
    assert_eq!(snapshot(&mut reader, 10)["has_more"], false);
}

#[test]
fn long_lived_reader_reloads_visibility_and_sort_without_changing_active_context() {
    let root = tempfile::tempdir().unwrap();
    let mut writer = Service::open(root.path(), 100_000).unwrap();
    let alpha = image(&mut writer, "alpha.png");
    let zulu = image(&mut writer, "zulu.png");
    let mut reader = Service::open_read_only(root.path()).unwrap();
    assert_eq!(snapshot(&mut reader, 10)["events"][0]["id"], zulu);
    call(
        &mut writer,
        Command::SetSort {
            mode: SortMode::Name,
            reverse: false,
            seed: None,
        },
    );
    assert_eq!(snapshot(&mut reader, 10)["events"][0]["id"], alpha);
    call(
        &mut writer,
        Command::SetTab {
            tab: ShelfTab::Downloads,
        },
    );
    call(&mut writer, Command::TogglePinnedView);
    let before = call(&mut writer, Command::Interaction);
    let stamp = call(&mut writer, Command::CatalogueStamp);
    assert_eq!(
        snapshot(&mut writer, 10)["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(call(&mut writer, Command::Interaction), before);
    assert_eq!(call(&mut writer, Command::CatalogueStamp), stamp);
    call(
        &mut writer,
        Command::SetTabEnabled {
            tab: ShelfTab::Images,
            enabled: false,
        },
    );
    let disabled = reply(
        &mut reader,
        Command::BrowseTabSnapshot {
            tab: ShelfTab::Images,
            query: String::new(),
            limit: 10,
        },
    );
    assert_eq!(disabled["error"]["code"], "tab_disabled");
}

#[test]
fn catalogue_stamp_detects_writer_mutations_and_external_commits_on_same_connection() {
    let root = tempfile::tempdir().unwrap();
    let mut writer = Service::open(root.path(), 100_000).unwrap();
    let before = call(&mut writer, Command::CatalogueStamp);
    assert_eq!(call(&mut writer, Command::CatalogueStamp), before);
    let id = image(&mut writer, "synthetic.png");
    let after = call(&mut writer, Command::CatalogueStamp);
    assert!(after["changes"].as_u64().unwrap() > before["changes"].as_u64().unwrap());
    let connection = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE events SET name='external-synthetic.png' WHERE id=?1",
            [id],
        )
        .unwrap();
    let external = call(&mut writer, Command::CatalogueStamp);
    assert_eq!(external["changes"], after["changes"]);
    assert_ne!(external["data_version"], after["data_version"]);
    assert_eq!(call(&mut writer, Command::CatalogueStamp), external);
}

#[test]
fn snapshot_validates_bounds_and_refuses_session_only_folder() {
    let root = tempfile::tempdir().unwrap();
    let writer = Service::open(root.path(), 100_000).unwrap();
    let mut reader = Service::open_read_only(root.path()).unwrap();
    for limit in [0, 1001] {
        let response = reply(
            &mut reader,
            Command::BrowseTabSnapshot {
                tab: ShelfTab::Images,
                query: String::new(),
                limit,
            },
        );
        assert_eq!(response["error"]["code"], "invalid_argument");
    }
    let folder = reply(
        &mut reader,
        Command::BrowseTabSnapshot {
            tab: ShelfTab::All,
            query: String::new(),
            limit: 100,
        },
    );
    assert_eq!(folder["error"]["code"], "session_only_tab");
    drop(writer);
}

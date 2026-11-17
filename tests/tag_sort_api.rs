use katran::{Command, DEFAULT_QUOTA, Request, Service, interaction::ShelfTab, sorting::SortMode};
use serde_json::Value;

fn execute(service: &mut Service, command: Command) -> Value {
    service.execute(Request {
        api_version: 1,
        request_id: "tag-sort-api".into(),
        command,
    })
}

fn call(service: &mut Service, command: Command) -> Value {
    let response = execute(service, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn rejected(service: &mut Service, command: Command, code: &str) {
    let response = execute(service, command);
    assert_eq!(response["error"]["code"], code, "{response}");
}

fn set_images(service: &mut Service) {
    call(
        service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
}

fn add(service: &mut Service, name: &str) -> i64 {
    call(
        service,
        Command::IngestBytes {
            name: name.into(),
            bytes: name.as_bytes().to_vec(),
            source: "synthetic".into(),
        },
    )["id"]
        .as_i64()
        .unwrap()
}

fn page(service: &mut Service, limit: u32, cursor: Option<String>) -> Value {
    call(
        service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit,
            cursor,
        },
    )
}

fn ids(page: &Value) -> Vec<i64> {
    page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["id"].as_i64().unwrap())
        .collect()
}

#[test]
fn tags_sort_uses_current_slot_order_untagged_last_and_arrival_descending() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    set_images(&mut service);
    let a = add(&mut service, "a.png");
    let b = add(&mut service, "b.png");
    let c = add(&mut service, "c.png");
    let d = add(&mut service, "d.png");
    call(
        &mut service,
        Command::TogglePins {
            ids: vec![a, b, c, d],
        },
    );
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![a],
            slot: Some(2),
        },
    );
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![c],
            slot: Some(1),
        },
    );
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![d],
            slot: Some(2),
        },
    );
    call(
        &mut service,
        Command::SetSort {
            mode: SortMode::Tags,
            reverse: true,
            seed: Some(0),
        },
    );
    assert_eq!(
        call(&mut service, Command::Interaction)["sort"]["reverse"],
        false
    );
    assert_eq!(ids(&page(&mut service, 10, None)), [c, d, a, b]);
    let first = page(&mut service, 2, None);
    let second = page(
        &mut service,
        2,
        first["next_cursor"].as_str().map(str::to_owned),
    );
    let mut paged = ids(&first);
    paged.extend(ids(&second));
    assert_eq!(paged, [c, d, a, b]);
    assert!(second["next_cursor"].is_null());

    let slots = call(
        &mut service,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    let order = vec![2, 1, 3, 4, 5, 6, 7, 8, 9];
    call(
        &mut service,
        Command::ReorderTagSlots {
            tab: ShelfTab::Images,
            order,
            expected_revision: slots["revision"].as_str().unwrap().into(),
        },
    );
    assert_eq!(ids(&page(&mut service, 10, None)), [d, a, c, b]);
}

#[test]
fn tags_cursor_rejects_assignment_and_definition_changes_from_another_service() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    set_images(&mut service);
    let a = add(&mut service, "a.png");
    let b = add(&mut service, "b.png");
    call(&mut service, Command::TogglePins { ids: vec![a, b] });
    call(
        &mut service,
        Command::SetSort {
            mode: SortMode::Tags,
            reverse: false,
            seed: None,
        },
    );
    let cursor = page(&mut service, 1, None)["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut writer = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    set_images(&mut writer);
    call(
        &mut writer,
        Command::AssignTagSlot {
            ids: vec![a],
            slot: Some(1),
        },
    );
    rejected(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 1,
            cursor: Some(cursor),
        },
        "stale_cursor",
    );

    let cursor = page(&mut service, 1, None)["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let slots = call(
        &mut writer,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    call(
        &mut writer,
        Command::ReorderTagSlots {
            tab: ShelfTab::Images,
            order: vec![2, 1, 3, 4, 5, 6, 7, 8, 9],
            expected_revision: slots["revision"].as_str().unwrap().into(),
        },
    );
    rejected(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 1,
            cursor: Some(cursor),
        },
        "stale_cursor",
    );
}

#[test]
fn folder_rejects_tags_sort_at_the_command_boundary() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    call(&mut service, Command::SetTab { tab: ShelfTab::All });
    rejected(
        &mut service,
        Command::SetSort {
            mode: SortMode::Tags,
            reverse: false,
            seed: None,
        },
        "folder_has_no_tags",
    );
}

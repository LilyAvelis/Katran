use katran::{
    Command, DEFAULT_QUOTA, Request, Service,
    interaction::{SelectionMode, ShelfTab},
    pinned_order::Direction,
    sorting::SortMode,
};
use serde_json::Value;

fn reply(service: &mut Service, command: Command) -> Value {
    serde_json::to_value(service.execute(Request {
        api_version: 1,
        request_id: "pinned-order-test".into(),
        command,
    }))
    .unwrap()
}

fn call(service: &mut Service, command: Command) -> Value {
    let result = reply(service, command);
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
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
fn pinned_order_precedes_arrival_and_survives_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let mut event_ids = Vec::new();
    for n in 1..=5 {
        let event = call(
            &mut service,
            Command::IngestBytes {
                name: format!("image-{n}.png"),
                bytes: vec![n],
                source: "manual".into(),
            },
        );
        event_ids.push(event["id"].as_i64().unwrap());
    }
    call(
        &mut service,
        Command::TogglePins {
            ids: vec![event_ids[1], event_ids[3]],
        },
    );
    call(
        &mut service,
        Command::RunAction {
            id: "sort_pinned".into(),
        },
    );
    assert_eq!(
        call(&mut service, Command::Interaction)["sort"]["mode"],
        "pinned"
    );
    assert_eq!(
        ids(&page(&mut service, 20, None)),
        vec![
            event_ids[1],
            event_ids[3],
            event_ids[4],
            event_ids[2],
            event_ids[0]
        ]
    );
    let mut paged_ids = Vec::new();
    let mut cursor = None;
    loop {
        let result = page(&mut service, 1, cursor);
        paged_ids.extend(ids(&result));
        cursor = result["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        paged_ids,
        vec![
            event_ids[1],
            event_ids[3],
            event_ids[4],
            event_ids[2],
            event_ids[0]
        ]
    );

    let first = page(&mut service, 1, None);
    let stale_cursor = first["next_cursor"].as_str().unwrap().to_owned();
    call(
        &mut service,
        Command::SetVisible {
            ids: event_ids.clone(),
        },
    );
    call(
        &mut service,
        Command::Select {
            id: event_ids[3],
            mode: SelectionMode::Replace,
        },
    );
    let moved = call(
        &mut service,
        Command::RunAction {
            id: "pinned_move_up".into(),
        },
    );
    assert_eq!(moved["moved"], true);
    assert_eq!(
        ids(&page(&mut service, 20, None)),
        vec![
            event_ids[3],
            event_ids[1],
            event_ids[4],
            event_ids[2],
            event_ids[0]
        ]
    );
    assert_eq!(
        reply(
            &mut service,
            Command::BrowseShelfSorted {
                query: String::new(),
                limit: 1,
                cursor: Some(stale_cursor)
            }
        )["error"]["code"],
        "stale_cursor"
    );
    drop(service);

    let mut service = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(
        &mut service,
        Command::SetSort {
            mode: SortMode::Pinned,
            reverse: false,
            seed: None,
        },
    );
    assert_eq!(
        ids(&page(&mut service, 20, None))[..2],
        [event_ids[3], event_ids[1]]
    );
    call(
        &mut service,
        Command::SetSort {
            mode: SortMode::Arrival,
            reverse: false,
            seed: None,
        },
    );
    assert_eq!(
        ids(&page(&mut service, 20, None)),
        event_ids.into_iter().rev().collect::<Vec<_>>()
    );
}

#[test]
fn moving_visible_pins_skips_other_categories() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let first = call(
        &mut service,
        Command::IngestBytes {
            name: "first.png".into(),
            bytes: vec![1],
            source: "manual".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    let hidden = call(
        &mut service,
        Command::IngestText {
            text: "hidden text".into(),
            source: "manual".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    let last = call(
        &mut service,
        Command::IngestBytes {
            name: "last.png".into(),
            bytes: vec![3],
            source: "manual".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    let event_ids = vec![first, hidden, last];
    call(
        &mut service,
        Command::TogglePins {
            ids: event_ids.clone(),
        },
    );
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(
        &mut service,
        Command::SetSort {
            mode: SortMode::Pinned,
            reverse: false,
            seed: None,
        },
    );
    assert_eq!(
        ids(&page(&mut service, 20, None)),
        vec![event_ids[0], event_ids[2]]
    );
    assert_eq!(
        call(
            &mut service,
            Command::MovePinned {
                id: event_ids[2],
                direction: Direction::Up
            }
        )["moved"],
        true
    );
    assert_eq!(
        ids(&page(&mut service, 20, None)),
        vec![event_ids[2], event_ids[0]]
    );
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    assert_eq!(ids(&page(&mut service, 20, None)), vec![event_ids[1]]);
}

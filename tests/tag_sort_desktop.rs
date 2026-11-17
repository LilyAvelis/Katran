use katran::{Command, Request, Service, interaction::ShelfTab, sorting::SortMode};
use serde_json::Value;

fn execute(service: &mut Service, command: Command) -> Value {
    service.execute(Request {
        api_version: 1,
        request_id: "tag-sort-desktop".into(),
        command,
    })
}

fn call(service: &mut Service, command: Command) -> Value {
    let response = execute(service, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn rejected_cursor(service: &mut Service, cursor: &str) {
    let response = execute(
        service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 1,
            cursor: Some(cursor.into()),
        },
    );
    assert_eq!(response["error"]["code"], "invalid_cursor", "{response}");
}

fn import(service: &mut Service, directory: &std::path::Path, number: u8) {
    let path = directory.join(format!("asset-{number}.png"));
    image::RgbaImage::from_pixel(2, 2, image::Rgba([number, 2, 3, 255]))
        .save(&path)
        .unwrap();
    call(service, Command::ImportThemeBackground { path });
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

#[test]
fn desktop_tags_sort_uses_slot_buckets_arrival_and_stable_id() {
    let root = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 100).unwrap();
    for number in 1..=4 {
        import(&mut service, originals.path(), number);
    }
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    let ids: Vec<i64> = page(&mut service, 10, None)["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["id"].as_i64().unwrap())
        .collect();
    call(&mut service, Command::TogglePins { ids: ids.clone() });
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![ids[0]],
            slot: Some(2),
        },
    );
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![ids[2]],
            slot: Some(1),
        },
    );
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![ids[3]],
            slot: Some(2),
        },
    );
    call(
        &mut service,
        Command::SetSort {
            mode: SortMode::Tags,
            reverse: true,
            seed: None,
        },
    );
    let events = page(&mut service, 10, None)["events"]
        .as_array()
        .unwrap()
        .clone();
    let mut expected = events.clone();
    expected.sort_by(|a, b| {
        let bucket = |id: i64| {
            if id == ids[2] {
                1
            } else if id == ids[0] || id == ids[3] {
                2
            } else {
                10
            }
        };
        bucket(a["id"].as_i64().unwrap())
            .cmp(&bucket(b["id"].as_i64().unwrap()))
            .then_with(|| {
                b["detected_at_ms"]
                    .as_i64()
                    .cmp(&a["detected_at_ms"].as_i64())
            })
            .then_with(|| a["id"].as_i64().cmp(&b["id"].as_i64()))
    });
    assert_eq!(events, expected);
    assert_eq!(
        call(&mut service, Command::Interaction)["sort"]["reverse"],
        false
    );

    let cursor = page(&mut service, 1, None)["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut writer = Service::open(root.path(), 100).unwrap();
    call(
        &mut writer,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    call(
        &mut writer,
        Command::AssignTagSlot {
            ids: vec![ids[1]],
            slot: Some(1),
        },
    );
    rejected_cursor(&mut service, &cursor);
}

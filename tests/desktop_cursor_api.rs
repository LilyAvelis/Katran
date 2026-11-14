use katran::{Command, Request, Service, interaction::ShelfTab, sorting::SortMode};
use serde_json::Value;

fn execute(service: &mut Service, command: Command) -> Value {
    service.execute(Request {
        api_version: 1,
        request_id: "desktop-cursor".into(),
        command,
    })
}
fn ok(service: &mut Service, command: Command) -> Value {
    let response = execute(service, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn page(service: &mut Service, query: &str, cursor: Option<String>) -> Value {
    ok(
        service,
        Command::BrowseShelfSorted {
            query: query.into(),
            limit: 1,
            cursor,
        },
    )
}
fn rejected(service: &mut Service, query: &str, cursor: &str) {
    let response = execute(
        service,
        Command::BrowseShelfSorted {
            query: query.into(),
            limit: 1,
            cursor: Some(cursor.into()),
        },
    );
    assert_eq!(response["error"]["code"], "invalid_cursor", "{response}");
}
fn import(service: &mut Service, originals: &std::path::Path, n: u8) {
    let path = originals.join(format!("item{n}.png"));
    image::RgbaImage::from_pixel(2, 2, image::Rgba([n, 2, 3, 255]))
        .save(&path)
        .unwrap();
    ok(service, Command::ImportThemeBackground { path });
}
#[test]
fn desktop_cursor_is_bound_to_view_and_catalogue_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 100).unwrap();
    for n in 1..=3 {
        import(&mut service, originals.path(), n);
    }
    ok(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    let first = page(&mut service, "", None);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = page(&mut service, "", Some(cursor.clone()));
    assert_ne!(first["events"][0]["id"], second["events"][0]["id"]);
    rejected(&mut service, "item", &cursor);
    rejected(&mut service, "", "1");
    ok(
        &mut service,
        Command::SetSort {
            mode: SortMode::Name,
            reverse: false,
            seed: Some(0),
        },
    );
    rejected(&mut service, "", &cursor);
    let cursor = page(&mut service, "", None)["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(&mut service, Command::FilterTagSlot { slot: Some(1) });
    rejected(&mut service, "", &cursor);
    ok(&mut service, Command::FilterTagSlot { slot: Some(1) });
    let cursor = page(&mut service, "", None)["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    import(&mut service, originals.path(), 4);
    rejected(&mut service, "", &cursor);
}

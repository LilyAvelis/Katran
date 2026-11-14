use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;

fn call(service: &mut Service, command: Command) -> Value {
    let response = service.execute(Request {
        api_version: 1,
        request_id: "desktop-shelf".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

#[test]
fn desktop_assets_use_standard_browse_selection_and_delete() {
    let root = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    for name in ["one.png", "two.png"] {
        let path = originals.path().join(name);
        image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([if name == "one.png" { 1 } else { 2 }, 2, 3, 255]),
        )
        .save(&path)
        .unwrap();
        call(
            &mut Service::open(root.path(), 100).unwrap(),
            Command::ImportThemeBackground { path },
        );
    }
    let mut service = Service::open(root.path(), 100).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    let page = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 1,
            cursor: None,
        },
    );
    let first = page["events"][0]["id"].as_i64().unwrap();
    let page2 = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 1,
            cursor: Some(page["next_cursor"].as_str().unwrap().into()),
        },
    );
    let second = page2["events"][0]["id"].as_i64().unwrap();
    assert_ne!(first, second);
    call(
        &mut service,
        Command::SetVisible {
            ids: vec![first, second],
        },
    );
    call(
        &mut service,
        Command::Select {
            id: first,
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut service,
        Command::Select {
            id: second,
            mode: SelectionMode::Toggle,
        },
    );
    let selection = call(&mut service, Command::Interaction);
    assert_eq!(selection["selected"].as_array().unwrap().len(), 2);
    let response = call(
        &mut service,
        Command::DeleteSelection {
            ids: vec![first, second],
            confirmed: false,
        },
    );
    assert_eq!(response["confirmation_required"], true);
    assert_eq!(response["deleted"], 0);
    let response = call(
        &mut service,
        Command::DeleteSelection {
            ids: vec![first, second],
            confirmed: true,
        },
    );
    assert_eq!(response["deleted"], 2, "{response}");
    assert!(originals.path().join("one.png").exists());
    assert!(originals.path().join("two.png").exists());
}

#[test]
fn selected_desktop_image_uses_existing_background_action() {
    let root = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    // A managed background can retain a source label without an extension.
    let path = originals.path().join("portrait");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([8, 9, 10, 255]))
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    let mut service = Service::open(root.path(), 100).unwrap();
    call(&mut service, Command::ImportThemeBackground { path });
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    let page = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    let id = page["events"][0]["id"].as_i64().unwrap();
    call(&mut service, Command::SetVisible { ids: vec![id] });
    call(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let result = call(&mut service, Command::UseSelectedImageAsBackground);
    assert_eq!(result["background_applied"], true);
}

use katran::{
    Command, Request, SelectionRef, Service,
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
    let mut event: katran::Event = serde_json::from_value(page["events"][0].clone()).unwrap();
    assert_eq!(event.name, "portrait");
    assert!(katran::file_types::has_internal_preview(&event));
    // A path extension alone must not broaden external reference behavior.
    event.id = 1;
    assert!(!katran::file_types::has_internal_preview(&event));
    call(&mut service, Command::SetVisible { ids: vec![id] });
    call(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    assert_eq!(call(&mut service, Command::TogglePreview)["preview"], id);
    call(&mut service, Command::PreviewImage { id, max_edge: 64 });
    call(&mut service, Command::ClosePreview);
    let result = call(&mut service, Command::UseSelectedImageAsBackground);
    assert_eq!(result["background_applied"], true);
}

#[test]
fn desktop_pin_survives_restart_and_filters_the_standard_view() {
    let root = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 100).unwrap();
    for (name, color) in [("a.png", 1), ("b.png", 2)] {
        let path = originals.path().join(name);
        image::RgbaImage::from_pixel(2, 2, image::Rgba([color, 2, 3, 255]))
            .save(&path)
            .unwrap();
        call(&mut service, Command::ImportThemeBackground { path });
    }
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
    let result = call(&mut service, Command::TogglePins { ids: vec![id] });
    assert_eq!(result["items"][0]["pinned"], true);
    call(
        &mut service,
        Command::SaveTagSlot {
            slot: katran::tag_slots::Slot {
                number: 1,
                name: "Desktop test".into(),
                color: "#123456".into(),
                material: katran::tag_slots::Material::Dots,
                scale: 50,
                intensity: 50,
                opacity: 55,
                angle: 0,
                seed: 1,
            },
        },
    );
    let assigned = call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![id],
            slot: Some(1),
        },
    );
    assert_eq!(assigned["items"][0]["outcome"], "applied");
    assert_eq!(call(&mut service, Command::GetTagSlot { id })["slot"], 1);
    let page = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap()["properties"]["pinned"],
        true
    );
    drop(service);
    let mut service = Service::open(root.path(), 100).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    call(&mut service, Command::TogglePinnedView);
    assert_eq!(call(&mut service, Command::GetTagSlot { id })["slot"], 1);
    let page = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["id"], id);
    call(&mut service, Command::FilterTagSlot { slot: Some(1) });
    let tagged = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(tagged["events"].as_array().unwrap().len(), 1);
    call(&mut service, Command::FilterTagSlot { slot: Some(1) });
    let delete = call(
        &mut service,
        Command::DeleteSelection {
            ids: vec![id],
            confirmed: true,
        },
    );
    assert_eq!(delete["deleted"], 0);
    assert_eq!(delete["items"][0]["skipped_pinned"], true);
    call(&mut service, Command::TogglePins { ids: vec![id] });
    assert!(call(&mut service, Command::GetTagSlot { id })["slot"].is_null());
    let delete = call(
        &mut service,
        Command::DeleteSelection {
            ids: vec![id],
            confirmed: true,
        },
    );
    assert_eq!(delete["deleted"], 1);
}

#[test]
fn desktop_group_can_drag_and_copy_to_folder() {
    let root = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 10_000_000).unwrap();
    for (name, color) in [("plain", 3), ("other.png", 4)] {
        let path = originals.path().join(name);
        image::RgbaImage::from_pixel(2, 2, image::Rgba([color, 2, 3, 255]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        call(&mut service, Command::ImportThemeBackground { path });
    }
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
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
    let ids: Vec<_> = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    let items: Vec<_> = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| SelectionRef {
            id: e["id"].as_i64().unwrap(),
            revision: e["revision"].as_str().unwrap().into(),
        })
        .collect();
    call(&mut service, Command::SetVisible { ids: ids.clone() });
    for (index, id) in ids.iter().enumerate() {
        call(
            &mut service,
            Command::Select {
                id: *id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
    let drag = call(&mut service, Command::DragEventsChecked { items });
    assert_eq!(drag["count"], 2, "{drag}");
    call(
        &mut service,
        Command::CancelPreparedDrag {
            token: drag["drag_token"].as_str().unwrap().into(),
        },
    );
    let copy = call(&mut service, Command::CopySelection);
    assert_eq!(copy["items"].as_array().unwrap().len(), 2);
    assert!(
        copy["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"] == "copied"),
        "{copy}"
    );
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2);
    assert!(
        std::fs::read_dir(folder.path()).unwrap().all(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some())
    );
}

#[test]
fn desktop_move_keeps_pinned_asset_and_removes_unpinned_copy() {
    let root = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 10_000_000).unwrap();
    for (name, color) in [("first.png", 5), ("second.png", 6)] {
        let path = originals.path().join(name);
        image::RgbaImage::from_pixel(2, 2, image::Rgba([color, 2, 3, 255]))
            .save(&path)
            .unwrap();
        call(&mut service, Command::ImportThemeBackground { path });
    }
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
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
    let ids: Vec<_> = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    call(&mut service, Command::TogglePins { ids: vec![ids[0]] });
    call(&mut service, Command::SetVisible { ids: ids.clone() });
    for (index, id) in ids.iter().enumerate() {
        call(
            &mut service,
            Command::Select {
                id: *id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
    let result = call(&mut service, Command::TransferSelection);
    let statuses: Vec<_> = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["status"].as_str().unwrap())
        .collect();
    assert!(statuses.contains(&"moved"), "{result}");
    assert!(statuses.contains(&"source_retained_copy"), "{result}");
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2);
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
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["id"], ids[0]);
}

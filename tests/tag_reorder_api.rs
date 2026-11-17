use katran::{Command, Request, Service, interaction::ShelfTab, tag_slots::Slot};
use serde_json::Value;
fn execute(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "reorder".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let result = execute(api, command);
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
}
#[test]
fn reorder_preserves_tag_and_filter_and_rejects_stale_list() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let initial = call(
        &mut api,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    let mut tag: Slot = serde_json::from_value(initial["slots"][0].clone()).unwrap();
    tag.name = "Synthetic moved".into();
    tag.color = "#123456".into();
    tag.opacity = 76;
    call(
        &mut api,
        Command::SaveTagSlotForTab {
            tab: ShelfTab::Images,
            slot: tag,
        },
    );
    let slots = call(
        &mut api,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    let revision = slots["revision"].as_str().unwrap().to_owned();
    let id = call(
        &mut api,
        Command::IngestBytes {
            name: "synthetic.png".into(),
            bytes: vec![1, 2, 3],
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut api, Command::TogglePins { ids: vec![id] });
    call(
        &mut api,
        Command::AssignTagSlot {
            ids: vec![id],
            slot: Some(1),
        },
    );
    call(&mut api, Command::FilterTagSlot { slot: Some(1) });
    call(&mut api, Command::TogglePinnedView);
    let order = vec![2, 3, 1, 4, 5, 6, 7, 8, 9];
    let reordered = call(
        &mut api,
        Command::ReorderTagSlots {
            tab: ShelfTab::Images,
            order: order.clone(),
            expected_revision: revision.clone(),
        },
    );
    assert_eq!(reordered["slots"][2]["name"], "Synthetic moved");
    assert_eq!(reordered["slots"][2]["color"], "#123456");
    assert_eq!(reordered["slots"][2]["opacity"], 76);
    assert_eq!(call(&mut api, Command::GetTagSlot { id })["slot"], 3);
    let state = call(&mut api, Command::Interaction);
    assert_eq!(state["tag_slot_filter"], 3);
    assert_eq!(state["pinned_only"], true);
    let stale = execute(
        &mut api,
        Command::ReorderTagSlots {
            tab: ShelfTab::Images,
            order,
            expected_revision: revision,
        },
    );
    assert_eq!(stale["error"]["code"], "tag_slots_changed");
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert_eq!(call(&mut api, Command::GetTagSlot { id })["slot"], 3);
    assert_eq!(
        call(&mut api, Command::ListTagSlots)["slots"][2]["name"],
        "Synthetic moved"
    );
}

#[test]
fn checked_save_rejects_old_slot_after_reorder_without_touching_neighbor() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let original = call(
        &mut api,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    let mut draft: Slot = serde_json::from_value(original["slots"][0].clone()).unwrap();
    draft.name = "Stale draft".into();
    let old_revision = original["revision"].as_str().unwrap().to_owned();
    let reordered = call(
        &mut api,
        Command::ReorderTagSlots {
            tab: ShelfTab::Images,
            order: vec![2, 1, 3, 4, 5, 6, 7, 8, 9],
            expected_revision: old_revision.clone(),
        },
    );
    let conflict = execute(
        &mut api,
        Command::SaveTagSlotChecked {
            tab: ShelfTab::Images,
            slot: draft.clone(),
            expected_revision: old_revision,
        },
    );
    assert_eq!(conflict["error"]["code"], "tag_slots_changed");
    let after_conflict = call(
        &mut api,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    assert_eq!(after_conflict["slots"], reordered["slots"]);
    assert_eq!(after_conflict["revision"], reordered["revision"]);
    draft.number = 2;
    let saved = call(
        &mut api,
        Command::SaveTagSlotChecked {
            tab: ShelfTab::Images,
            slot: draft,
            expected_revision: after_conflict["revision"].as_str().unwrap().to_owned(),
        },
    );
    let fresh = call(
        &mut api,
        Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        },
    );
    assert_eq!(fresh["slots"][0], reordered["slots"][0]);
    assert_eq!(fresh["slots"][1]["name"], "Stale draft");
    assert_eq!(fresh["revision"], saved["revision"]);
    assert_ne!(fresh["revision"], reordered["revision"]);
}

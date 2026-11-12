use katran::{
    Command, Request, Service,
    tag_slots::{Material, Slot},
};
use serde_json::Value;

fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "tags".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

#[test]
fn tag_slots_require_pin_and_unpin_clears_assignment_but_not_slot_definition() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let slots = call(&mut api, Command::ListTagSlots);
    assert_eq!(slots["slots"].as_array().unwrap().len(), 9);
    let slot = Slot {
        number: 1,
        name: "Работа".into(),
        color: "#2255AA".into(),
        material: Material::Dots,
        scale: 22,
        intensity: 18,
        opacity: 72,
        angle: 0,
        seed: 7,
    };
    assert_eq!(
        call(&mut api, Command::SaveTagSlot { slot: slot.clone() })["slot"]["name"],
        "Работа"
    );
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "synthetic.bin".into(),
            bytes: b"synthetic".to_vec(),
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    let refused = call(
        &mut api,
        Command::AssignTagSlot {
            ids: vec![id],
            slot: Some(1),
        },
    );
    assert_eq!(refused["items"][0]["outcome"], "not_pinned");
    call(&mut api, Command::TogglePins { ids: vec![id] });
    let assigned = call(
        &mut api,
        Command::AssignTagSlot {
            ids: vec![id],
            slot: Some(1),
        },
    );
    assert_eq!(assigned["items"][0]["outcome"], "applied");
    assert_eq!(call(&mut api, Command::GetTagSlot { id })["slot"], 1);
    call(&mut api, Command::TogglePins { ids: vec![id] });
    assert!(call(&mut api, Command::GetTagSlot { id })["slot"].is_null());
    call(&mut api, Command::TogglePins { ids: vec![id] });
    assert!(call(&mut api, Command::GetTagSlot { id })["slot"].is_null());
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert_eq!(
        call(&mut api, Command::ListTagSlots)["slots"][0]["name"],
        "Работа"
    );
    assert_eq!(
        call(&mut api, Command::ListTagSlots)["slots"][0]["opacity"],
        72
    );
    assert!(call(&mut api, Command::GetTagSlot { id })["slot"].is_null());
}

#[test]
fn assigning_another_slot_replaces_and_repeating_it_clears() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let id = call(
        &mut api,
        Command::IngestBytes {
            name: "retag.png".into(),
            bytes: vec![1, 2, 3],
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut api, Command::TogglePins { ids: vec![id] });
    for (slot, expected) in [
        (Some(1), Some(1)),
        (Some(2), Some(2)),
        (Some(2), None),
        (Some(1), Some(1)),
        (None, None),
    ] {
        let result = call(
            &mut api,
            Command::AssignTagSlot {
                ids: vec![id],
                slot,
            },
        );
        assert_eq!(result["items"][0]["outcome"], "applied");
        assert_eq!(
            call(&mut api, Command::GetTagSlot { id })["slot"].as_u64(),
            expected.map(|value| value as u64)
        );
    }
}

#[test]
fn existing_tag_slots_gain_opacity_without_losing_materials() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let mut saved: Slot =
        serde_json::from_value(call(&mut api, Command::ListTagSlots)["slots"][0].clone()).unwrap();
    saved.material = Material::Dots;
    saved.intensity = 90;
    saved.opacity = 80;
    call(&mut api, Command::SaveTagSlot { slot: saved });
    drop(api);

    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute_batch("ALTER TABLE tag_slots DROP COLUMN opacity; PRAGMA user_version=10;")
        .unwrap();
    drop(db);

    let mut api = Service::open(root.path(), 100_000).unwrap();
    let result = call(&mut api, Command::ListTagSlots);
    let restored = &result["slots"][0];
    assert_eq!(restored["material"], "dots");
    assert_eq!(restored["intensity"], 90);
    assert_eq!(restored["opacity"], 55);
}

#[test]
fn pinned_view_and_tag_filter_are_independent_across_input_routes() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        },
    );
    let state = call(&mut api, Command::FilterTagSlot { slot: Some(2) });
    assert_eq!(state["tag_slot_filter"], 2);
    assert_eq!(state["pinned_only"], false);

    let state = call(&mut api, Command::TogglePinnedView);
    assert_eq!(state["tag_slot_filter"], 2);
    assert_eq!(state["pinned_only"], true);
    let state = call(
        &mut api,
        Command::RunAction {
            id: "toggle_pinned_view".into(),
        },
    );
    assert_eq!(state["tag_slot_filter"], 2);
    assert_eq!(state["pinned_only"], false);
    let state = call(&mut api, Command::SelectTabPosition { position: 1 });
    assert_eq!(state["tag_slot_filter"], 2);
    assert_eq!(state["pinned_only"], true);
    let state = call(&mut api, Command::FilterTagSlot { slot: None });
    assert!(state["tag_slot_filter"].is_null());
    assert_eq!(state["pinned_only"], true);
}

#[test]
fn tagged_search_still_finds_text_link_and_table_contents() {
    use katran::interaction::ShelfTab;
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    for tab in [ShelfTab::Text, ShelfTab::Links, ShelfTab::Tables] {
        call(&mut api, Command::SetTabEnabled { tab, enabled: true });
    }
    let needle = "HIDDENNEEDLEZXYZ";
    let cases = [
        (
            ShelfTab::Text,
            Command::IngestText {
                text: format!("body {needle}"),
                source: "synthetic".into(),
            },
        ),
        (
            ShelfTab::Links,
            Command::IngestLink {
                url: format!("https://example.com/{needle}"),
                source: "synthetic".into(),
            },
        ),
        (
            ShelfTab::Tables,
            Command::IngestTable {
                rows: vec![vec![needle.into()]],
                source: "synthetic".into(),
            },
        ),
    ];
    for (tab, command) in cases {
        let id = call(&mut api, command)["id"].as_i64().unwrap();
        call(&mut api, Command::TogglePins { ids: vec![id] });
        call(
            &mut api,
            Command::AssignTagSlot {
                ids: vec![id],
                slot: Some(2),
            },
        );
        call(&mut api, Command::SetTab { tab });
        let query = || Command::BrowseShelfSorted {
            query: needle.into(),
            limit: 100,
            cursor: None,
        };
        assert_eq!(call(&mut api, query())["events"][0]["id"], id);
        call(&mut api, Command::FilterTagSlot { slot: Some(2) });
        assert_eq!(call(&mut api, query())["events"][0]["id"], id);
        call(&mut api, Command::TogglePinnedView);
        assert_eq!(call(&mut api, query())["events"][0]["id"], id);
    }
}

#[test]
fn tag_filter_intersects_current_tab_and_batch_lookup_preserves_order() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let a = call(
        &mut api,
        Command::IngestBytes {
            name: "a.png".into(),
            bytes: b"a".to_vec(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    let b = call(
        &mut api,
        Command::IngestBytes {
            name: "b.png".into(),
            bytes: b"b".to_vec(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut api, Command::TogglePins { ids: vec![a, b] });
    call(
        &mut api,
        Command::AssignTagSlot {
            ids: vec![a],
            slot: Some(2),
        },
    );
    let batch = call(&mut api, Command::GetTagSlots { ids: vec![b, a, b] });
    assert_eq!(batch["items"][0]["slot"], Value::Null);
    assert_eq!(batch["items"][1]["slot"], 2);
    assert_eq!(batch["items"][2]["id"], b);
    call(
        &mut api,
        Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        },
    );
    let state = call(&mut api, Command::FilterTagSlot { slot: Some(2) });
    assert_eq!(state["tag_slot_filter"], 2);
    assert_eq!(state["pinned_only"], false);
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["id"], a);
    let state = call(&mut api, Command::FilterTagSlot { slot: Some(2) });
    assert_eq!(state["tag_slot_filter"], Value::Null);
    assert_eq!(state["pinned_only"], false);
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(page["events"].as_array().unwrap().len(), 2);
}

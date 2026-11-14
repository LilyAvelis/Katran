use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
fn call(api: &mut Service, command: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "emoji".into(),
        command,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn full_catalogue_is_read_only_and_copies_selected_entries_to_pods() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1000000).unwrap();
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
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let first = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 1000,
            cursor: None,
        },
    );
    assert_eq!(first["total"], 3781);
    assert_eq!(first["events"][0]["text_preview"], "😀");
    assert_eq!(first["events"][0]["category"]["group"], "Smileys & Emotion");
    assert_eq!(first["dividers"][0]["level"], "group");
    assert_eq!(first["dividers"][0]["before"], 0);
    assert_eq!(first["dividers"][1]["level"], "subgroup");
    let mut cursor = first["next_cursor"].as_str().map(str::to_owned);
    let mut total = first["events"].as_array().unwrap().len();
    while let Some(c) = cursor {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 1000,
                cursor: Some(c),
            },
        );
        total += page["events"].as_array().unwrap().len();
        cursor = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(total as u64, first["total"].as_u64().unwrap());
    let found = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "cow face".into(),
            limit: 100,
            cursor: None,
        },
    );
    let events = found["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["text_preview"] == "🐮"));
    let scotland = "🏴\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}";
    let rare = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: scotland.into(),
            limit: 10,
            cursor: None,
        },
    );
    assert!(
        rare["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["text_preview"] == scotland)
    );
    assert!(katran::pods::is_pod(scotland));
    let two: Vec<i64> = first["events"]
        .as_array()
        .unwrap()
        .iter()
        .take(2)
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    let description = call(&mut api, Command::DescribePod { id: two[0] });
    assert_eq!(description["text"], first["events"][0]["text_preview"]);
    call(&mut api, Command::SetVisible { ids: two.clone() });
    for (n, id) in two.iter().enumerate() {
        call(
            &mut api,
            Command::Select {
                id: *id,
                mode: if n == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
    for command in [
        Command::DeleteSelection {
            ids: two.clone(),
            confirmed: true,
        },
        Command::DeleteEvents {
            ids: two.clone(),
            confirmed: true,
        },
    ] {
        let response = api.execute(Request {
            api_version: 1,
            request_id: "read-only".into(),
            command,
        });
        assert_eq!(response["error"]["code"], "read_only", "{response}");
    }
    let pin = api.execute(Request {
        api_version: 1,
        request_id: "read-only-pin".into(),
        command: Command::TogglePins { ids: two.clone() },
    });
    assert_eq!(pin["error"]["code"], "read_only");
    let result = call(&mut api, Command::CopySelectedObjects);
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["ok"] == true),
        "{result}"
    );
    let check = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 1000,
            cursor: None,
        },
    );
    assert_eq!(check["total"], first["total"]);
    call(&mut api, Command::SetEmojiCatalogue { enabled: false });
    let saved = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(saved["events"].as_array().unwrap().len(), 2, "{saved}");
}

#[test]
fn categories_are_not_selected_and_page_boundaries_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1_000_000).unwrap();
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
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    let mut groups = std::collections::HashSet::new();
    let mut subgroups = 0;
    let mut total = 0;
    loop {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 7,
                cursor,
            },
        );
        let events = page["events"].as_array().unwrap();
        for item in events {
            let text = item["text_preview"].as_str().unwrap();
            let stripped = text.replace('\u{fe0f}', "");
            assert!(seen.insert(stripped), "duplicate presentation: {text}");
            total += 1;
        }
        for divider in page["dividers"].as_array().unwrap() {
            assert_eq!(divider["type"], "category_divider");
            assert!(divider["before"].as_u64().unwrap() < events.len() as u64);
            assert!(divider.get("id").is_none());
            if divider["level"] == "group" {
                groups.insert(divider["label"].as_str().unwrap().to_owned());
            } else {
                subgroups += 1;
            }
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(total, 3781);
    assert_eq!(groups.len(), 9);
    assert_eq!(subgroups, 98);
}

#[test]
fn range_selection_and_search_cross_category_dividers() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1_000_000).unwrap();
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
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "face".into(),
            limit: 1000,
            cursor: None,
        },
    );
    let events = page["events"].as_array().unwrap();
    let boundary = (1..events.len())
        .find(|&i| events[i]["category"]["subgroup"] != events[i - 1]["category"]["subgroup"])
        .unwrap();
    let ids: Vec<i64> = events.iter().map(|e| e["id"].as_i64().unwrap()).collect();
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    call(
        &mut api,
        Command::Select {
            id: ids[boundary - 1],
            mode: SelectionMode::Replace,
        },
    );
    let selection = call(
        &mut api,
        Command::Select {
            id: ids[boundary],
            mode: SelectionMode::Range,
        },
    );
    assert_eq!(selection["selected"].as_array().unwrap().len(), 2);
    let copied = call(&mut api, Command::CopySelectedObjects);
    assert_eq!(copied["items"].as_array().unwrap().len(), 2);
    assert!(
        copied["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["ok"] == true)
    );
}

#[test]
fn category_filter_is_validated_and_pagination_is_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = Service::open(dir.path(), 1_000_000).unwrap();
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
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let categories = call(&mut api, Command::EmojiCategories);
    assert_eq!(categories["groups"].as_array().unwrap().len(), 9);
    let full = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: None,
        },
    );
    let stale_cursor = full["next_cursor"].as_str().unwrap().to_owned();
    let id = full["events"][0]["id"].as_i64().unwrap();
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let selected = call(
        &mut api,
        Command::SetEmojiCategory {
            group: Some("Animals & Nature".into()),
        },
    );
    assert_eq!(selected["emoji_category"], "Animals & Nature");
    assert!(selected["selected"].as_array().unwrap().is_empty());
    let bad = api.execute(Request {
        api_version: 1,
        request_id: "stale-page".into(),
        command: Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: Some(stale_cursor),
        },
    });
    assert_eq!(bad["error"]["code"], "invalid_cursor", "{bad}");
    let mut cursor = None;
    let mut count = 0;
    loop {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 7,
                cursor,
            },
        );
        let entries = page["events"].as_array().unwrap();
        assert!(
            entries
                .iter()
                .all(|e| e["category"]["group"] == "Animals & Nature")
        );
        count += entries.len();
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(count as u64, page["total"].as_u64().unwrap());
            break;
        }
    }
    assert!(count > 100);
    let search = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "cat".into(),
            limit: 100,
            cursor: None,
        },
    );
    assert!(search["total"].as_u64().unwrap() > 0);
    assert!(
        search["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["category"]["group"] == "Animals & Nature")
    );
    let invalid = api.execute(Request {
        api_version: 1,
        request_id: "invalid-group".into(),
        command: Command::SetEmojiCategory {
            group: Some("not real".into()),
        },
    });
    assert_eq!(invalid["error"]["code"], "invalid_category");
    call(
        &mut api,
        Command::SetEmojiCategory {
            group: Some("People & Body".into()),
        },
    );
    let mut cursor = None;
    let mut people = std::collections::HashSet::new();
    loop {
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 1000,
                cursor,
            },
        );
        for event in page["events"].as_array().unwrap() {
            assert_eq!(event["category"]["group"], "People & Body");
            assert!(people.insert(event["id"].as_i64().unwrap()));
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(people.len() as u64, page["total"].as_u64().unwrap());
            break;
        }
    }
    assert_eq!(people.len(), 2261);
    call(&mut api, Command::SetEmojiCategory { group: None });
    let reset = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: None,
        },
    );
    assert_eq!(reset["total"], 3781);
}

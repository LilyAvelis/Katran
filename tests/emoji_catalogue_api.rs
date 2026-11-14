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

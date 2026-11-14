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
    assert!(first["total"].as_u64().unwrap() > 4000, "{first}");
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

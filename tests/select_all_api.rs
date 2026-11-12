use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::{Value, json};

fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "select-all".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn browse(api: &mut Service, query: &str) -> Value {
    call(
        api,
        Command::BrowseShelfSorted {
            query: query.into(),
            limit: 1,
            cursor: None,
        },
    )
}

#[test]
fn selects_current_space_and_search_across_pages() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let a = folder.path().join("alpha-one.bin");
    let b = folder.path().join("alpha-two.bin");
    let c = folder.path().join("beta.bin");
    for path in [&a, &b, &c] {
        std::fs::write(path, b"synthetic").unwrap();
    }
    let mut api = Service::open(data.path(), 1_000_000).unwrap();
    let added = call(
        &mut api,
        Command::AddFiles {
            paths: vec![a.clone(), b.clone(), c.clone()],
        },
    );
    let bound_ids: Vec<_> = added["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["event"]["id"].as_i64().unwrap())
        .collect();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    browse(&mut api, "alpha");
    let state = call(
        &mut api,
        Command::RunAction {
            id: "select_all".into(),
        },
    );
    let selected = state["selected"].as_array().unwrap();
    assert_eq!(selected.len(), 2);
    assert!(
        selected
            .iter()
            .all(|id| bound_ids[..2].contains(&id.as_i64().unwrap()))
    );
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    browse(&mut api, "alpha");
    let state = call(&mut api, Command::SelectAll);
    let selected = state["selected"].as_array().unwrap();
    assert_eq!(selected.len(), 2);
    assert!(selected.iter().all(|id| id.as_i64().unwrap() < 0));
    browse(&mut api, "no matches");
    assert_eq!(call(&mut api, Command::SelectAll)["selected"], json!([]));
    for path in [&a, &b, &c] {
        assert_eq!(std::fs::read(path).unwrap(), b"synthetic");
    }
}

#[test]
fn selects_folder_entries_beyond_first_thousand() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    for index in 0..1001 {
        std::fs::write(folder.path().join(format!("item-{index:04}.txt")), b"test").unwrap();
    }
    let mut api = Service::open(data.path(), 1_000_000).unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    browse(&mut api, "");
    let state = call(&mut api, Command::SelectAll);
    assert_eq!(state["selected"].as_array().unwrap().len(), 1001);
}

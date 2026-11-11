use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::{Value, json};
use std::collections::HashMap;

fn call(service: &mut Service, command: Command) -> Value {
    let response = service.execute(Request {
        api_version: 1,
        request_id: "folder-import".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

#[test]
fn explicit_folder_import_goes_to_visible_categories_and_preserves_sources() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let fixtures = [
        ("note.md", "Привет 🐄\n".as_bytes().to_vec()),
        ("cells.csv", "\u{feff}имя,,значение\r\nA,,Б\r\n".as_bytes().to_vec()),
        ("bookmarks.html", b"<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p><DT><A HREF=\"https://example.test/?a=1&amp;b=2\">one</A><DT><A HREF=\"javascript:alert(1)\">bad</A></DL><p>".to_vec()),
        ("other.pdf", b"synthetic unsupported".to_vec()),
        ("picture.png", b"synthetic image reference".to_vec()),
    ];
    for (name, data) in &fixtures {
        std::fs::write(folder.path().join(name), data).unwrap();
    }
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    for tab in [ShelfTab::Text, ShelfTab::Tables, ShelfTab::Links] {
        call(&mut service, Command::SetTabEnabled { tab, enabled: true });
    }
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let browse = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let ids: HashMap<_, _> = browse["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["name"].as_str().unwrap().to_owned(),
                v["id"].as_i64().unwrap(),
            )
        })
        .collect();
    let names = [
        "note.md",
        "cells.csv",
        "bookmarks.html",
        "other.pdf",
        "picture.png",
    ];
    let selected = names.iter().map(|name| ids[*name]).collect();
    let receipt = call(&mut service, Command::AddFolderFiles { ids: selected });
    let items = receipt["items"].as_array().unwrap();
    assert_eq!(items.len(), names.len());
    assert_eq!(items[0]["event"]["kind"], "text");
    let text_id = items[0]["event"]["event"]["id"].as_i64().unwrap();
    assert_eq!(
        call(&mut service, Command::GetText { id: text_id })["text"],
        "Привет 🐄\n"
    );
    assert_eq!(items[1]["event"]["kind"], "table");
    let table_id = items[1]["event"]["event"]["id"].as_i64().unwrap();
    assert_eq!(
        call(&mut service, Command::GetTable { id: table_id })["table"]["rows"],
        json!([["имя", "", "значение"], ["A", "", "Б"]])
    );
    assert_eq!(items[2]["ok"], false);
    assert_eq!(items[2]["event"]["imported"], 1);
    assert_eq!(items[2]["event"]["failed"], 1);
    assert_eq!(
        items[2]["event"]["items"][0]["url"],
        "https://example.test/?a=1&b=2"
    );
    assert_eq!(
        items[2]["event"]["items"][1]["error"]["code"],
        "invalid_link"
    );
    assert_eq!(items[3]["ok"], false);
    assert_eq!(items[3]["error"]["code"], "unsupported_import");
    assert_eq!(items[4]["event"]["kind"], "image");
    for (name, data) in fixtures {
        assert_eq!(std::fs::read(folder.path().join(name)).unwrap(), data);
    }

    for (tab, id) in [(ShelfTab::Text, text_id), (ShelfTab::Tables, table_id)] {
        call(&mut service, Command::SetTab { tab });
        let page = call(
            &mut service,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 100,
                cursor: None,
            },
        );
        assert!(
            page["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["id"] == id)
        );
    }
}

#[test]
fn disabled_or_changed_source_is_a_failure_without_import() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("note.txt");
    std::fs::write(&path, b"original").unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let browse = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let id = browse["events"][0]["id"].as_i64().unwrap();
    let result = call(&mut service, Command::AddFolderFiles { ids: vec![id] });
    assert_eq!(result["items"][0]["error"]["code"], "tab_disabled");
    call(
        &mut service,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    std::fs::write(&path, b"changed length").unwrap();
    let result = call(&mut service, Command::AddFolderFiles { ids: vec![id] });
    assert_eq!(result["items"][0]["error"]["code"], "stale_selection");
    assert_eq!(std::fs::read(path).unwrap(), b"changed length");
}

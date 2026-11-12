use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;

fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "actions-presentation".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

fn find<'a>(actions: &'a Value, id: &str) -> &'a Value {
    actions
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap()
}

#[test]
fn actions_have_contiguous_human_readable_groups_and_stable_bindings() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    let actions = call(&mut api, Command::Actions);
    let mut seen = std::collections::HashSet::new();
    let mut previous = String::new();
    for action in actions.as_array().unwrap() {
        let group = action["group"].as_str().unwrap();
        if group != previous {
            assert!(seen.insert(group), "group {group} appeared twice");
            previous = group.into();
        }
    }
    assert_eq!(find(&actions, "copy")["group"], "Выбранное");
    assert_eq!(find(&actions, "transfer_copy")["group"], "Полка ↔ Folder");
    assert_eq!(find(&actions, "gallery")["group"], "Отображение");
    assert_eq!(find(&actions, "copy")["shortcut"], "Ctrl C");
    assert_eq!(find(&actions, "delete")["label"], "Удалить выбранное…");
    assert_eq!(find(&actions, "tab_toggle")["label"], "Вернуться в Images");
    assert_eq!(
        find(&actions, "transfer_copy")["label"],
        "Копировать из Folder в Катран"
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let images = call(&mut api, Command::Actions);
    assert_eq!(find(&images, "tab_toggle")["label"], "Перейти в Folder");
    assert_eq!(
        find(&images, "transfer_copy")["label"],
        "Копировать в Folder"
    );
}

use katran::{Command, Request, Service, interaction::ShelfTab};
fn call(s: &mut Service, c: Command) -> serde_json::Value {
    s.execute(Request {
        api_version: 1,
        request_id: "tabs".into(),
        command: c,
    })
}
#[test]
fn positions_follow_persisted_order_and_tab_memory_uses_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut s = Service::open(root.path(), 1000).unwrap();
    assert_eq!(
        call(&mut s, Command::SelectTabPosition { position: 2 })["result"]["tab"],
        "screenshots"
    );
    assert_eq!(
        call(
            &mut s,
            Command::MoveTab {
                tab: ShelfTab::Downloads,
                index: 0
            }
        )["ok"],
        true
    );
    assert_eq!(call(&mut s, Command::ToggleTab)["result"]["tab"], "folder");
    assert_eq!(
        call(&mut s, Command::ToggleTab)["result"]["tab"],
        "screenshots"
    );
    assert_eq!(
        call(&mut s, Command::SelectTabPosition { position: 1 })["result"]["tab"],
        "downloads"
    );
    assert_eq!(
        call(
            &mut s,
            Command::MoveTab {
                tab: ShelfTab::All,
                index: 0
            }
        )["ok"],
        false
    );
    drop(s);
    let mut s = Service::open(root.path(), 1000).unwrap();
    assert_eq!(
        call(&mut s, Command::SelectTabPosition { position: 1 })["result"]["tab"],
        "downloads"
    );
    let defaults: katran::settings::Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.tab_order, katran::tab_layout::default_order());
}

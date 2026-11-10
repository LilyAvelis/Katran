use katran::{Command, Request, Service, interaction::View, settings::Settings};
fn call(s: &mut Service, c: Command) -> serde_json::Value {
    s.execute(Request {
        api_version: 1,
        request_id: "settings-test".into(),
        command: c,
    })
}
#[test]
fn settings_persist_validate_and_do_not_delete_cache() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    let settings = Settings {
        sort: Default::default(),
        quota_bytes: 200,
        gallery_columns: 4,
        stream_rows: 3,
        tab_order: katran::tab_layout::default_order(),
        hidden_tabs: Vec::new(),
        text_enabled: false,
        links_enabled: false,
        tables_enabled: false,
        card_width: 280.0,
        default_view: View::List,
        capture_on_start: false,
        sharex_folder: None,
        downloads_folder: None,
        downloads_enabled: true,
    };
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["ok"],
        true
    );
    assert_eq!(
        call(&mut api, Command::ShowSettings)["result"]["settings_open"],
        true
    );
    drop(api);
    let mut api = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["card_width"],
        280.0
    );
    assert_eq!(
        call(&mut api, Command::Interaction)["result"]["view"],
        "list"
    );
    call(
        &mut api,
        Command::IngestBytes {
            name: "test.bin".into(),
            bytes: vec![1, 2],
            source: "test".into(),
        },
    );
    let mut settings = Settings::defaults(1);
    settings.card_width = 280.0;
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["error"]["code"],
        "invalid_settings"
    );
    assert_eq!(call(&mut api, Command::Status)["result"]["cached_bytes"], 2);
}

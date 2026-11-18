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
        language: katran::settings::Language::English,
        translation_target: Default::default(),
        theme: Default::default(),
        window_resizable: false,
        desktop_wallpaper_id: None,
        sort_notifications: true,
        pods_enabled: false,
        pods_tile_size: 96,
        pods_columns: 9,
        links_columns: 8,
        overlay_hotkey: String::new(),
        tab_hotkeys: Default::default(),
        emoji_font: "apple".into(),
        sort: Default::default(),
        tab_preferences: Default::default(),
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
        search_keyboard_only: true,
        link_favicons_online: true,
        folder_ai_gpu: false,
        video_gpu_acceleration: false,
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
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["search_keyboard_only"],
        true
    );
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["link_favicons_online"],
        true
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

#[test]
fn capture_defaults_on_and_explicit_pause_preference_survives() {
    assert!(Settings::default().capture_on_start);
    assert!(!Settings::default().search_keyboard_only);
    assert!(!Settings::default().link_favicons_online);
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["capture_on_start"],
        true
    );
    let settings = Settings {
        capture_on_start: false,
        ..Settings::default()
    };
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["ok"],
        true
    );
    drop(api);
    let mut api = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["capture_on_start"],
        false
    );
}

#[test]
fn video_gpu_defaults_off_and_persists_through_settings_api() {
    let old: Settings = serde_json::from_str("{}").unwrap();
    assert!(!old.video_gpu_acceleration);
    assert!(
        serde_json::to_value(&old)
            .unwrap()
            .get("video_gpu_acceleration")
            .is_none()
    );
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 100).unwrap();
    let mut settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)["result"].clone()).unwrap();
    assert!(!settings.video_gpu_acceleration);
    settings.video_gpu_acceleration = true;
    assert_eq!(
        call(&mut service, Command::SetSettings { settings })["ok"],
        true
    );
    drop(service);
    let mut service = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        call(&mut service, Command::GetSettings)["result"]["video_gpu_acceleration"],
        true
    );
}

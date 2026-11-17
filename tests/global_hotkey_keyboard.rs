use katran::{Command, Request, Service, interaction::ShelfTab, overlay::Hotkey};
#[test]
fn punctuation_navigation_and_modifier_combinations_round_trip() {
    for binding in [
        "Ctrl Shift /",
        "Alt Shift Comma",
        "Ctrl Alt Shift Backslash",
        "Win Home",
        "Ctrl PageDown",
        "Ctrl Shift Equals",
        "Alt Quote",
        "Ctrl Backtick",
        "Ctrl OpenBracket",
        "Ctrl Delete",
    ] {
        let key = Hotkey::parse(binding).unwrap();
        assert_eq!(Hotkey::parse(&key.canonical()).unwrap(), key);
    }
    assert_eq!(Hotkey::parse("Ctrl Shift /").unwrap().key, 0xbf);
    assert!(Hotkey::parse("Ctrl Shift").is_err());
    assert!(Hotkey::parse("Ctrl Shift Shift /").is_err());
    assert!(Hotkey::parse("Ctrl F25").is_err());
}
#[test]
fn text_global_ctrl_shift_slash_is_saved_and_reloaded() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let result = api.execute(Request {
        api_version: 1,
        request_id: "slash".into(),
        command: Command::SetTabGlobalHotkey {
            tab: ShelfTab::Text,
            binding: Some("Ctrl Shift /".into()),
        },
    });
    assert_eq!(result["ok"], true, "{result}");
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let result = api.execute(Request {
        api_version: 1,
        request_id: "reload".into(),
        command: Command::GetSettings,
    });
    assert_eq!(result["result"]["tab_hotkeys"]["text"], "Ctrl+Shift+Slash");
}

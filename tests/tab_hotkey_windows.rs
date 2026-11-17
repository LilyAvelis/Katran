#![cfg(windows)]

#[test]
#[ignore = "briefly registers a synthetic global key; no input or desktop actions"]
fn registration_conflict_and_drop_release_the_chord() {
    use katran::{overlay::Hotkey, overlay_windows::HotkeyListener};
    let key = Hotkey::parse("Ctrl+Alt+Shift+F24").unwrap();
    let listener = HotkeyListener::start(key, || {}).unwrap();
    let duplicate = HotkeyListener::start(key, || {});
    assert!(duplicate.is_err(), "duplicate registration must fail");
    drop(listener);
    let rebound = HotkeyListener::start(key, || {}).unwrap();
    drop(rebound);
}

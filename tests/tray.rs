use katran::tray::{Event, callback_event, menu_event};

#[test]
fn notifications_map_one_activation_and_ignore_other_events() {
    assert_eq!(callback_event(0x0400), Some(Event::Toggle));
    assert_eq!(callback_event(0x0401), Some(Event::Toggle));
    assert_eq!(callback_event(0x0202), None); // no duplicate from WM_LBUTTONUP
    assert_eq!(callback_event(0x007b), None); // context menu is handled separately
}

#[test]
fn menu_ids_are_explicit_and_dismissal_does_nothing() {
    assert_eq!(menu_event(1), Some(Event::Show));
    assert_eq!(menu_event(2), Some(Event::Hide));
    assert_eq!(menu_event(3), Some(Event::Quit));
    assert_eq!(menu_event(0), None);
    assert_eq!(menu_event(999), None);
}

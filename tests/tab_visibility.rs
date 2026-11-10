use katran::{interaction::ShelfTab::*, settings::Settings, tab_layout};

#[test]
fn old_settings_and_filtered_positions_are_compatible() {
    let mut settings: Settings = serde_json::from_str(r#"{"downloads_enabled":false}"#).unwrap();
    assert!(settings.hidden_tabs.is_empty());
    assert!(settings.tab_enabled(All));
    assert!(!settings.tab_enabled(Downloads));
    assert!(!settings.tab_enabled(Tables));
    settings.hidden_tabs.push(Images);
    assert_eq!(settings.visible_tabs(), vec![Screenshots]);
    assert_eq!(
        tab_layout::tab_at_visible(&settings.visible_tabs(), 0).unwrap(),
        All
    );
    assert_eq!(
        tab_layout::tab_at_visible(&settings.visible_tabs(), 1).unwrap(),
        Screenshots
    );
    assert!(tab_layout::tab_at_visible(&settings.visible_tabs(), 2).is_err());
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.hidden_tabs, vec![Images]);
    assert_eq!(
        restored.tab_order,
        vec![Images, Screenshots, Downloads, Text, Links, Tables]
    );
}

#[test]
fn hidden_tabs_reject_all_and_duplicates() {
    let mut settings = Settings {
        hidden_tabs: vec![All],
        ..Settings::default()
    };
    assert!(settings.validate(0).is_err());
    settings.hidden_tabs = vec![Images, Images];
    assert!(settings.validate(0).is_err());
    settings.hidden_tabs = vec![Downloads, Screenshots, Images];
    assert!(settings.validate(0).is_ok());
    assert!(settings.visible_tabs().is_empty());
    assert_eq!(tab_layout::tab_at_visible(&[], 0).unwrap(), All);
}

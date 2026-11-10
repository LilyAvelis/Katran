use katran::interaction::{Interaction, SelectionMode, ShelfTab};

#[test]
fn tabs_default_to_images_and_toggle_remembers_last_non_all_tab() {
    let mut state = Interaction::default();
    assert_eq!(state.tab, ShelfTab::Images);
    assert_eq!(state.last_tab, ShelfTab::Images);
    state.toggle_tab();
    assert_eq!(state.tab, ShelfTab::All);
    state.toggle_tab();
    assert_eq!(state.tab, ShelfTab::Images);
    state.set_tab(ShelfTab::Screenshots);
    state.set_tab(ShelfTab::All);
    state.set_tab(ShelfTab::All);
    assert_eq!(state.last_tab, ShelfTab::Screenshots);
    state.toggle_tab();
    assert_eq!(state.tab, ShelfTab::Screenshots);
    assert_eq!(serde_json::to_value(state).unwrap()["tab"], "screenshots");
    assert_eq!(
        serde_json::from_str::<ShelfTab>("\"all\"").unwrap(),
        ShelfTab::All
    );
}

#[test]
fn filtering_preserves_selection_but_keeps_cursor_and_preview_visible() {
    let mut state = Interaction::default();
    state.visible(vec![1, 2, 3]);
    state.select(1, SelectionMode::Replace);
    state.select(3, SelectionMode::Toggle);
    state.set_preview(Some(3));
    state.set_tab(ShelfTab::Screenshots);
    state.visible(vec![2]);
    assert_eq!(state.selected, vec![1, 3]);
    assert_eq!(state.cursor, Some(2));
    assert_eq!(state.preview, None);
    state.set_tab(ShelfTab::All);
    state.visible(vec![1, 2, 3]);
    assert_eq!(state.selected, vec![1, 3]);
    state.visible(vec![]);
    assert_eq!(state.selected, vec![1, 3]);
    assert_eq!(state.cursor, None);
    state.visible(vec![1, 2, 3]);
    state.select(2, SelectionMode::Replace);
    assert_eq!(state.selected, vec![2]);
}

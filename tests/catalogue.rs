use katran::{actions, interaction::Interaction, shortcuts::Shortcuts};
#[test]
fn catalogue_has_unique_routable_ids_and_matching_shortcuts() {
    let mut ids = std::collections::HashSet::new();
    let keys = Shortcuts::default().catalogue();
    for def in actions::DEFS {
        assert!(ids.insert(def.id));
        let mut context = Interaction::default();
        if matches!(def.id, "folder_favorites" | "folder_history") {
            assert!(actions::resolve(def.id, &context, false).is_none());
            context.tab = katran::interaction::ShelfTab::All;
        }
        assert!(
            actions::resolve(def.id, &context, false).is_some(),
            "{}",
            def.id
        );
        let key = keys.iter().find(|k| k.id == def.id).unwrap();
        assert_eq!(key.binding.as_deref().unwrap_or(""), def.default_binding);
        assert_eq!(key.rebindable, def.rebindable);
    }
    assert!(actions::resolve("unknown", &Interaction::default(), false).is_none());
    assert!(
        !actions::DEFS
            .iter()
            .find(|d| d.id == "commands")
            .unwrap()
            .in_palette
    );
}

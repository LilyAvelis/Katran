use katran::{actions, interaction::Interaction, shortcuts::Shortcuts};
#[test]
fn catalogue_has_unique_routable_ids_and_matching_shortcuts() {
    let mut ids = std::collections::HashSet::new();
    let keys = Shortcuts::default().catalogue();
    for def in actions::DEFS {
        assert!(ids.insert(def.0));
        assert!(actions::resolve(def.0, &Interaction::default(), false).is_some());
        let key = keys.iter().find(|k| k.id == def.0).unwrap();
        assert_eq!(key.binding.as_deref().unwrap_or(""), def.2);
        assert_eq!(key.rebindable, def.3);
    }
    assert!(actions::resolve("unknown", &Interaction::default(), false).is_none());
    assert!(!actions::DEFS.iter().find(|d| d.0 == "commands").unwrap().5);
}

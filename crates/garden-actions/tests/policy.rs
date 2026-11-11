use garden_actions::{
    Assignment, Conflict, Definition, Error, Overrides, binding, normalize, plan_set,
    validate_overrides,
};

const FIRST: &[Definition<'_>] = &[
    Definition {
        id: "open",
        label: "Open",
        default_binding: "Ctrl O / F2",
        rebindable: true,
    },
    Definition {
        id: "save",
        label: "Save",
        default_binding: "Ctrl S / F3",
        rebindable: true,
    },
    Definition {
        id: "fixed",
        label: "Fixed",
        default_binding: "Escape",
        rebindable: false,
    },
];
const SECOND: &[Definition<'_>] = &[
    Definition {
        id: "open",
        label: "Launch",
        default_binding: "Ctrl L",
        rebindable: true,
    },
    Definition {
        id: "close",
        label: "Close",
        default_binding: "Ctrl Q",
        rebindable: true,
    },
];
fn allow(_: &str, _: &str) -> bool {
    false
}

#[test]
fn normalization_preserves_original_alias_and_validation_rules() {
    assert_eq!(
        normalize("shift+control+a / ←").unwrap(),
        "Ctrl Shift A / ArrowLeft"
    );
    assert_eq!(normalize("cmd Esc / ,").unwrap(), "Super Escape / Comma");
    assert_eq!(normalize("").unwrap(), "");
    for bad in ["Ctrl", "ctrl a b", "escape", "Ctrl / A", "F36"] {
        assert!(
            matches!(normalize(bad), Err(Error::InvalidShortcut { .. })),
            "{bad}"
        );
    }
}

#[test]
fn conflict_replacement_preserves_other_alternatives_and_input() {
    let before = Overrides::new();
    let error = plan_set(
        FIRST,
        &before,
        "open",
        Assignment::Set(Some("Ctrl S")),
        false,
        allow,
    )
    .unwrap_err();
    assert_eq!(
        error,
        Error::Conflict {
            conflicts: vec![Conflict {
                id: "save".into(),
                label: "Save".into()
            }]
        }
    );
    assert!(before.is_empty());
    let after = plan_set(
        FIRST,
        &before,
        "open",
        Assignment::Set(Some("Ctrl S")),
        true,
        allow,
    )
    .unwrap();
    assert_eq!(binding(FIRST, &after, "save"), "F3");
    assert_eq!(binding(FIRST, &after, "open"), "Ctrl S");
    assert_eq!(binding(FIRST, &before, "open"), "Ctrl O / F2");
    assert_eq!(binding(SECOND, &before, "open"), "Ctrl L");
}

#[test]
fn reset_checks_conflicts_and_explicit_unbinding_differs_from_absence() {
    let start = plan_set(
        FIRST,
        &Overrides::new(),
        "open",
        Assignment::Set(None),
        false,
        allow,
    )
    .unwrap();
    assert_eq!(start.get("open"), Some(&None));
    let other = plan_set(
        FIRST,
        &start,
        "save",
        Assignment::Set(Some("Ctrl O")),
        false,
        allow,
    )
    .unwrap();
    assert!(matches!(
        plan_set(FIRST, &other, "open", Assignment::Reset, false, allow),
        Err(Error::Conflict { .. })
    ));
    let reset = plan_set(FIRST, &other, "open", Assignment::Reset, true, allow).unwrap();
    assert!(!reset.contains_key("open"));
    assert_eq!(binding(FIRST, &reset, "open"), "Ctrl O / F2");
    assert_eq!(reset.get("save"), Some(&None));
}

#[test]
fn protected_reserved_and_catalogue_order_are_preserved() {
    let err = plan_set(
        FIRST,
        &Overrides::new(),
        "open",
        Assignment::Set(Some("Escape")),
        true,
        allow,
    )
    .unwrap_err();
    assert!(matches!(err, Error::ReservedShortcut { chord: None, .. }));
    let err = plan_set(
        FIRST,
        &Overrides::new(),
        "fixed",
        Assignment::Set(None),
        true,
        allow,
    )
    .unwrap_err();
    assert_eq!(err, Error::ProtectedAction { id: "fixed".into() });
    let err = plan_set(
        FIRST,
        &Overrides::new(),
        "open",
        Assignment::Set(Some("Tab")),
        true,
        |id, chord| id == "open" && chord == "Tab",
    )
    .unwrap_err();
    assert!(matches!(err, Error::ReservedShortcut { chord: Some(chord), .. } if chord == "Tab"));
    let err = plan_set(
        FIRST,
        &Overrides::new(),
        "missing",
        Assignment::Reset,
        false,
        allow,
    )
    .unwrap_err();
    assert_eq!(
        err,
        Error::UnknownAction {
            id: "missing".into()
        }
    );
    let err = plan_set(
        FIRST,
        &Overrides::new(),
        "open",
        Assignment::Set(Some("Ctrl S / Escape")),
        false,
        allow,
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::ReservedShortcut { conflicts, .. } if conflicts.iter().map(|c| c.id.as_str()).collect::<Vec<_>>() == ["save", "fixed"])
    );
}

#[test]
fn loading_normalizes_without_rejecting_conflicts_or_reserved_values() {
    let overrides = Overrides::from([
        (String::from("open"), Some("ctrl+s".into())),
        (String::from("save"), Some("Escape".into())),
    ]);
    let loaded = validate_overrides(FIRST, &overrides).unwrap();
    assert_eq!(binding(FIRST, &loaded, "open"), "Ctrl S");
    assert_eq!(binding(FIRST, &loaded, "save"), "Escape");
    assert!(matches!(
        validate_overrides(SECOND, &overrides),
        Err(Error::UnknownAction { .. })
    ));
    assert!(matches!(
        validate_overrides(FIRST, &Overrides::from([("fixed".into(), None)])),
        Err(Error::ProtectedAction { .. })
    ));
}

#[test]
fn malformed_catalogues_are_rejected_before_planning_or_loading() {
    use garden_actions::validate_catalogue;
    let def = |id, default_binding| Definition {
        id,
        label: "Action",
        default_binding,
        rebindable: true,
    };
    let duplicate = [def("same", "Ctrl S"), def("same", "Ctrl O")];
    let empty = [def("  ", "")];
    let aliases = [def("one", "Ctrl+S"), def("two", "Ctrl O")];
    let invalid = [def("one", "Ctrl")];
    for catalogue in [&duplicate[..], &empty[..], &aliases[..], &invalid[..]] {
        assert!(matches!(
            validate_catalogue(catalogue),
            Err(Error::InvalidCatalogue { .. })
        ));
        assert!(matches!(
            validate_overrides(catalogue, &Overrides::new()),
            Err(Error::InvalidCatalogue { .. })
        ));
        let original = Overrides::new();
        assert!(matches!(
            plan_set(
                catalogue,
                &original,
                "two",
                Assignment::Set(Some("Ctrl S")),
                true,
                |_, _| panic!("invalid catalogue must be rejected first")
            ),
            Err(Error::InvalidCatalogue { .. })
        ));
        assert!(original.is_empty());
    }
    let message = validate_catalogue(&aliases).unwrap_err().to_string();
    assert!(
        message.contains("Ctrl+S") && message.contains("Ctrl S"),
        "{message}"
    );
    let valid = [def("one", "Ctrl S"), def("two", "Ctrl S"), def("three", "")];
    assert!(
        validate_catalogue(&valid).is_ok(),
        "overlaps are host-context dependent"
    );
    assert!(
        matches!(plan_set(&valid, &Overrides::new(), "three", Assignment::Set(Some("Ctrl S")), false, allow), Err(Error::Conflict { conflicts }) if conflicts.len() == 2)
    );
}

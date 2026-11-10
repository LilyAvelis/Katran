use katran::sorting::*;

#[test]
fn natural_numbers_are_unbounded_and_case_insensitive() {
    let mut values = [
        "file10.png",
        "File2.png",
        "file1000000000000000000000000000000.png",
        "file9.png",
    ];
    values.sort_by_key(|name| natural_key(name));
    assert_eq!(
        values,
        [
            "File2.png",
            "file9.png",
            "file10.png",
            "file1000000000000000000000000000000.png"
        ]
    );
    assert_eq!(natural_key("FILE0002"), natural_key("file2"));
    assert!(natural_key("file") < natural_key("file0"));
}

#[test]
fn extension_is_last_suffix_and_dotfiles_have_no_suffix() {
    assert_eq!(extension_key("archive.TAR.GZ"), "gz");
    assert_eq!(extension_key(".gitignore"), "");
    assert_eq!(extension_key("LICENSE"), "");
    assert_eq!(extension_key("picture."), "");
    assert_eq!(name_key("ЯA"), "яa");
}

#[test]
fn origin_uses_recorded_domain_then_app_then_known_adapter() {
    assert_eq!(
        origin_key(Some(" Example.COM "), Some("Browser"), "downloads"),
        Some("example.com".into())
    );
    assert_eq!(
        origin_key(Some("  "), Some("Firefox"), "downloads"),
        Some("firefox".into())
    );
    assert_eq!(
        origin_key(None, None, "downloads"),
        Some("downloads".into())
    );
    assert_eq!(
        origin_key(None, None, "clipboard_file"),
        Some("clipboard".into())
    );
    assert_eq!(origin_key(None, None, "unrecorded"), None);
    assert_eq!(
        origin_key(None, None, "clipboard_link"),
        Some("clipboard".into())
    );
}

#[test]
fn shuffle_is_stable_seeded_and_not_reversed() {
    let keys: Vec<_> = (1..101).map(|id| shuffle_key(id, 42)).collect();
    assert_eq!(
        keys,
        (1..101).map(|id| shuffle_key(id, 42)).collect::<Vec<_>>()
    );
    assert_ne!(
        keys,
        (1..101).map(|id| shuffle_key(id, 43)).collect::<Vec<_>>()
    );
    assert!(keys.iter().all(|key| *key >= 0));
    assert!(
        !SortSpec {
            mode: SortMode::Shuffle,
            reverse: true,
            seed: 42
        }
        .normalized()
        .reverse
    );
    assert!(
        SortSpec {
            mode: SortMode::Name,
            reverse: true,
            seed: 0
        }
        .normalized()
        .reverse
    );
    assert_eq!(SortSpec::default().mode, SortMode::Arrival);
}

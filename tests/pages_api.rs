use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;

fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "pages".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn browse(api: &mut Service) -> Vec<Value> {
    call(
        api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 30,
            cursor: None,
        },
    )["events"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn pages_pins_and_tags_survive_edits_and_restart() {
    use katran::sorting::SortMode;
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let tab = ShelfTab::Pages;
    call(&mut api, Command::SetTabEnabled { tab, enabled: true });
    call(&mut api, Command::SetTab { tab });
    assert!(browse(&mut api).is_empty());
    let vault = root.path().join("Katran Pages");
    let note = vault.join("Заметка.md");
    std::fs::write(&note, "First version").unwrap();
    std::fs::write(vault.join("Other.md"), "Unpinned note").unwrap();
    let page = browse(&mut api)
        .into_iter()
        .find(|row| row["name"] == "Заметка.md")
        .unwrap();
    let id = page["id"].as_i64().unwrap();
    let tag = |slot| Command::AssignTagSlot {
        ids: vec![id],
        slot,
    };
    assert_eq!(
        call(&mut api, tag(Some(2)))["items"][0]["outcome"],
        "not_pinned"
    );
    assert_eq!(
        call(&mut api, Command::TogglePins { ids: vec![id] })["items"][0]["pinned"],
        true
    );
    assert_eq!(
        call(&mut api, tag(Some(2)))["items"][0]["outcome"],
        "applied"
    );
    let slots = call(&mut api, Command::ListTagSlots);
    assert_eq!(slots["slots"].as_array().unwrap().len(), 9);
    call(
        &mut api,
        Command::ReorderTagSlots {
            tab,
            order: vec![2, 1, 3, 4, 5, 6, 7, 8, 9],
            expected_revision: slots["revision"].as_str().unwrap().to_owned(),
        },
    );
    assert_eq!(
        call(&mut api, Command::GetTagSlots { ids: vec![id] })["items"][0]["slot"],
        1
    );
    call(
        &mut api,
        Command::SetSort {
            mode: SortMode::Tags,
            reverse: false,
            seed: None,
        },
    );
    assert_eq!(browse(&mut api)[0]["id"], id);
    call(&mut api, Command::FilterTagSlot { slot: Some(1) });
    assert_eq!(browse(&mut api).len(), 1);
    call(&mut api, Command::FilterTagSlot { slot: Some(1) });
    call(&mut api, Command::TogglePinnedView);
    assert_eq!(browse(&mut api).len(), 1);
    let result = call(
        &mut api,
        Command::DeleteSelection {
            ids: vec![id],
            confirmed: false,
        },
    );
    assert_eq!(result["items"][0]["skipped_pinned"], true);
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "First version");
    drop(api);

    std::fs::write(&note, "Edited in the vault").unwrap();
    // Change the directory ordering so a saved session ID could not pass.
    std::fs::write(vault.join("A.md"), "New note").unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    call(&mut api, Command::SetTab { tab });
    let page = &browse(&mut api)[0];
    let reopened = page["id"].as_i64().unwrap();
    assert_ne!(reopened, id);
    assert_eq!(page["name"], "Заметка.md");
    assert_eq!(page["properties"]["pinned"], true);
    assert_eq!(
        call(&mut api, Command::GetTagSlot { id: reopened })["slot"],
        1
    );
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![reopened],
        },
    );
    assert!(call(&mut api, Command::GetTagSlot { id: reopened })["slot"].is_null());
    assert_eq!(
        std::fs::read_to_string(&note).unwrap(),
        "Edited in the vault"
    );
}

#[cfg(windows)]
#[test]
fn explicit_move_writes_one_markdown_and_observes_external_edits() {
    use katran::interaction::SelectionMode;
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    let texts = ["Small note", "# A longer note\n\nBody"];
    let ids: Vec<_> = texts
        .iter()
        .map(|text| {
            call(
                &mut api,
                Command::IngestText {
                    text: (*text).into(),
                    source: "test".into(),
                },
            )["id"]
                .as_i64()
                .unwrap()
        })
        .collect();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pages,
        },
    );
    assert!(browse(&mut api).is_empty());
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    for id in ids.iter().rev() {
        call(
            &mut api,
            Command::Select {
                id: *id,
                mode: SelectionMode::Toggle,
            },
        );
    }
    let moved = call(&mut api, Command::MoveToPages);
    assert_eq!(moved["pages_moved"], 2);
    assert_eq!(moved["pages_created"], 1);
    assert!(browse(&mut api).is_empty());
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pages,
        },
    );
    let notes = browse(&mut api);
    assert_eq!(notes.len(), 1);
    let paths: Vec<_> = notes
        .iter()
        .map(|item| std::path::PathBuf::from(item["path"].as_str().unwrap()))
        .collect();
    assert!(
        paths
            .iter()
            .all(|path| path.parent().unwrap().ends_with("Katran Pages"))
    );
    assert_eq!(
        std::fs::read_to_string(&paths[0]).unwrap(),
        format!("{}\n\n{}", texts[1], texts[0])
    );
    std::fs::write(&paths[0], "# Edited in Obsidian").unwrap();
    std::fs::write(
        paths[0].with_file_name("Created in Obsidian.md"),
        "# Second note",
    )
    .unwrap();
    let refreshed = browse(&mut api);
    let note = refreshed
        .iter()
        .find(|note| note["path"] == notes[0]["path"])
        .unwrap();
    assert_eq!(note["id"], notes[0]["id"]);
    assert_ne!(note["revision"], notes[0]["revision"]);
    assert_eq!(
        call(
            &mut api,
            Command::GetDocumentText {
                id: note["id"].as_i64().unwrap()
            }
        )["text"],
        "# Edited in Obsidian"
    );

    struct FixtureTrash;
    impl katran::downloads::Trash for FixtureTrash {
        fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
            assert!(path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .starts_with(".katran-recovery-")
            }));
            std::fs::remove_file(path)?;
            Ok(())
        }
    }
    api.set_trash(Box::new(FixtureTrash));
    fn pack(api: &mut Service, events: &[Value]) {
        let ids: Vec<_> = events
            .iter()
            .map(|event| event["id"].as_i64().unwrap())
            .collect();
        call(api, Command::SetVisible { ids: ids.clone() });
        for id in ids {
            call(
                api,
                Command::Select {
                    id,
                    mode: SelectionMode::Toggle,
                },
            );
        }
        let job = call(api, Command::CreateImageCollection);
        let job_id = serde_json::from_value(job["job_id"].clone()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let reply = call(api, Command::PollJob { job_id });
            match reply["state"].as_str() {
                Some("succeeded") => break,
                Some("queued" | "running" | "ready") => {}
                _ => panic!("{reply}"),
            }
            assert!(std::time::Instant::now() < deadline, "{reply}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    pack(&mut api, &refreshed);
    let archives = browse(&mut api);
    assert_eq!(archives.len(), 1);
    assert!(archives[0]["name"].as_str().unwrap().ends_with(".bull"));
    pack(&mut api, &archives);
    let extracted = browse(&mut api);
    assert_eq!(extracted.len(), 2);
    assert!(
        extracted
            .iter()
            .all(|note| note["name"].as_str().unwrap().ends_with(".md"))
    );
}

#[test]
fn shift_move_follows_both_directions_of_visible_order() {
    use katran::interaction::SelectionMode;
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let tab = ShelfTab::Text;
    call(&mut api, Command::SetTabEnabled { tab, enabled: true });
    for reverse in [false, true] {
        call(&mut api, Command::SetTab { tab });
        let texts = if reverse {
            ["four", "five", "six"]
        } else {
            ["one", "two", "three"]
        };
        let ids: Vec<_> = texts
            .iter()
            .map(|text| {
                call(
                    &mut api,
                    Command::IngestText {
                        text: (*text).into(),
                        source: "test".into(),
                    },
                )["id"]
                    .as_i64()
                    .unwrap()
            })
            .collect();
        call(&mut api, Command::SetVisible { ids: ids.clone() });
        let (anchor, end) = if reverse { (2, 0) } else { (0, 2) };
        call(
            &mut api,
            Command::Select {
                id: ids[anchor],
                mode: SelectionMode::Replace,
            },
        );
        call(
            &mut api,
            Command::Select {
                id: ids[end],
                mode: SelectionMode::Range,
            },
        );
        assert_eq!(call(&mut api, Command::MoveToPages)["pages_created"], 1);
        call(
            &mut api,
            Command::SetTab {
                tab: ShelfTab::Pages,
            },
        );
        let expected = if reverse {
            texts.into_iter().rev().collect::<Vec<_>>().join("\n\n")
        } else {
            texts.join("\n\n")
        };
        assert!(browse(&mut api).iter().any(|event| {
            std::fs::read_to_string(event["path"].as_str().unwrap()).unwrap() == expected
        }));
    }
}

#[test]
fn page_reader_recognizes_embeds_and_preserves_literal_code() {
    use katran::pages::Section;
    let text = "# Note\n\n![[image.png|100]]\n\n![alt](media/photo.jpg)\n\n`![[example.jpg]]`";
    let sections = katran::pages::reader_sections(text);
    let embeds: Vec<_> = sections
        .iter()
        .filter_map(|section| match section {
            Section::Embed(target) => Some(target.as_str()),
            Section::Markdown(_) => None,
        })
        .collect();
    assert_eq!(embeds, ["image.png", "media/photo.jpg"]);
}

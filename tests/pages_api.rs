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

#[cfg(windows)]
#[test]
fn explicit_move_writes_flat_pages_and_observes_external_edits() {
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
    for id in &ids {
        call(
            &mut api,
            Command::Select {
                id: *id,
                mode: SelectionMode::Toggle,
            },
        );
    }
    assert_eq!(call(&mut api, Command::MoveToPages)["pages_moved"], 2);
    assert!(browse(&mut api).is_empty());
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pages,
        },
    );
    let notes = browse(&mut api);
    assert_eq!(notes.len(), 2);
    let paths: Vec<_> = notes
        .iter()
        .map(|item| std::path::PathBuf::from(item["path"].as_str().unwrap()))
        .collect();
    assert!(
        paths
            .iter()
            .all(|path| path.parent().unwrap().ends_with("Katran Pages"))
    );
    for text in texts {
        assert!(
            paths
                .iter()
                .any(|path| std::fs::read_to_string(path).unwrap() == text)
        );
    }
    std::fs::write(&paths[0], "# Edited in Obsidian").unwrap();
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

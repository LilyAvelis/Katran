use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
    settings::{Language, Settings, TranslationTarget},
};
use serde_json::Value;
use std::time::{Duration, Instant};

fn call(service: &mut Service, command: Command) -> Value {
    let response = service.execute(Request {
        api_version: 1,
        request_id: "translate-test".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

#[test]
fn target_follows_interface_and_explicit_choice_persists() {
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let mut settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    assert_eq!(settings.translation_target, TranslationTarget::Interface);
    assert_eq!(
        settings.translation_target.language(Language::English),
        Language::English
    );
    assert_eq!(
        settings.translation_target.language(Language::Russian),
        Language::Russian
    );
    settings.language = Language::English;
    settings.translation_target = TranslationTarget::Russian;
    call(&mut service, Command::SetSettings { settings });
    drop(service);
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    assert_eq!(
        settings.translation_target.language(settings.language),
        Language::Russian
    );
    assert_eq!(
        TranslationTarget::English.language(Language::Russian),
        Language::English
    );
}

#[test]
#[ignore = "requires approved manga fixture and isolated local OCR/Ollama components"]
fn selected_japanese_image_publishes_png_to_translate_with_interface_target() {
    let input = std::env::var_os("KATRAN_TRANSLATE_TEST_IMAGE").expect("approved input fixture");
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let mut settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    settings.language = Language::English;
    call(&mut service, Command::SetSettings { settings });
    let metadata = call(&mut service, Command::AiComponentsStatus);
    assert_eq!(metadata["image_translate"]["ready"], true, "{metadata}");
    let staging = profile.path().join("translate-work");
    let first = translate_page(&mut service, &input);
    assert_eq!(first["skipped_blocks"], 0);
    let session_dirs = || {
        std::fs::read_dir(&staging)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>()
    };
    let session = session_dirs();
    assert_eq!(session.len(), 1);

    let warm = translate_page(&mut service, &input);
    assert_eq!(warm["skipped_blocks"], 0);
    assert_eq!(session_dirs(), session);
    let mut results = vec![first, warm];
    if let Some(complex) = std::env::var_os("KATRAN_TRANSLATE_TEST_COMPLEX_IMAGE") {
        let result = translate_page(&mut service, &complex);
        assert!(result["skipped_blocks"].as_u64().unwrap() > 0);
        results.push(result);
    }
    for (tab, expected) in [(ShelfTab::Images, false), (ShelfTab::Translate, true)] {
        call(&mut service, Command::SetTab { tab });
        let page = browse(&mut service);
        for result in &results {
            assert_eq!(has_event(&page, &result["event"]["id"]), expected);
        }
    }
    drop(service);
    assert!(
        session_dirs().is_empty(),
        "session files survive service shutdown"
    );
}

#[test]
fn saved_translations_are_visible_with_tag_sort_and_exclusive_to_translate() {
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let event = call(
        &mut service,
        Command::IngestBytes {
            name: "result.png".into(),
            bytes: vec![1, 2, 3],
            source: "image_translation".into(),
        },
    );
    rusqlite::Connection::open(profile.path().join("index.sqlite3"))
        .unwrap()
        .execute(
            "INSERT INTO translated_images(event_id) VALUES(?1)",
            [event["id"].as_i64().unwrap()],
        )
        .unwrap();
    for (tab, expected) in [(ShelfTab::Images, false), (ShelfTab::Translate, true)] {
        call(&mut service, Command::SetTab { tab });
        call(
            &mut service,
            Command::SetSort {
                mode: katran::sorting::SortMode::Tags,
                reverse: false,
                seed: None,
            },
        );
        assert_eq!(has_event(&browse(&mut service), &event["id"]), expected);
        let mut reader = Service::open_read_only(profile.path()).unwrap();
        let snapshot = call(
            &mut reader,
            Command::BrowseTabSnapshot {
                tab,
                query: String::new(),
                limit: 10,
            },
        );
        assert_eq!(has_event(&snapshot, &event["id"]), expected);
    }
}

fn has_event(page: &Value, id: &Value) -> bool {
    page["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["id"] == *id)
}

fn browse(service: &mut Service) -> Value {
    call(
        service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 10,
            cursor: None,
        },
    )
}

fn translate_page(service: &mut Service, input: &std::ffi::OsStr) -> Value {
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("original.png");
    std::fs::copy(input, &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    call(
        service,
        Command::ChooseFolder {
            path: source_dir.path().to_owned(),
        },
    );
    let id = browse(service)["events"][0]["id"].as_i64().unwrap();
    call(service, Command::SetVisible { ids: vec![id] });
    call(
        service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let actions = call(service, Command::Actions);
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == "translate_image_ja")
    );
    let started_at = Instant::now();
    let started = call(
        service,
        Command::RunAction {
            id: "translate_image_ja".into(),
        },
    );
    let job_id = JobId(started["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(300);
    let result = loop {
        let result = call(service, Command::PollJob { job_id });
        match result["state"].as_str() {
            Some("succeeded") => break result["result"].clone(),
            Some("queued" | "running" | "ready") => assert!(Instant::now() < deadline),
            _ => panic!("{result}"),
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(result["source_language"], "ja");
    assert_eq!(result["target_language"], "en");
    assert_eq!(result["event"]["kind"], "managed_image");
    assert!(result["rendered_blocks"].as_u64().unwrap() > 0);
    let output = result["event"]["path"].as_str().unwrap();
    let image = image::ImageReader::open(output)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    assert_eq!(image.format(), Some(image::ImageFormat::Png));
    assert_eq!(
        image.into_dimensions().unwrap(),
        image::image_dimensions(&source).unwrap()
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    eprintln!(
        "Translate in {:?}: {} rendered, {} skipped",
        started_at.elapsed(),
        result["rendered_blocks"],
        result["skipped_blocks"]
    );
    result
}

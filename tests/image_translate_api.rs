use katran::{
    Command, ImageTranslationSource, Request, Service,
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
#[ignore = "requires local OCR/Ollama components and a synthetic Traditional Chinese PNG"]
fn traditional_chinese_bull_keeps_untranslated_pages_in_place() {
    use std::io::Cursor;

    let fixture = std::env::var_os("KATRAN_TRANSLATE_TEST_TRADITIONAL_PNG").unwrap();
    let text_page = std::fs::read(fixture).unwrap();
    let blank_png = |width| {
        let mut output = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(width, 360, image::Rgb([255, 255, 255]))
            .write_to(&mut output, image::ImageFormat::Png)
            .unwrap();
        output.into_inner()
    };
    let blank1 = blank_png(352);
    let blank2 = blank_png(376);
    let profile = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("four-pages.bull");
    let mut original = Vec::new();
    baloh_collection::codec::write(
        &mut original,
        &[
            text_page.clone(),
            blank1.clone(),
            text_page.clone(),
            blank2.clone(),
        ],
    )
    .unwrap();
    std::fs::write(&source, &original).unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let mut settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    settings.translation_numbered_only = true;
    call(&mut service, Command::SetSettings { settings });
    let status = call(&mut service, Command::AiComponentsStatus);
    assert_eq!(
        status["image_translate"]["sources"]["zh-hant"]["ready"], true,
        "{status}"
    );
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().to_owned(),
        },
    );
    let id = browse(&mut service)["events"][0]["id"].as_i64().unwrap();
    call(&mut service, Command::SetVisible { ids: vec![id] });
    call(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let queued = call(
        &mut service,
        Command::RunAction {
            id: "translate_image_zh_hant".into(),
        },
    );
    let result = wait_for_job(&mut service, queued);
    assert_eq!(result["source_language"], "zh-hant");
    assert_eq!(result["total"], 4);
    assert_eq!(result["translated"], 2, "{result}");
    let output = std::fs::File::open(result["event"]["path"].as_str().unwrap()).unwrap();
    let pages = baloh_collection::codec::read(output).unwrap();
    assert_eq!(pages.len(), 4);
    for (i, blank) in [(1, blank1), (3, blank2)] {
        assert_eq!(result["pages"][i]["index"], i);
        assert_eq!(result["pages"][i]["page"], i);
        assert_eq!(result["pages"][i]["preserved_original"], true);
        assert_eq!(result["pages"][i]["error"]["code"], "no_text_recognized");
        assert_eq!(pages[i], blank);
    }
    for i in [0, 2] {
        assert_eq!(result["pages"][i]["page"], i);
        assert_eq!(result["pages"][i]["ok"], true);
        assert_ne!(pages[i], text_page);
    }
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[test]
fn target_follows_interface_and_explicit_choice_persists() {
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let mut settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    assert_eq!(settings.translation_target, TranslationTarget::Interface);
    assert!(!settings.translation_numbered_only);
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
    settings.translation_numbered_only = true;
    call(&mut service, Command::SetSettings { settings });
    drop(service);
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    assert!(settings.translation_numbered_only);
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
    let first = translate_page(&mut service, &input, ImageTranslationSource::Japanese);
    assert_eq!(first["skipped_blocks"], 0);
    assert_eq!(first["fallback_blocks"], 0);
    struct Preview(std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>);
    impl katran::quicklook::Launcher for Preview {
        fn launch(&self, path: &std::path::Path) -> katran::Result<()> {
            self.0.lock().unwrap().push(path.to_owned());
            Ok(())
        }
    }
    let launched = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    service.set_preview_launcher(Box::new(Preview(launched.clone())));
    let preview = call(
        &mut service,
        Command::PreviewPublishedTranslation {
            id: first["event"]["id"].as_i64().unwrap(),
            revision: first["event"]["revision"].as_str().unwrap().into(),
        },
    );
    assert!(wait_for_job(&mut service, preview)["requested"] == true);
    let started = launched.lock().unwrap();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].extension().unwrap(), "png");
    assert!(
        std::fs::read(&started[0])
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    drop(started);
    let session_dirs = || {
        std::fs::read_dir(&staging)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>()
    };
    let session = session_dirs();
    assert_eq!(session.len(), 1);

    let warm = translate_page(&mut service, &input, ImageTranslationSource::Japanese);
    assert_eq!(warm["skipped_blocks"], 0);
    assert_eq!(session_dirs(), session);
    let mut results = vec![first, warm];
    let complex = std::env::var_os("KATRAN_TRANSLATE_TEST_COMPLEX_IMAGE");
    if let Some(complex) = complex.as_ref() {
        let result = translate_page(&mut service, &complex, ImageTranslationSource::Japanese);
        assert!(result["side_blocks"].as_u64().unwrap() > 0);
        assert_eq!(result["skipped_blocks"], 0);
        results.push(result);
    }
    let mut numbered_settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    numbered_settings.translation_numbered_only = true;
    call(
        &mut service,
        Command::SetSettings {
            settings: numbered_settings,
        },
    );
    let numbered = translate_page(&mut service, &input, ImageTranslationSource::Japanese);
    assert_eq!(numbered["fallback_blocks"], numbered["rendered_blocks"]);
    assert_eq!(numbered["skipped_blocks"], 0);
    results.push(numbered);
    let collection = complex.as_ref().map(|complex| {
        let result = reverse_batch(&mut service, &input, complex);
        results.push(result.clone());
        result
    });
    for (language, fixture) in [
        (
            ImageTranslationSource::Chinese,
            "KATRAN_TRANSLATE_TEST_CHINESE_IMAGE",
        ),
        (
            ImageTranslationSource::Korean,
            "KATRAN_TRANSLATE_TEST_KOREAN_IMAGE",
        ),
    ] {
        if let Some(fixture) = std::env::var_os(fixture) {
            assert_eq!(
                metadata["image_translate"]["sources"][language.code()]["ready"],
                true
            );
            let result = translate_page(&mut service, &fixture, language);
            assert_eq!(result["rendered_blocks"], 2);
            assert_eq!(result["skipped_blocks"], 0);
            assert_eq!(session_dirs(), session);
            results.push(result);
        }
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
    if let Some(batch) = collection {
        let mut reopened = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
        let event = &batch["event"];
        let read = |api: &mut Service, index| {
            let request = call(
                api,
                Command::TranslationCollectionPage {
                    id: event["id"].as_i64().unwrap(),
                    revision: event["revision"].as_str().unwrap().into(),
                    index,
                },
            );
            let page = wait_for_job(api, request);
            assert_eq!(page["count"], 2);
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(page["png"].as_str().unwrap())
                .unwrap();
            image::load_from_memory(&bytes).unwrap().width()
        };
        let simple_width = image::image_dimensions(std::path::Path::new(&input))
            .unwrap()
            .0;
        let second_width = read(&mut reopened, 1);
        assert!(read(&mut reopened, 0) > second_width);
        assert!(
            second_width > simple_width,
            "the batch must use the numbered mode too"
        );
    }
}

fn reverse_batch(
    service: &mut Service,
    simple: &std::ffi::OsStr,
    complex: &std::ffi::OsStr,
) -> Value {
    let folder = tempfile::tempdir().unwrap();
    std::fs::copy(simple, folder.path().join("first.png")).unwrap();
    std::fs::write(folder.path().join("broken.png"), b"invalid PNG").unwrap();
    std::fs::copy(complex, folder.path().join("last.png")).unwrap();
    call(
        service,
        Command::ChooseFolder {
            path: folder.path().to_owned(),
        },
    );
    let page = browse(service);
    let ids: Vec<i64> = ["first.png", "broken.png", "last.png"]
        .into_iter()
        .map(|name| {
            page["events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|event| event["name"] == name)
                .unwrap()["id"]
                .as_i64()
                .unwrap()
        })
        .collect();
    call(service, Command::SetVisible { ids: ids.clone() });
    call(
        service,
        Command::Select {
            id: ids[2],
            mode: SelectionMode::Replace,
        },
    );
    let selected = call(
        service,
        Command::Select {
            id: ids[0],
            mode: SelectionMode::Range,
        },
    );
    assert_eq!(
        selected["selected"],
        serde_json::json!([ids[2], ids[1], ids[0]])
    );
    let started = call(
        service,
        Command::RunAction {
            id: "translate_image_ja".into(),
        },
    );
    let result = wait_for_job(service, started);
    assert_eq!(result["total"], 3);
    assert_eq!(result["translated"], 2);
    assert_eq!(result["pages"][0]["page"], 0);
    assert_eq!(result["pages"][1]["ok"], false);
    assert_eq!(result["pages"][2]["page"], 1);
    assert_eq!(result["event"]["source"], "image_translation_collection");
    result
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

fn translate_page(
    service: &mut Service,
    input: &std::ffi::OsStr,
    language: ImageTranslationSource,
) -> Value {
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
    let action = format!("translate_image_{}", language.code());
    let actions = call(service, Command::Actions);
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == action)
    );
    let started_at = Instant::now();
    let started = call(service, Command::RunAction { id: action });
    let result = wait_for_job(service, started);
    assert_eq!(result["source_language"], language.code());
    assert_eq!(result["target_language"], "en");
    assert_eq!(result["event"]["kind"], "managed_image");
    assert!(result["rendered_blocks"].as_u64().unwrap() > 0);
    let output = result["event"]["path"].as_str().unwrap();
    let image = image::ImageReader::open(output)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    assert_eq!(image.format(), Some(image::ImageFormat::Png));
    let (width, height) = image.into_dimensions().unwrap();
    let (source_width, source_height) = image::image_dimensions(&source).unwrap();
    assert!(width >= source_width);
    assert_eq!(result["width"], width);
    assert_eq!(result["height"], height);
    let fallback = result["fallback_blocks"].as_u64().unwrap();
    let side = result["side_blocks"].as_u64().unwrap();
    let left = result["source_left"].as_u64().unwrap();
    assert!(left <= u64::from(width - source_width));
    assert_eq!(width > source_width, side > 0);
    assert!(height >= source_height);
    assert_eq!(height > source_height, fallback > side);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    eprintln!(
        "{} in {:?}: {} rendered, {} outside the original canvas ({} in margins), {} skipped",
        language.code(),
        started_at.elapsed(),
        result["rendered_blocks"],
        result["fallback_blocks"],
        result["side_blocks"],
        result["skipped_blocks"]
    );
    result
}

fn wait_for_job(service: &mut Service, started: Value) -> Value {
    let job_id = JobId(started["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let result = call(service, Command::PollJob { job_id });
        match result["state"].as_str() {
            Some("succeeded") => return result["result"].clone(),
            Some("queued" | "running" | "ready") => assert!(Instant::now() < deadline),
            _ => panic!("{result}"),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

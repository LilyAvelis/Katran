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
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("original.png");
    std::fs::copy(input, &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let mut settings: Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    settings.language = Language::English;
    call(&mut service, Command::SetSettings { settings });
    let metadata = call(&mut service, Command::AiComponentsStatus);
    assert_eq!(metadata["image_translate"]["ready"], true, "{metadata}");
    call(
        &mut service,
        Command::ChooseFolder {
            path: source_dir.path().to_owned(),
        },
    );
    let browse = |service: &mut Service| {
        call(
            service,
            Command::BrowseShelfSorted {
                query: String::new(),
                limit: 10,
                cursor: None,
            },
        )
    };
    let id = browse(&mut service)["events"][0]["id"].as_i64().unwrap();
    call(&mut service, Command::SetVisible { ids: vec![id] });
    call(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let actions = call(&mut service, Command::Actions);
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == "translate_image_ja")
    );
    let started = call(
        &mut service,
        Command::RunAction {
            id: "translate_image_ja".into(),
        },
    );
    let job_id = JobId(started["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(300);
    let result = loop {
        let result = call(&mut service, Command::PollJob { job_id });
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
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Translate,
        },
    );
    let page = browse(&mut service);
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["id"], result["event"]["id"]);
}

use katran::{
    Command, Request, Service, UpscaleOperation, domain_jobs::JobId, interaction::SelectionMode,
};
use serde_json::Value;
use std::time::{Duration, Instant};

fn call(service: &mut Service, command: Command) -> Value {
    let response = service.execute(Request {
        api_version: 1,
        request_id: "upscale".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

#[test]
#[ignore = "requires an existing ComfyUI Python runtime and all three local weights"]
fn local_weights_produce_exact_independent_results() {
    let profile = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("кадр.png");
    image::RgbaImage::from_fn(31, 23, |x, y| {
        image::Rgba([(x * 7) as u8, (y * 11) as u8, ((x + y) * 4) as u8, 240])
    })
    .save(&source)
    .unwrap();
    let original = std::fs::read(&source).unwrap();
    let mut service = Service::open(profile.path(), 100_000_000).unwrap();
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().to_owned(),
        },
    );
    let page = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 10,
            cursor: None,
        },
    );
    let id = page["events"][0]["id"].as_i64().unwrap();
    call(&mut service, Command::SetVisible { ids: vec![id] });
    call(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let actions = call(&mut service, Command::Actions);
    for (action_id, operation, factor) in [
        ("upscale_remacri_x2", UpscaleOperation::RemacriX2, 2),
        ("upscale_remacri_x4", UpscaleOperation::RemacriX4, 4),
        ("upscale_realesrgan_x2", UpscaleOperation::RealEsrganX2, 2),
        ("upscale_realesrgan_x4", UpscaleOperation::RealEsrganX4, 4),
    ] {
        assert!(
            actions
                .as_array()
                .unwrap()
                .iter()
                .any(|action| action["id"] == action_id)
        );
        let started = call(
            &mut service,
            Command::BeginJob {
                command: Box::new(Command::RunAction {
                    id: action_id.into(),
                }),
            },
        );
        let job_id = JobId(started["job_id"].as_u64().unwrap());
        let deadline = Instant::now() + Duration::from_secs(90);
        let result = loop {
            let status = call(&mut service, Command::PollJob { job_id });
            match status["state"].as_str() {
                Some("succeeded") => break status["result"].clone(),
                Some("queued" | "running" | "ready") => assert!(Instant::now() < deadline),
                _ => panic!("{status}"),
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        assert_eq!(result["input_width"], 31);
        assert_eq!(
            result["operation"],
            serde_json::to_value(operation).unwrap()
        );
        assert_eq!(result["input_height"], 23);
        assert_eq!(result["width"], 31 * factor);
        assert_eq!(result["height"], 23 * factor);
        assert_eq!(result["event"]["kind"], "managed_image");
        let path = result["event"]["path"].as_str().unwrap();
        assert_eq!(
            image::ImageReader::open(path)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .into_dimensions()
                .unwrap(),
            (31 * factor, 23 * factor)
        );
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }

    let queued = call(
        &mut service,
        Command::UpscaleSelectedImage {
            operation: UpscaleOperation::RemacriX4,
        },
    );
    let job_id = JobId(queued["job_id"].as_u64().unwrap());
    call(&mut service, Command::CancelJob { job_id });
    let cancelled = call(&mut service, Command::PollJob { job_id });
    assert_eq!(cancelled["state"], "cancelled");
    assert!(cancelled["result"].is_null());
    assert_eq!(std::fs::read(source).unwrap(), original);
}

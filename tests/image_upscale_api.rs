use katran::{
    Command, Request, Service, UpscaleOperation, domain_jobs::JobId, interaction::SelectionMode,
};
use serde_json::Value;
use std::io::Cursor;
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

fn synthetic_png(width: u32, height: u32) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(width, height)
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

fn select(service: &mut Service, ids: &[i64]) {
    call(service, Command::SetVisible { ids: ids.to_vec() });
    for (index, &id) in ids.iter().enumerate() {
        call(
            service,
            Command::Select {
                id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
}

fn run(service: &mut Service, action: &str) -> Value {
    call(service, Command::RunAction { id: action.into() })
}

#[test]
fn image_collection_actions_are_available_for_folder_images_only() {
    use baloh_collection::bs::{self, FileEntry};
    let profile = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    for (family, name, bytes) in [
        ("images", "page.png", synthetic_png(15, 11)),
        ("video", "clip.mp4", vec![0]),
    ] {
        let mut archive = b"BALOHCOL\x02\0\0\0FILE".to_vec();
        archive.push(family.len() as u8);
        archive.extend_from_slice(family.as_bytes());
        bs::write(
            &mut archive,
            &[FileEntry {
                name: name.into(),
                bytes,
            }],
        )
        .unwrap();
        std::fs::write(folder.path().join(format!("{family}.bull")), archive).unwrap();
    }
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().to_owned(),
        },
    );
    let rows = call(
        &mut service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 10,
            cursor: None,
        },
    );
    for row in rows["events"].as_array().unwrap() {
        select(&mut service, &[row["id"].as_i64().unwrap()]);
        let expected = row["name"] == "images.bull";
        let actions = call(&mut service, Command::Actions);
        for command in ["upscale_remacri_x2", "translate_image_ja"] {
            assert_eq!(
                actions
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|action| action["id"] == command),
                expected
            );
        }
        if expected {
            let job_id = JobId(
                run(&mut service, "translate_image_ja")["job_id"]
                    .as_u64()
                    .unwrap(),
            );
            call(&mut service, Command::CancelJob { job_id });
            assert_eq!(
                call(&mut service, Command::PollJob { job_id })["state"],
                "cancelled"
            );
        }
    }
}

#[test]
#[ignore = "requires an existing ComfyUI Python runtime and Real-ESRGAN ×2 weights"]
fn local_weights_upscale_an_entire_bull_into_a_new_ordered_bull() {
    use katran::interaction::ShelfTab;
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let images: Vec<_> = [(17, 19), (23, 21)]
        .into_iter()
        .map(|(width, height)| {
            call(
                &mut service,
                Command::IngestBytes {
                    name: format!("{width}.png"),
                    bytes: synthetic_png(width, height),
                    source: "synthetic".into(),
                },
            )["id"]
                .as_i64()
                .unwrap()
        })
        .collect();
    select(&mut service, &[images[1], images[0]]);
    let packed = run(&mut service, "create_image_collection");
    let packed = wait(&mut service, packed);
    let source = &packed["event"];
    let source_id = source["id"].as_i64().unwrap();
    let source_path = source["path"].as_str().unwrap();
    let original = std::fs::read(source_path).unwrap();
    select(&mut service, &[source_id]);
    let queued = run(&mut service, "upscale_realesrgan_x2");
    let result = wait(&mut service, queued);
    assert_eq!(result["collection"], true);
    assert_eq!(result["total"], 2);
    assert_eq!(result["upscaled"], 2, "{result}");
    assert_ne!(result["event"]["id"], source["id"]);
    let result_file = std::fs::File::open(result["event"]["path"].as_str().unwrap()).unwrap();
    let pages = baloh_collection::codec::read(result_file).unwrap();
    let dimensions: Vec<_> = pages
        .iter()
        .map(|bytes| {
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png)
                .into_dimensions()
                .unwrap()
        })
        .collect();
    assert_eq!(dimensions, [(46, 42), (34, 38)]);
    assert_eq!(std::fs::read(source_path).unwrap(), original);
    let shelf = call(
        &mut service,
        Command::BrowseTabSnapshot {
            tab: ShelfTab::Images,
            query: String::new(),
            limit: 10,
        },
    );
    let cards = shelf["events"].as_array().unwrap();
    assert_eq!(cards.len(), 2);
    for id in [source["id"].clone(), result["event"]["id"].clone()] {
        assert!(cards.iter().any(|card| card["id"] == id));
    }
}

fn wait(service: &mut Service, queued: Value) -> Value {
    let job_id = JobId(queued["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let status = call(service, Command::PollJob { job_id });
        match status["state"].as_str() {
            Some("succeeded") => return status["result"].clone(),
            Some("queued" | "running" | "ready") => assert!(Instant::now() < deadline),
            _ => panic!("{status}"),
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

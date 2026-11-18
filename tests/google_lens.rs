use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;

fn execute(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "lens-fixture".into(),
        command,
    })
}

fn call(api: &mut Service, command: Command) -> Value {
    let response = execute(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

#[test]
fn action_requires_one_image() {
    let profile = tempfile::tempdir().unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let response = execute(&mut api, Command::SearchSelectedImageWithGoogleLens);
    assert_eq!(response["error"]["code"], "invalid_selection");
}

#[test]
#[ignore = "explicitly sends a generated image to Google Lens"]
fn live_synthetic_search_hands_result_to_default_browser_boundary() {
    use std::{
        io::Cursor,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    struct Browser(Arc<Mutex<Vec<PathBuf>>>);
    impl katran::open_file::Launcher for Browser {
        fn launch(
            &self,
            path: &Path,
            operation: katran::open_file::Operation,
        ) -> katran::Result<()> {
            assert_eq!(operation, katran::open_file::Operation::Open);
            self.0.lock().unwrap().push(path.to_owned());
            Ok(())
        }
    }
    let profile = tempfile::tempdir().unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let opens = Arc::new(Mutex::new(Vec::new()));
    api.set_file_launcher(Box::new(Browser(opens.clone())));
    let image = image::RgbImage::from_fn(640, 440, |x, y| {
        if (x as i32 - 320).pow(2) + (y as i32 - 220).pow(2) < 125_i32.pow(2) {
            image::Rgb([20_u8, 80, 180])
        } else {
            image::Rgb([250_u8, 243, 187])
        }
    });
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "generated-fixture.png".into(),
            bytes: png.into_inner(),
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let actions = call(&mut api, Command::Actions);
    actions
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["id"] == "search_image_google_lens")
        .expect("Lens action is available for a selected image");
    assert!(
        katran::actions::DEFS
            .iter()
            .any(|action| { action.id == "search_image_google_lens" && action.rebindable })
    );
    let started = call(
        &mut api,
        Command::RunAction {
            id: "search_image_google_lens".into(),
        },
    );
    let job_id = JobId(started["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(35);
    loop {
        let status = call(&mut api, Command::PollJob { job_id });
        match status["state"].as_str() {
            Some("succeeded") => {
                assert_eq!(status["result"]["google_lens_requested"], true);
                break;
            }
            Some("queued" | "running" | "ready") => assert!(Instant::now() < deadline),
            _ => panic!("{status}"),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let opens = opens.lock().unwrap();
    assert_eq!(opens.len(), 1);
    let url = url::Url::parse(opens[0].to_str().unwrap()).unwrap();
    assert!(matches!(
        url.host_str(),
        Some("www.google.com" | "lens.google.com")
    ));
    assert_eq!(url.path(), "/search");
}

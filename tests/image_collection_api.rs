use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::{Value, json};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn call(service: &mut Service, command: Command) -> Value {
    let reply = service.execute(Request {
        api_version: 1,
        request_id: "image-collection".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

fn finish(service: &mut Service, job: Value) -> Value {
    let job_id = JobId(job["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = call(service, Command::PollJob { job_id });
        match status["state"].as_str() {
            Some("succeeded") => return status["result"].clone(),
            Some("queued" | "running" | "ready") => assert!(Instant::now() < deadline),
            _ => panic!("{status}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn image(service: &mut Service, width: u32, name: &str) -> Value {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(width, 18)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    call(
        service,
        Command::IngestBytes {
            name: name.into(),
            bytes: bytes.into_inner(),
            source: "synthetic".into(),
        },
    )
}

fn id(event: &Value) -> i64 {
    event["id"].as_i64().unwrap()
}

fn snapshot(service: &mut Service, tab: ShelfTab) -> Value {
    call(
        service,
        Command::BrowseTabSnapshot {
            tab,
            query: String::new(),
            limit: 20,
        },
    )
}

fn action(service: &mut Service) -> Value {
    let started = call(
        service,
        Command::RunAction {
            id: "create_image_collection".into(),
        },
    );
    finish(service, started)
}

fn palette_label(service: &mut Service) -> Value {
    call(service, Command::Actions)
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "create_image_collection")
        .unwrap()["label"]
        .clone()
}

struct Preview(Arc<Mutex<Vec<PathBuf>>>);
impl katran::quicklook::Launcher for Preview {
    fn launch(&self, path: &Path) -> katran::Result<()> {
        self.0.lock().unwrap().push(path.to_owned());
        Ok(())
    }
}

#[test]
fn one_contextual_action_round_trips_ordered_pages_and_shelf_metadata() {
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    settings.language = katran::settings::Language::English;
    call(&mut service, Command::SetSettings { settings });
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let first = image(&mut service, 12, "first.png");
    let second = image(&mut service, 19, "second.png");
    call(
        &mut service,
        Command::SetVisible {
            ids: vec![id(&first), id(&second)],
        },
    );
    call(
        &mut service,
        Command::Select {
            id: id(&second),
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut service,
        Command::Select {
            id: id(&first),
            mode: SelectionMode::Range,
        },
    );
    assert_eq!(palette_label(&mut service), "Pack images into BULL");
    let packed = action(&mut service);
    let bull = &packed["event"];
    assert_eq!(packed["count"], 2);
    assert_eq!(bull["source"], "image_collection");
    let page = snapshot(&mut service, ShelfTab::Images);
    let cards = page["events"].as_array().unwrap();
    assert_eq!(cards.len(), 3);
    let card = cards.iter().find(|item| id(item) == id(bull)).unwrap();
    assert_eq!(card["properties"]["pinned"], false);
    assert_eq!(
        snapshot(&mut service, ShelfTab::Translate)["events"],
        json!([])
    );
    call(
        &mut service,
        Command::TogglePins {
            ids: vec![id(bull)],
        },
    );
    call(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![id(bull)],
            slot: Some(2),
        },
    );
    assert_eq!(
        snapshot(&mut service, ShelfTab::Images)["assignments"],
        json!([{"id": id(bull), "slot": 2}])
    );
    drop(service);

    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(
        &mut service,
        Command::SetVisible {
            ids: vec![id(bull)],
        },
    );
    call(
        &mut service,
        Command::Select {
            id: id(bull),
            mode: SelectionMode::Replace,
        },
    );
    assert_eq!(palette_label(&mut service), "Unpack BULL");
    let page = call(
        &mut service,
        Command::TranslationCollectionPage {
            id: id(bull),
            revision: bull["revision"].as_str().unwrap().into(),
            index: 0,
        },
    );
    assert_eq!(finish(&mut service, page)["count"], 2);

    let launched = Arc::new(Mutex::new(Vec::new()));
    service.set_preview_launcher(Box::new(Preview(launched.clone())));
    let preview = call(&mut service, Command::PreviewExternal { id: id(bull) });
    assert_eq!(finish(&mut service, preview)["backend"], "quicklook");
    let named = launched.lock().unwrap();
    assert_eq!(named[0].extension().unwrap(), "bull");
    let pages = baloh_collection::codec::read(std::fs::File::open(&named[0]).unwrap()).unwrap();
    let widths: Vec<_> = pages
        .iter()
        .map(|bytes| image::load_from_memory(bytes).unwrap().width())
        .collect();
    assert_eq!(widths, [19, 12]);
    drop(named);

    let unpacked = action(&mut service);
    assert_eq!(unpacked["unpacked"], true);
    let extracted = unpacked["items"].as_array().unwrap();
    let available = snapshot(&mut service, ShelfTab::Images);
    let events = available["events"].as_array().unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(
        events
            .iter()
            .filter(|item| item["source"] == "image_collection_page")
            .map(|item| &item["id"])
            .collect::<Vec<_>>(),
        extracted.iter().collect::<Vec<_>>()
    );
    assert_eq!(action(&mut service)["items"], unpacked["items"]);
    assert_eq!(
        snapshot(&mut service, ShelfTab::Translate)["events"],
        json!([])
    );
}

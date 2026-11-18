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

fn select(service: &mut Service, ids: &[i64]) {
    call(service, Command::SetVisible { ids: ids.to_vec() });
    for (index, &id) in ids.iter().enumerate() {
        let mode = if index == 0 {
            SelectionMode::Replace
        } else {
            SelectionMode::Toggle
        };
        call(service, Command::Select { id, mode });
    }
}

fn pin_and_tag(service: &mut Service, ids: &[i64], slot: u8) {
    call(service, Command::TogglePins { ids: ids.to_vec() });
    call(
        service,
        Command::AssignTagSlot {
            ids: ids.to_vec(),
            slot: Some(slot),
        },
    );
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
    assert_eq!(cards.len(), 1);
    assert_eq!(id(&cards[0]), id(bull));
    assert_eq!(cards[0]["properties"]["pinned"], false);
    assert_eq!(
        call(&mut service, Command::List { limit: 20 })["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
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
    let cover = call(
        &mut service,
        Command::PreviewImage {
            id: id(bull),
            max_edge: 64,
        },
    );
    assert!(cover["width"].as_u64().unwrap() > cover["height"].as_u64().unwrap());

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
    assert_eq!(unpacked["items"], json!([id(&second), id(&first)]));
    let available = snapshot(&mut service, ShelfTab::Images);
    let events = available["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events.iter().map(id).collect::<Vec<_>>(),
        vec![id(&second), id(&first)]
    );
    assert_eq!(
        snapshot(&mut service, ShelfTab::Translate)["events"],
        json!([])
    );
}

#[test]
fn merge_hides_old_cards_inherits_only_matching_tags_and_restores_originals() {
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let penguins: Vec<_> = (1..=4)
        .map(|number| {
            id(&image(
                &mut service,
                20 + number,
                &format!("penguin-{number}.png"),
            ))
        })
        .collect();
    pin_and_tag(&mut service, &penguins[..2], 1);
    pin_and_tag(&mut service, &penguins[2..], 2);
    select(&mut service, &[penguins[1], penguins[0]]);
    let first = id(&action(&mut service)["event"]);
    let shelf = snapshot(&mut service, ShelfTab::Images);
    assert_eq!(id(&shelf["events"][0]), first);
    assert_eq!(shelf["events"][0]["properties"]["pinned"], true);
    assert!(
        shelf["assignments"]
            .as_array()
            .unwrap()
            .contains(&json!({"id": first, "slot": 1}))
    );
    select(&mut service, &penguins[2..]);
    let second = id(&action(&mut service)["event"]);
    select(&mut service, &[second, first]);
    let merged = action(&mut service);
    assert_eq!(merged["merged"], true);
    assert_eq!(merged["count"], 4);
    let bull = &merged["event"];
    let shelf = snapshot(&mut service, ShelfTab::Images);
    assert_eq!(shelf["events"].as_array().unwrap().len(), 1);
    assert_eq!(id(&shelf["events"][0]), id(bull));
    assert_eq!(shelf["events"][0]["properties"]["pinned"], true);
    assert_eq!(shelf["assignments"], json!([]));
    let pages =
        baloh_collection::codec::read(std::fs::File::open(bull["path"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(
        pages
            .iter()
            .map(|page| image::load_from_memory(page).unwrap().width())
            .collect::<Vec<_>>(),
        [23, 24, 22, 21]
    );
    select(&mut service, &[id(bull)]);
    assert_eq!(
        action(&mut service)["items"],
        json!([penguins[2], penguins[3], penguins[1], penguins[0]])
    );
    let shelf = snapshot(&mut service, ShelfTab::Images);
    assert_eq!(shelf["events"].as_array().unwrap().len(), 4);
    for (index, &penguin) in penguins.iter().enumerate() {
        assert!(
            shelf["assignments"]
                .as_array()
                .unwrap()
                .contains(&json!({"id":penguin,"slot":if index < 2 { 1 } else { 2 }}))
        );
    }
}

#[test]
fn merging_a_legacy_bull_restores_its_pngs_and_the_local_original() {
    let profile = tempfile::tempdir().unwrap();
    let mut service = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(51, 22)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut bytes = Vec::new();
    baloh_collection::codec::write(&mut bytes, &[png.into_inner()]).unwrap();
    let legacy = id(&call(
        &mut service,
        Command::IngestBytes {
            name: "Legacy.bull".into(),
            bytes,
            source: "image_collection".into(),
        },
    ));
    let original = id(&image(&mut service, 30, "original.png"));
    select(&mut service, &[original]);
    let local = id(&action(&mut service)["event"]);
    select(&mut service, &[legacy, local]);
    let merged = id(&action(&mut service)["event"]);
    select(&mut service, &[merged]);
    let items = action(&mut service)["items"].as_array().unwrap().clone();
    assert_eq!(items[1], original);
    let item = call(
        &mut service,
        Command::Get {
            id: items[0].as_i64().unwrap(),
        },
    );
    assert_eq!(item["source"], "image_collection_page");
    let bytes = std::fs::read(item["path"].as_str().unwrap()).unwrap();
    let restored = image::load_from_memory(&bytes).unwrap();
    assert_eq!((restored.width(), restored.height()), (51, 22));
    assert_eq!(
        snapshot(&mut service, ShelfTab::Images)["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

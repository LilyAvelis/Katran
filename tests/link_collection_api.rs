use baloh_collection::links::{self, Link};
use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::{Value, json};
use std::{
    io::Cursor,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn call(api: &mut Service, command: Command) -> Value {
    let result = api.execute(Request {
        api_version: 1,
        request_id: "synthetic-link-bull".into(),
        command,
    });
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
}

fn finish(api: &mut Service, command: Command) -> Value {
    let queued = call(api, command);
    let job_id = JobId(queued["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let result = call(api, Command::PollJob { job_id });
        match result["state"].as_str() {
            Some("succeeded") => return result["result"].clone(),
            Some("queued" | "running" | "ready") if Instant::now() < deadline => {}
            _ => panic!("{result}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn id(card: &Value) -> i64 {
    card["id"].as_i64().unwrap()
}

fn select(api: &mut Service, ids: &[i64]) {
    call(api, Command::SetVisible { ids: ids.to_vec() });
    for (index, &id) in ids.iter().enumerate() {
        let mode = if index == 0 {
            SelectionMode::Replace
        } else {
            SelectionMode::Toggle
        };
        call(api, Command::Select { id, mode });
    }
}

fn shelf(api: &mut Service, tab: ShelfTab) -> Value {
    call(
        api,
        Command::BrowseTabSnapshot {
            tab,
            query: String::new(),
            limit: 20,
        },
    )
}

fn bull_action(api: &mut Service) -> Value {
    finish(
        api,
        Command::RunAction {
            id: "create_image_collection".into(),
        },
    )
}

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<String>>>);
impl katran::text_fragments::ClipboardSink for Sink {
    fn write(&self, text: &str) -> katran::Result<u32> {
        self.0.lock().unwrap().push(text.into());
        Ok(12)
    }
}
impl katran::open_file::Launcher for Sink {
    fn launch(&self, path: &Path, operation: katran::open_file::Operation) -> katran::Result<()> {
        assert_eq!(operation, katran::open_file::Operation::Open);
        self.0
            .lock()
            .unwrap()
            .push(path.to_string_lossy().into_owned());
        Ok(())
    }
}

#[test]
fn links_pack_merge_browse_copy_one_open_one_and_unpack() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Links,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Links,
        },
    );
    let urls = [
        "http://example.test/first?q=1#anchor",
        "http://example.test/second?q=2",
    ];
    let ids: Vec<_> = urls
        .iter()
        .map(|url| {
            id(&call(
                &mut api,
                Command::IngestLink {
                    url: (*url).into(),
                    source: "synthetic".into(),
                },
            ))
        })
        .collect();
    let custom_name = "Мои заметки 🐳";
    call(&mut api, Command::TogglePins { ids: vec![ids[0]] });
    call(
        &mut api,
        Command::SetPinnedProperties {
            id: ids[0],
            display_name: Some(custom_name.into()),
            opener: None,
        },
    );
    select(&mut api, &[ids[1], ids[0]]);
    assert!(
        call(&mut api, Command::Actions)
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["id"] == "create_image_collection")
    );
    let actual = bull_action(&mut api);
    let packed = &actual["event"];
    assert_eq!(actual["link_collection"], true);
    assert_eq!(actual["count"], 2);
    let packed_page = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: id(packed),
            revision: packed["revision"].as_str().unwrap().to_owned(),
            index: 1,
        },
    );
    assert_eq!(packed_page["link"]["title"], custom_name);
    assert_eq!(
        shelf(&mut api, ShelfTab::Links)["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        shelf(&mut api, ShelfTab::Images)["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let mut thumbnail = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(20, 10)
        .write_to(&mut thumbnail, image::ImageFormat::Png)
        .unwrap();
    let rich = Link {
        url: "https://example.test/rich".into(),
        title: "Synthetic title".into(),
        description: "A saved description".into(),
        preview_png: Some(thumbnail.into_inner()),
    };
    let mut portable = Vec::new();
    links::write(&mut portable, std::slice::from_ref(&rich)).unwrap();
    let fixture = call(
        &mut api,
        Command::IngestBytes {
            name: "seed.bull".into(),
            source: "link_collection".into(),
            bytes: portable,
        },
    );
    select(&mut api, &[id(&fixture), id(packed)]);
    let merged = bull_action(&mut api);
    assert_eq!(merged["merged"], true);
    let card = &merged["event"];
    let card_id = id(card);
    let of = || card["revision"].as_str().unwrap().to_owned();
    let entries =
        links::read(std::fs::File::open(card["path"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(entries[0], rich);
    assert_eq!(entries[1].url, urls[1]);
    assert!(entries[1].title.is_empty());
    assert_eq!(entries[2].url, urls[0]);
    assert_eq!(entries[2].title, custom_name);
    let event: katran::Event = serde_json::from_value(card.clone()).unwrap();
    let whole = katran::copy_out::prepare_with_budget(
        &root.path().join("exports"),
        &[event],
        katran::DEFAULT_QUOTA,
    )
    .unwrap();
    assert_eq!(whole.paths.len(), 1);
    assert_eq!(whole.paths[0].extension().unwrap(), "bull");
    assert_eq!(
        links::read(std::fs::File::open(&whole.paths[0]).unwrap()).unwrap(),
        entries
    );
    assert_eq!(
        shelf(&mut api, ShelfTab::Links)["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let first = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: card_id,
            revision: of(),
            index: 0,
        },
    );
    assert_eq!(first["count"], 3);
    assert_eq!(first["link"]["title"], rich.title);
    assert_eq!(first["link"]["description"], rich.description);
    assert!(!first["link"]["preview_png"].as_str().unwrap().is_empty());

    let clipboard = Sink::default();
    let browser = Sink::default();
    api.set_text_clipboard_sink(Box::new(clipboard.clone()));
    api.set_file_launcher(Box::new(browser.clone()));
    let copy = finish(
        &mut api,
        Command::CopyCollectionPage {
            id: card_id,
            revision: of(),
            index: 2,
        },
    );
    let open = finish(
        &mut api,
        Command::OpenCollectionLink {
            id: card_id,
            revision: of(),
            index: 0,
        },
    );
    assert_eq!(copy["copied_collection_page"], true);
    assert_eq!(open["opened_collection_link"], true);
    assert_eq!(clipboard.0.lock().unwrap().as_slice(), &[urls[0]]);
    assert_eq!(browser.0.lock().unwrap().as_slice(), &[rich.url.as_str()]);
    assert_eq!(
        shelf(&mut api, ShelfTab::Links)["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    select(&mut api, &[card_id]);
    let unpacked = bull_action(&mut api);
    assert_eq!(
        &unpacked["items"].as_array().unwrap()[1..],
        &[json!(ids[1]), json!(ids[0])]
    );
    assert_eq!(
        call(&mut api, Command::GetPinnedProperties { id: ids[0] })["properties"]["display_name"],
        custom_name
    );
    assert_eq!(
        shelf(&mut api, ShelfTab::Links)["events"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

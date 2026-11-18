use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "synthetic-text-bull".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn finish(api: &mut Service, command: Command) -> Value {
    let job_id = JobId(call(api, command)["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = call(api, Command::PollJob { job_id });
        match status["state"].as_str() {
            Some("succeeded") => return status["result"].clone(),
            Some("queued" | "running" | "ready") if Instant::now() < deadline => {}
            _ => panic!("{status}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn visible_texts(api: &mut Service) -> Value {
    call(
        api,
        Command::BrowseTabSnapshot {
            tab: ShelfTab::Text,
            query: String::new(),
            limit: 20,
        },
    )["events"]
        .clone()
}

struct OwnedText(Arc<Mutex<Vec<String>>>);
impl katran::text_fragments::ClipboardSink for OwnedText {
    fn write(&self, text: &str) -> katran::Result<u32> {
        self.0.lock().unwrap().push(text.into());
        Ok(17)
    }
    fn write_if_current(&self, text: &str, _: u32) -> katran::Result<u32> {
        self.write(text)
    }
}

#[test]
fn text_bull_packages_exact_sources_and_copies_only_the_visible_entry() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), katran::DEFAULT_QUOTA).unwrap();
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
    let sources = [
        "# Заголовок\r\n\r\n**Первый**\r\n",
        "**Второй**\n\n  `точно`\t \n",
    ];
    let ids: Vec<_> = sources
        .iter()
        .map(|source| {
            call(
                &mut api,
                Command::IngestText {
                    text: (*source).into(),
                    source: "synthetic".into(),
                },
            )["id"]
                .as_i64()
                .unwrap()
        })
        .collect();
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    call(
        &mut api,
        Command::Select {
            id: ids[1],
            mode: SelectionMode::Replace,
        },
    );
    call(
        &mut api,
        Command::Select {
            id: ids[0],
            mode: SelectionMode::Range,
        },
    );
    assert!(
        call(&mut api, Command::Actions)
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "create_image_collection")
    );
    let action = || Command::RunAction {
        id: "create_image_collection".into(),
    };
    let packed = finish(&mut api, action());
    assert_eq!(packed["text_collection"], true);
    let card = &packed["event"];
    let id = card["id"].as_i64().unwrap();
    let revision = card["revision"].as_str().unwrap().to_owned();
    assert_eq!(visible_texts(&mut api).as_array().unwrap().len(), 1);
    let exported = katran::copy_out::prepare_with_budget(
        &root.path().join("exports"),
        &[serde_json::from_value(card.clone()).unwrap()],
        katran::DEFAULT_QUOTA,
    )
    .unwrap();
    assert_eq!(exported.paths.len(), 1);
    assert_eq!(exported.paths[0].extension().unwrap(), "bull");
    assert_eq!(
        baloh_collection::text::read(std::fs::File::open(&exported.paths[0]).unwrap()).unwrap(),
        [sources[1], sources[0]]
    );
    let page = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id,
            revision: revision.clone(),
            index: 0,
        },
    );
    assert_eq!(page["text"]["source"], sources[1]);
    assert_eq!(page["count"], 2);
    let writes = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(OwnedText(writes.clone())));
    let copied = finish(
        &mut api,
        Command::CopyCollectionPage {
            id,
            revision,
            index: 1,
        },
    );
    assert_eq!(copied["copied_collection_page"], true);
    assert_eq!(&*writes.lock().unwrap(), &[sources[0]]);
    assert_eq!(visible_texts(&mut api).as_array().unwrap().len(), 1);
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    assert_eq!(finish(&mut api, action())["items"], json!([ids[1], ids[0]]));
    assert_eq!(visible_texts(&mut api).as_array().unwrap().len(), 2);
}

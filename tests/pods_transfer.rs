use katran::{Command, Request, SelectionRef, Service, interaction::ShelfTab};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "pods-transfer".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let r = reply(api, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
fn ingest(api: &mut Service, text: &str) -> SelectionRef {
    let v = call(
        api,
        Command::IngestText {
            text: text.into(),
            source: "test".into(),
        },
    );
    SelectionRef {
        id: v["id"].as_i64().unwrap(),
        revision: v["revision"].as_str().unwrap().into(),
    }
}
struct Sink(Arc<Mutex<Vec<String>>>);
impl katran::text_fragments::ClipboardSink for Sink {
    fn write(&self, text: &str) -> katran::Result<u32> {
        self.0.lock().unwrap().push(text.into());
        Ok(5)
    }
}
type DragItems = Arc<Mutex<Vec<(String, String)>>>;
struct FakeDrag(DragItems);
impl katran::drag_out::Runner for FakeDrag {
    fn run(
        &mut self,
        paths: &[PathBuf],
        _: Option<&katran::drag_out::Preview>,
        _: u64,
    ) -> Result<katran::drag_out::Outcome, String> {
        for path in paths {
            self.0.lock().unwrap().push((
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(path).map_err(|e| e.to_string())?,
            ));
        }
        Ok(katran::drag_out::Outcome {
            dropped: false,
            warnings: vec![],
        })
    }
}
fn setup() -> (
    tempfile::TempDir,
    Service,
    Arc<Mutex<Vec<String>>>,
    DragItems,
) {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    for tab in [ShelfTab::Pods, ShelfTab::Text] {
        call(&mut api, Command::SetTabEnabled { tab, enabled: true });
    }
    let text = Arc::new(Mutex::new(Vec::new()));
    api.set_text_clipboard_sink(Box::new(Sink(text.clone())));
    let drag = Arc::new(Mutex::new(Vec::new()));
    api.set_drag_runner(Box::new(FakeDrag(drag.clone())));
    (root, api, text, drag)
}
fn perform_drag(api: &mut Service, items: Vec<SelectionRef>) -> Value {
    let plan = call(api, Command::DragEventsChecked { items });
    call(
        api,
        Command::RunPreparedDrag {
            token: plan["drag_token"].as_str().unwrap().into(),
            owner_window: 0,
        },
    );
    plan
}
fn wait_job(api: &mut Service, submission: Value) -> Value {
    let job_id = serde_json::from_value(submission["job_id"].clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let value = call(api, Command::PollJob { job_id });
        if value["state"] == "succeeded" {
            return value["result"].clone();
        }
        assert_ne!(value["state"], "failed", "{value}");
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn smart_copy_concatenates_only_pods_and_drag_is_one_file() {
    let (_root, mut api, text, drag) = setup();
    let first = ingest(&mut api, "🐄");
    let second = ingest(&mut api, "🇰🇿");
    let plain = ingest(&mut api, "hello");
    call(
        &mut api,
        Command::CopySmart {
            items: vec![second.clone(), first.clone(), second.clone()],
        },
    );
    assert_eq!(text.lock().unwrap()[0], "🇰🇿🐄");
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopySmart {
                items: vec![first.clone(), second.clone()],
            }),
        },
    );
    wait_job(&mut api, job);
    assert_eq!(text.lock().unwrap()[1], "🐄🇰🇿");
    call(
        &mut api,
        Command::CopySmart {
            items: vec![first.clone(), plain.clone()],
        },
    );
    assert_eq!(text.lock().unwrap()[2], "🐄\n\nhello");
    call(
        &mut api,
        Command::CopyText {
            items: vec![first.clone(), second.clone()],
            separator: " | ".into(),
        },
    );
    assert_eq!(text.lock().unwrap()[3], "🐄 | 🇰🇿");
    let plan = perform_drag(
        &mut api,
        vec![second.clone(), first.clone(), second.clone()],
    );
    assert_eq!(plan["count"], 1);
    assert_eq!(
        drag.lock().unwrap().as_slice(),
        &[("pods.txt".into(), "🇰🇿🐄".into())]
    );
    drag.lock().unwrap().clear();
    let plan = perform_drag(&mut api, vec![first.clone(), plain]);
    assert_eq!(plan["count"], 2);
    assert_eq!(drag.lock().unwrap().len(), 2);
    let mut stale = first;
    stale.revision = "blob:stale".into();
    assert_eq!(
        reply(&mut api, Command::DragEventsChecked { items: vec![stale] })["ok"],
        false
    );
}

use katran::{
    Command, Request, SelectionRef, Service,
    drag_out::{Outcome, Preview, Runner},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::ThreadId,
};
type Calls = Arc<Mutex<Vec<Vec<PathBuf>>>>;

fn execute(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "synthetic-drag".into(),
        command,
    })
}
fn result(api: &mut Service, command: Command) -> Value {
    let r = execute(api, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
struct Fake {
    thread: ThreadId,
    calls: Calls,
    dropped: bool,
    error: bool,
}
impl Runner for Fake {
    fn run(
        &mut self,
        paths: &[PathBuf],
        preview: Option<&Preview>,
        owner: u64,
    ) -> Result<Outcome, String> {
        assert_eq!(std::thread::current().id(), self.thread);
        assert_eq!(owner, 77);
        assert!(paths.iter().all(|p| p.is_file()));
        let preview = preview.expect("synthetic image has a drag preview");
        assert!(preview.width <= 160 && preview.height <= 160);
        assert_eq!(
            preview.rgba.len(),
            (preview.width * preview.height * 4) as usize
        );
        self.calls.lock().unwrap().push(paths.to_vec());
        if self.error {
            Err("synthetic native failure".into())
        } else {
            Ok(Outcome {
                dropped: self.dropped,
                warnings: vec![],
            })
        }
    }
}
fn setup(dropped: bool, error: bool) -> (tempfile::TempDir, Service, SelectionRef, Calls) {
    let root = tempfile::tempdir().unwrap();
    let image_path = root.path().join("original.png");
    image::RgbaImage::from_pixel(24, 12, image::Rgba([23, 45, 67, 255]))
        .save(&image_path)
        .unwrap();
    let mut api = Service::open(&root.path().join("data"), 1024 * 1024).unwrap();
    let event = result(
        &mut api,
        Command::IngestPath {
            path: image_path,
            source: "test".into(),
        },
    );
    let item = SelectionRef {
        id: event["id"].as_i64().unwrap(),
        revision: event["revision"].as_str().unwrap().into(),
    };
    let calls = Arc::new(Mutex::new(vec![]));
    api.set_drag_runner(Box::new(Fake {
        thread: std::thread::current().id(),
        calls: calls.clone(),
        dropped,
        error,
    }));
    (root, api, item, calls)
}

#[test]
fn preparation_off_thread_does_not_enter_native_loop_and_completion_is_one_shot() {
    let (root, mut api, item, calls) = setup(true, false);
    let (mut api, prepared) = std::thread::spawn(move || {
        let prepared = result(
            &mut api,
            Command::DragEventsChecked {
                items: vec![item.clone(), item],
            },
        );
        (api, prepared)
    })
    .join()
    .unwrap();
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(prepared["count"], 1);
    let token = prepared["drag_token"].as_str().unwrap().to_owned();
    let wrong = execute(
        &mut api,
        Command::RunPreparedDrag {
            token: "wrong".into(),
            owner_window: 77,
        },
    );
    assert_eq!(wrong["error"]["code"], "stale_drag");
    let outcome = result(
        &mut api,
        Command::RunPreparedDrag {
            token: token.clone(),
            owner_window: 77,
        },
    );
    assert_eq!(outcome["dropped"], true);
    assert_eq!(outcome["dragged"], 1);
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert!(calls.lock().unwrap()[0][0].is_file());
    let repeated = execute(
        &mut api,
        Command::RunPreparedDrag {
            token,
            owner_window: 77,
        },
    );
    assert_eq!(repeated["error"]["code"], "stale_drag");
    assert!(root.path().join("original.png").is_file());
    assert_eq!(
        result(&mut api, Command::List { limit: 10 })["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn cancel_failure_and_abandoned_preparation_release_files_and_service() {
    for (drop, error) in [(false, false), (false, true)] {
        let (root, mut api, item, calls) = setup(drop, error);
        let prepared = result(
            &mut api,
            Command::DragEventsChecked {
                items: vec![item.clone()],
            },
        );
        let reply = execute(
            &mut api,
            Command::RunPreparedDrag {
                token: prepared["drag_token"].as_str().unwrap().into(),
                owner_window: 77,
            },
        );
        if error {
            assert_eq!(reply["ok"], false);
        } else {
            assert_eq!(reply["result"]["cancelled"], true);
        }
        assert!(!calls.lock().unwrap()[0][0].exists());
        assert!(root.path().join("original.png").is_file());
        let second = result(&mut api, Command::DragEventsChecked { items: vec![item] });
        assert_eq!(
            result(
                &mut api,
                Command::CancelPreparedDrag {
                    token: second["drag_token"].as_str().unwrap().into()
                }
            )["cancelled"],
            true
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("data/drag-cache/exports"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            result(&mut api, Command::List { limit: 10 })["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

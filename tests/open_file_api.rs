use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::SelectionMode,
    open_file::{Launcher, Operation},
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct Fake(Arc<Mutex<Vec<(PathBuf, Operation)>>>);
struct FakeProvider;
impl katran::app_handlers::Provider for FakeProvider {
    fn discover(&self, _: &str) -> katran::Result<Vec<katran::app_handlers::AppHandler>> {
        Ok(vec![])
    }
    fn invoke(&self, _: &str, _: &str, _: &Path) -> katran::Result<()> {
        panic!("unexpected fake provider invocation")
    }
}
impl Launcher for Fake {
    fn launch(&self, path: &Path, op: Operation) -> katran::Result<()> {
        self.0.lock().unwrap().push((path.into(), op));
        Ok(())
    }
}
fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "file-test".into(),
        command,
    })
}
fn ok(api: &mut Service, command: Command) -> Value {
    let v = call(api, command);
    assert_eq!(v["ok"], true, "{v}");
    v["result"].clone()
}
fn terminal(api: &mut Service, id: JobId) -> Value {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let v = ok(api, Command::PollJob { job_id: id });
        if matches!(
            v["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            return v;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn dispatch_actions_jobs_and_snapshots_use_the_validated_file() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(&root.path().join("shelf"), 1024).unwrap();
    let calls = Arc::new(Mutex::new(vec![]));
    api.set_file_launcher(Box::new(Fake(calls.clone())));
    api.set_app_provider(Box::new(FakeProvider));
    let a = root.path().join("one & 名.txt");
    let b = root.path().join("two.txt");
    std::fs::write(&a, b"first").unwrap();
    std::fs::write(&b, b"second").unwrap();
    let a = a.canonicalize().unwrap();
    let id = ok(
        &mut api,
        Command::IngestPath {
            path: a.clone(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    let id2 = ok(
        &mut api,
        Command::IngestPath {
            path: b.clone(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    for op in [Operation::Open, Operation::Reveal, Operation::OpenWith] {
        let out = ok(&mut api, Command::OpenFile { id, operation: op });
        assert_eq!(out["id"], id);
    }
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            (a.clone(), Operation::Open),
            (a.clone(), Operation::Reveal),
            (a.clone(), Operation::OpenWith)
        ]
    );
    ok(&mut api, Command::SetVisible { ids: vec![id, id2] });
    ok(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    for (action, operation) in [("open", Operation::Open), ("reveal", Operation::Reveal)] {
        ok(&mut api, Command::RunAction { id: action.into() });
        assert_eq!(
            calls.lock().unwrap().last().unwrap(),
            &(a.clone(), operation)
        );
    }
    let before_picker = calls.lock().unwrap().len();
    let picker = ok(
        &mut api,
        Command::RunAction {
            id: "open_with".into(),
        },
    );
    assert_eq!(picker["open_apps"], true);
    assert_eq!(calls.lock().unwrap().len(), before_picker);
    ok(
        &mut api,
        Command::OpenWithApp {
            token: picker["token"].as_str().unwrap().into(),
            app_id: None,
        },
    );
    assert_eq!(
        calls.lock().unwrap().last().unwrap(),
        &(a.clone(), Operation::OpenWith)
    );
    let started = ok(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::RunAction {
                id: "reveal".into(),
            }),
        },
    );
    let job: JobId = serde_json::from_value(started["job_id"].clone()).unwrap();
    ok(
        &mut api,
        Command::Select {
            id: id2,
            mode: SelectionMode::Replace,
        },
    );
    assert_eq!(terminal(&mut api, job)["state"], "succeeded");
    assert_eq!(
        calls.lock().unwrap().last().unwrap(),
        &(a.clone(), Operation::Reveal)
    );
    let count = calls.lock().unwrap().len();
    std::fs::write(&a, b"changed").unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::OpenFile {
                id,
                operation: Operation::Open
            }
        )["error"]["code"],
        "source_changed"
    );
    std::fs::remove_file(&a).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::OpenFile {
                id,
                operation: Operation::Open
            }
        )["error"]["code"],
        "file_unavailable"
    );
    assert_eq!(calls.lock().unwrap().len(), count);
    let cached = ok(
        &mut api,
        Command::IngestBytes {
            name: "synthetic.png".into(),
            bytes: b"pixels".to_vec(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    ok(
        &mut api,
        Command::OpenFile {
            id: cached,
            operation: Operation::Open,
        },
    );
    let snapshot = calls.lock().unwrap().last().unwrap().0.clone();
    assert_eq!(snapshot.extension().unwrap(), "png");
    std::fs::write(&snapshot, b"edited").unwrap();
    ok(
        &mut api,
        Command::OpenFile {
            id: cached,
            operation: Operation::OpenWith,
        },
    );
    let copy = calls.lock().unwrap().last().unwrap().0.clone();
    assert_eq!(std::fs::read(copy).unwrap(), b"pixels");
}
#[test]
fn action_defaults_expose_new_file_operations_and_keep_preview() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1024).unwrap();
    let actions = ok(&mut api, Command::Actions);
    let actions = actions.as_array().unwrap();
    for (id, binding) in [
        ("open", "Alt Enter"),
        ("reveal", "Ctrl Enter"),
        ("open_with", "Ctrl Shift Enter"),
        ("preview", "Space"),
    ] {
        let action = actions.iter().find(|a| a["id"] == id).unwrap();
        assert_eq!(action["shortcut"], binding, "{action}");
    }
    let quicklook = actions.iter().find(|a| a["id"] == "quicklook").unwrap();
    assert!(
        quicklook["shortcut"].is_null() || quicklook["shortcut"] == "",
        "{quicklook}"
    );
}

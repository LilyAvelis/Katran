use katran::{
    Command, Request, Service,
    app_handlers::{AppHandler, Provider},
    interaction::ShelfTab,
    open_file::{Launcher, Operation},
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

type Calls = Arc<Mutex<Vec<(String, String, PathBuf)>>>;
struct FakeProvider {
    calls: Calls,
    available: Arc<AtomicBool>,
}
impl Provider for FakeProvider {
    fn discover(&self, _: &str) -> katran::Result<Vec<AppHandler>> {
        Ok(if self.available.load(Ordering::SeqCst) {
            vec![AppHandler {
                id: "editor".into(),
                name: "Editor".into(),
                recommended: true,
                icon: None,
            }]
        } else {
            vec![]
        })
    }
    fn invoke(&self, ext: &str, id: &str, path: &Path) -> katran::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push((ext.into(), id.into(), path.into()));
        Ok(())
    }
}
struct FakeLauncher(Arc<Mutex<Vec<(PathBuf, Operation)>>>);
impl Launcher for FakeLauncher {
    fn launch(&self, path: &Path, op: Operation) -> katran::Result<()> {
        self.0.lock().unwrap().push((path.into(), op));
        Ok(())
    }
}
fn reply(service: &mut Service, command: Command) -> Value {
    service.execute(Request {
        api_version: 1,
        request_id: "pinned-opener".into(),
        command,
    })
}
fn call(service: &mut Service, command: Command) -> Value {
    let value = reply(service, command);
    assert_eq!(value["ok"], true, "{value}");
    value["result"].clone()
}
fn properties(service: &mut Service, id: i64, opener: Option<&str>) -> Value {
    reply(
        service,
        Command::SetPinnedProperties {
            id,
            display_name: Some("  My name  ".into()),
            opener: opener.map(PathBuf::from),
        },
    )
}

#[test]
fn pinned_default_validates_current_handler_and_leaves_explicit_actions_alone() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("original.txt");
    std::fs::write(&path, b"original").unwrap();
    let mut service = Service::open(&root.path().join("shelf"), 1024 * 1024).unwrap();
    let calls = Calls::default();
    let available = Arc::new(AtomicBool::new(true));
    service.set_app_provider(Box::new(FakeProvider {
        calls: calls.clone(),
        available: available.clone(),
    }));
    let defaults = Arc::new(Mutex::new(Vec::new()));
    service.set_file_launcher(Box::new(FakeLauncher(defaults.clone())));
    let id = call(
        &mut service,
        Command::IngestPath {
            path: path.clone(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut service, Command::TogglePins { ids: vec![id] });
    assert_eq!(
        properties(&mut service, id, Some("C:\\untrusted.exe"))["ok"],
        false
    );
    assert_eq!(properties(&mut service, id, Some("editor"))["ok"], true);
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    call(
        &mut service,
        Command::OpenFile {
            id,
            operation: Operation::Open,
        },
    );
    assert_eq!(
        calls.lock().unwrap()[0],
        (".txt".into(), "editor".into(), path.canonicalize().unwrap())
    );
    call(
        &mut service,
        Command::OpenFile {
            id,
            operation: Operation::Reveal,
        },
    );
    assert_eq!(defaults.lock().unwrap()[0].1, Operation::Reveal);
    let picker = call(&mut service, Command::ListOpenApps { id });
    call(
        &mut service,
        Command::OpenWithApp {
            token: picker["token"].as_str().unwrap().into(),
            app_id: None,
        },
    );
    assert_eq!(defaults.lock().unwrap()[1].1, Operation::OpenWith);
    available.store(false, Ordering::SeqCst);
    assert_eq!(
        reply(
            &mut service,
            Command::OpenFile {
                id,
                operation: Operation::Open
            }
        )["error"]["code"],
        "handler_unavailable"
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(defaults.lock().unwrap().len(), 2);
    call(&mut service, Command::TogglePins { ids: vec![id] });
    call(
        &mut service,
        Command::OpenFile {
            id,
            operation: Operation::Open,
        },
    );
    assert_eq!(defaults.lock().unwrap()[2].1, Operation::Open);
    let saved = call(&mut service, Command::GetPinnedProperties { id });
    assert_eq!(saved["properties"]["opener"], "editor");
    assert_eq!(saved["properties"]["display_name"], "My name");
}

#[test]
fn selected_link_handler_receives_validated_url() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    let calls = Calls::default();
    service.set_app_provider(Box::new(FakeProvider {
        calls: calls.clone(),
        available: Arc::new(AtomicBool::new(true)),
    }));
    call(
        &mut service,
        Command::SetTabEnabled {
            tab: ShelfTab::Links,
            enabled: true,
        },
    );
    let id = call(
        &mut service,
        Command::IngestLink {
            url: "https://example.test/path?q=1".into(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut service, Command::TogglePins { ids: vec![id] });
    assert_eq!(properties(&mut service, id, Some("editor"))["ok"], true);
    call(
        &mut service,
        Command::OpenFile {
            id,
            operation: Operation::Open,
        },
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].0, ".html");
    assert_eq!(calls[0].2, PathBuf::from("https://example.test/path?q=1"));
}

#[test]
fn owned_object_opens_independent_named_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    let calls = Calls::default();
    service.set_app_provider(Box::new(FakeProvider {
        calls: calls.clone(),
        available: Arc::new(AtomicBool::new(true)),
    }));
    let event = call(
        &mut service,
        Command::IngestBytes {
            name: "owned.png".into(),
            bytes: b"owned synthetic bytes".to_vec(),
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    call(&mut service, Command::TogglePins { ids: vec![id] });
    assert_eq!(properties(&mut service, id, Some("editor"))["ok"], true);
    let result = call(
        &mut service,
        Command::OpenFile {
            id,
            operation: Operation::Open,
        },
    );
    assert_eq!(result["snapshot"], true);
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].0, ".png");
    assert_ne!(calls[0].2, PathBuf::from(event["path"].as_str().unwrap()));
    std::fs::write(&calls[0].2, b"edited snapshot").unwrap();
    assert_eq!(
        std::fs::read(event["path"].as_str().unwrap()).unwrap(),
        b"owned synthetic bytes"
    );
}

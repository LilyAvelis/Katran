use katran::{Command, Request, Service};
use std::sync::{Arc, Mutex};
struct Fake(Arc<Mutex<Vec<std::path::PathBuf>>>);
impl katran::quicklook::Launcher for Fake {
    fn launch(&self, path: &std::path::Path) -> katran::Result<()> {
        self.0.lock().unwrap().push(path.into());
        Ok(())
    }
}
#[test]
fn space_action_dispatches_nonimages_to_external_boundary_and_images_remain_internal() {
    let root = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("document.txt");
    std::fs::write(&path, b"synthetic").unwrap();
    let mut api = Service::open(root.path(), 1000).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    api.set_preview_launcher(Box::new(Fake(calls.clone())));
    let mut call = |command| {
        api.execute(Request {
            api_version: 1,
            request_id: "preview".into(),
            command,
        })
    };
    let r = call(Command::IngestPath {
        path: path.clone(),
        source: "test".into(),
    });
    let id = r["result"]["id"].as_i64().unwrap();
    call(Command::SetVisible { ids: vec![id] });
    let r = call(Command::RunAction {
        id: "preview".into(),
    });
    assert_eq!(r["result"]["backend"], "quicklook");
    assert_eq!(calls.lock().unwrap().len(), 1);
    let r = call(Command::IngestBytes {
        name: "native.png".into(),
        bytes: vec![1],
        source: "test".into(),
    });
    let id = r["result"]["id"].as_i64().unwrap();
    call(Command::SetVisible { ids: vec![id] });
    assert_eq!(
        call(Command::RunAction {
            id: "preview".into()
        })["result"]["preview"],
        id
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn explicit_quicklook_action_is_visible_rebindable_and_bypasses_image_preview() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1000).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    api.set_preview_launcher(Box::new(Fake(calls.clone())));
    let mut call = |command| {
        api.execute(Request {
            api_version: 1,
            request_id: "explicit".into(),
            command,
        })
    };
    let r = call(Command::IngestBytes {
        name: "image.png".into(),
        bytes: vec![1],
        source: "test".into(),
    });
    let id = r["result"]["id"].as_i64().unwrap();
    call(Command::SetVisible { ids: vec![id] });
    let actions = call(Command::Actions);
    assert!(
        actions["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "quicklook" && a["shortcut"] == "Ctrl Enter")
    );
    let r = call(Command::RunAction {
        id: "quicklook".into(),
    });
    assert_eq!(r["result"]["backend"], "quicklook");
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert!(call(Command::Interaction)["result"]["preview"].is_null());
    assert_eq!(
        call(Command::SetShortcut {
            id: "quicklook".into(),
            binding: Some("Ctrl Q".into()),
            replace: false,
            reset: false
        })["ok"],
        true
    );
}

#[path = "common/source_api.rs"]
mod source_api;
use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex}, time::{Duration, Instant}};

struct Fake(Arc<Mutex<Vec<PathBuf>>>);
impl katran::quicklook::Launcher for Fake {
    fn launch(&self, path: &Path) -> katran::Result<()> { self.0.lock().unwrap().push(path.into()); Ok(()) }
}
fn call(api: &mut Service, command: Command) -> Value {
    let r = api.execute(Request { api_version: 1, request_id: "parity".into(), command });
    assert_eq!(r["ok"], true, "{r}"); r["result"].clone()
}
fn preview(api: &mut Service) -> Value {
    let start = call(api, Command::BeginJob { command: Box::new(Command::RunAction { id: "preview".into() }) });
    if start["state"] == "succeeded" { return start["result"].clone(); }
    let job_id = serde_json::from_value(start["job_id"].clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = call(api, Command::PollJob { job_id });
        if status["state"] == "succeeded" { return status["result"].clone(); }
        assert_ne!(status["state"], "failed", "{status}");
        assert!(Instant::now() < deadline, "{status}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn preview_uses_same_event_in_ordinary_and_pinned_slices() {
    let root = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let mut settings = katran::settings::Settings::defaults(1_000_000);
    settings.downloads_folder = Some(files.path().to_owned());
    settings.downloads_enabled = true;
    call(&mut api, Command::SetSettings { settings });
    let calls = Arc::new(Mutex::new(Vec::new()));
    api.set_preview_launcher(Box::new(Fake(calls.clone())));
    for name in ["document.bin", "picture.png"] {
        let path = files.path().join(name);
        if name.ends_with("png") { image::RgbaImage::new(8, 4).save(&path).unwrap(); }
        else { std::fs::write(&path, b"synthetic").unwrap(); }
        let event = source_api::wait_event(&mut api, katran::source_policy::SourceId::Downloads, &path);
        let id = event["id"].as_i64().unwrap();
        call(&mut api, Command::TogglePins { ids: vec![id] });
        call(&mut api, Command::SetPinnedProperties { id, display_name: Some("Замок".into()), opener: None });
        call(&mut api, Command::SetTab { tab: ShelfTab::Downloads });
        let page = call(&mut api, Command::BrowseShelfSorted { query: "".into(), limit: 100, cursor: None });
        assert!(page["events"].as_array().unwrap().iter().any(|e| e["id"] == id), "ordinary page: {page}");
        call(&mut api, Command::SetVisible { ids: vec![id] });
        let ordinary = preview(&mut api);
        call(&mut api, Command::SetTab { tab: ShelfTab::Downloads });
        let state = call(&mut api, Command::SelectTabPosition { position: 3 });
        assert_eq!(state["pinned_only"], true);
        let page = call(&mut api, Command::BrowseShelfSorted { query: "".into(), limit: 100, cursor: None });
        let listed = page["events"].as_array().unwrap().iter().find(|e| e["id"] == id).unwrap_or_else(|| panic!("pinned page: {page}"));
        assert_eq!(listed["name"], name);
        assert_eq!(listed["revision"], event["revision"]);
        call(&mut api, Command::SetVisible { ids: vec![id] });
        let pinned = preview(&mut api);
        if name.ends_with("png") {
            assert_eq!(ordinary["preview"], id);
            assert_eq!(pinned["preview"], id);
        } else {
            for result in [&ordinary, &pinned] {
                assert_eq!(result["backend"], "quicklook");
                assert_eq!(result["id"], id);
                assert_eq!(result["revision"], event["revision"]);
            }
            assert_eq!(*calls.lock().unwrap(), vec![path.canonicalize().unwrap(); 2]);
        }
    }
    assert_eq!(calls.lock().unwrap().len(), 2);
}

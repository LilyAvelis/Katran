#[path = "common/source_api.rs"]
mod source_api;
use katran::{Command, Service, domain_jobs::JobId, source_policy::SourceId};
use serde_json::Value;
use source_api::call;
use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
struct Blocking {
    started: mpsc::Sender<()>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
}
impl katran::quicklook::Launcher for Blocking {
    fn launch(&self, _: &Path) -> katran::Result<()> {
        self.started.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(6))
            .unwrap();
        Ok(())
    }
}
fn configure(service: &mut Service, shots: &Path) {
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(service, Command::GetSettings)).unwrap();
    settings.downloads_enabled = false;
    settings.sharex_folder = Some(shots.into());
    call(service, Command::SetSettings { settings });
}
fn screenshot(path: &Path) {
    image::RgbaImage::from_pixel(4, 3, image::Rgba([20, 30, 40, 255]))
        .save(path)
        .unwrap();
}
fn poll(service: &mut Service, submission: Value) -> Value {
    if submission["state"] == "succeeded" {
        return submission["result"].clone();
    }
    let id: JobId = serde_json::from_value(submission["job_id"].clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let status = call(service, Command::PollJob { job_id: id });
        if status["state"] == "succeeded" {
            return status["result"].clone();
        }
        assert_ne!(status["state"], "failed", "{status}");
        assert!(Instant::now() < deadline, "{status}");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn queued_original_delete_rejects_settings_change_before_authorization() {
    let root = tempfile::tempdir().unwrap();
    let shots = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let path = shots.path().join("synthetic.png");
    screenshot(&path);
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    configure(&mut service, shots.path());
    let event = source_api::wait_event(&mut service, SourceId::Sharex, &path);
    let id = event["id"].as_i64().unwrap();
    let preview = external.path().join("preview.bin");
    std::fs::write(&preview, b"preview fixture").unwrap();
    let preview_id = call(
        &mut service,
        Command::IngestPath {
            path: preview,
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    let (started, observed) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    service.set_preview_launcher(Box::new(Blocking {
        started,
        release: Arc::new(Mutex::new(blocked)),
    }));
    let preview_job = call(
        &mut service,
        Command::BeginJob {
            command: Box::new(Command::PreviewExternal { id: preview_id }),
        },
    );
    observed.recv_timeout(Duration::from_secs(6)).unwrap();
    let deletion = call(
        &mut service,
        Command::BeginJob {
            command: Box::new(Command::DeleteSelection {
                ids: vec![id],
                confirmed: false,
            }),
        },
    );
    configure(&mut service, other.path());
    release.send(()).unwrap();
    poll(&mut service, preview_job);
    let result = poll(&mut service, deletion);
    assert_eq!(result["deleted"], 0);
    assert_eq!(result["items"][0]["ok"], false);
    assert!(path.is_file());
    assert_eq!(call(&mut service, Command::Get { id })["id"], id);
}
#[test]
fn asynchronous_single_delete_requires_actor_authorization_and_reports_receipt() {
    let root = tempfile::tempdir().unwrap();
    let shots = tempfile::tempdir().unwrap();
    let path = shots.path().join("synthetic.png");
    screenshot(&path);
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    configure(&mut service, shots.path());
    let event = source_api::wait_event(&mut service, SourceId::Sharex, &path);
    let id = event["id"].as_i64().unwrap();
    let deletion = call(
        &mut service,
        Command::BeginJob {
            command: Box::new(Command::DeleteSelection {
                ids: vec![id, id],
                confirmed: false,
            }),
        },
    );
    assert!(
        path.exists(),
        "preparation must not remove the original before actor authorization"
    );
    let result = poll(&mut service, deletion);
    assert_eq!(result["deleted"], 1);
    assert_eq!(result["items"][0]["original_removed"], true);
    assert!(!path.exists());
    let absent = service.execute(katran::Request {
        api_version: 1,
        request_id: "absent".into(),
        command: Command::Get { id },
    });
    assert_eq!(absent["ok"], false);
}
struct BlockingTrash {
    started: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl katran::downloads::Trash for BlockingTrash {
    fn recycle(&self, path: &Path) -> katran::Result<()> {
        self.started.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(6))
            .unwrap();
        std::fs::remove_file(path).map_err(Into::into)
    }
}
#[test]
fn authorized_recycle_effect_keeps_actor_responsive_and_cannot_be_cancelled() {
    let root = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let next = tempfile::tempdir().unwrap();
    let path = downloads.path().join("synthetic.bin");
    std::fs::write(&path, b"synthetic recyclable original").unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    settings.sharex_folder = None;
    settings.downloads_enabled = true;
    settings.downloads_folder = Some(downloads.path().into());
    call(&mut service, Command::SetSettings { settings });
    let event = source_api::wait_event(&mut service, SourceId::Downloads, &path);
    let id = event["id"].as_i64().unwrap();
    let (started, observed) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    service.set_trash(Box::new(BlockingTrash {
        started,
        release: Mutex::new(blocked),
    }));
    let deletion = call(
        &mut service,
        Command::BeginJob {
            command: Box::new(Command::DeleteSelection {
                ids: vec![id],
                confirmed: false,
            }),
        },
    );
    let job_id: JobId = serde_json::from_value(deletion["job_id"].clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        call(&mut service, Command::PollJob { job_id });
        if observed.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let started = Instant::now();
    call(&mut service, Command::Interaction);
    call(&mut service, Command::Status);
    assert!(started.elapsed() < Duration::from_millis(500));
    let cancelled = service.execute(katran::Request {
        api_version: 1,
        request_id: "cancel-effect".into(),
        command: Command::CancelJob { job_id },
    });
    assert_eq!(cancelled["ok"], false);
    assert_eq!(cancelled["error"]["code"], "job_not_cancellable");
    // Authorization is the explicit point of no return; changing the source
    // afterward cannot undo the already-started external operation.
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    settings.downloads_folder = Some(next.path().into());
    call(&mut service, Command::SetSettings { settings });
    assert!(path.exists());
    release.send(()).unwrap();
    let result = poll(&mut service, deletion);
    assert_eq!(result["deleted"], 1);
    assert_eq!(result["items"][0]["original_removed"], true);
    assert!(!path.exists());
}

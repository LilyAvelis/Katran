use katran::{
    Command, Request, Service,
    domain_jobs::Cancellation,
    interaction::SelectionMode,
    media_playback::{
        Action, Backend, CaptureTask, CapturedFrame, Frame, PlaybackState, Snapshot, Source,
    },
};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
struct FakeClipboard {
    bytes: Arc<std::sync::Mutex<Vec<u8>>>,
    fail: bool,
}
impl katran::copy_out::ClipboardSink for FakeClipboard {
    fn write(
        &mut self,
        paths: &[std::path::PathBuf],
    ) -> std::result::Result<(u32, Vec<String>), String> {
        assert_eq!(paths.len(), 1);
        if self.fail {
            return Err("synthetic clipboard failure".into());
        }
        *self.bytes.lock().unwrap() = std::fs::read(&paths[0]).unwrap();
        Ok((0, Vec::new()))
    }
}
struct Task {
    release: Arc<AtomicBool>,
}
impl CaptureTask for Task {
    fn wait(self: Box<Self>, cancel: &Cancellation) -> katran::Result<CapturedFrame> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.release.load(Ordering::Acquire) {
            cancel.check()?;
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(CapturedFrame {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
            position_ms: 1234,
        })
    }
}
struct Fake {
    release: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
}
impl Backend for Fake {
    fn open(&mut self, _: Source) -> katran::Result<()> {
        Ok(())
    }
    fn control(&mut self, action: Action) -> katran::Result<()> {
        match action {
            Action::Play => self.playing.store(true, Ordering::Release),
            Action::Pause => self.playing.store(false, Ordering::Release),
            _ => (),
        }
        Ok(())
    }
    fn snapshot(&mut self) -> katran::Result<Snapshot> {
        Ok(Snapshot {
            state: if self.playing.load(Ordering::Acquire) {
                PlaybackState::Playing
            } else {
                PlaybackState::Paused
            },
            duration_ms: Some(10000),
            position_ms: 1234,
            volume: 1.0,
            has_audio: true,
            width: 2,
            height: 1,
            error: None,
            acceleration: None,
            step_pending: false,
        })
    }
    fn frame(&mut self, _: u64) -> katran::Result<Option<Frame>> {
        Ok(None)
    }
    fn request_capture(&mut self) -> katran::Result<Box<dyn CaptureTask>> {
        Ok(Box::new(Task {
            release: self.release.clone(),
        }))
    }
    fn close(&mut self) {}
}
fn call(service: &mut Service, command: Command) -> Value {
    service.execute(Request {
        api_version: 1,
        request_id: "capture-test".into(),
        command,
    })
}
fn ok(service: &mut Service, command: Command) -> Value {
    let v = call(service, command);
    assert_eq!(v["ok"], true, "{v}");
    v["result"].clone()
}
fn poll(service: &mut Service, job_id: katran::domain_jobs::JobId) -> Value {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let status = ok(service, Command::PollJob { job_id });
        if status["state"] == "succeeded" || status["state"] == "failed" {
            return status;
        }
        assert!(Instant::now() < end, "{status}");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn video_actions_are_contextual_and_pause_uses_current_session() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1_000_000).unwrap();
    let ordinary = ok(&mut service, Command::Actions);
    assert!(
        !ordinary
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["group"] == "Видео" || a["group"] == "Video")
    );
    let playing = Arc::new(AtomicBool::new(false));
    let token = fixture(
        &mut service,
        folder.path(),
        Arc::new(AtomicBool::new(true)),
        playing.clone(),
    );
    let actions = ok(&mut service, Command::Actions);
    let video: Vec<_> = actions
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["group"] == "Видео" || a["group"] == "Video")
        .collect();
    let mut ids: Vec<_> = video.iter().filter_map(|a| a["id"].as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["capture_video_frame", "extract_video_audio", "video_pause"]
    );
    assert!(video.iter().all(|a| a["enabled"] == true));
    ok(
        &mut service,
        Command::RunAction {
            id: "video_pause".into(),
        },
    );
    assert!(playing.load(Ordering::Acquire));
    let actions = ok(&mut service, Command::Actions);
    let capture = actions
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "capture_video_frame")
        .unwrap();
    assert_eq!(capture["enabled"], false);
    assert!(capture["disabled_reason"].as_str().is_some());
    assert_eq!(
        call(
            &mut service,
            Command::ToggleMediaPause {
                session: Some(token + 1)
            }
        )["error"]["code"],
        "stale_media_session"
    );
    ok(
        &mut service,
        Command::ToggleMediaPause {
            session: Some(token),
        },
    );
    assert!(!playing.load(Ordering::Acquire));
}
fn fixture(
    service: &mut Service,
    directory: &std::path::Path,
    release: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
) -> u64 {
    std::fs::write(directory.join("sample.mp4"), b"synthetic video").unwrap();
    service
        .set_media_backend(Box::new(Fake { release, playing }))
        .unwrap();
    ok(
        service,
        Command::ChooseFolder {
            path: directory.into(),
        },
    );
    let page = ok(
        service,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 10,
            cursor: None,
        },
    );
    let event = &page["events"][0];
    let id = event["id"].as_i64().unwrap();
    let revision = event["revision"].as_str().unwrap().to_owned();
    ok(service, Command::SetVisible { ids: vec![id] });
    ok(
        service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    ok(service, Command::OpenCurrent);
    ok(service, Command::OpenMedia { id, revision })["token"]
        .as_u64()
        .unwrap()
}
#[test]
fn saves_full_frame_as_managed_image_even_when_images_hidden() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let release = Arc::new(AtomicBool::new(true));
    let playing = Arc::new(AtomicBool::new(false));
    let mut service = Service::open(root.path(), 1_000_000).unwrap();
    let token = fixture(&mut service, folder.path(), release, playing);
    let copied = Arc::new(std::sync::Mutex::new(Vec::new()));
    service.set_media_capture_clipboard_sink(Box::new(FakeClipboard {
        bytes: copied.clone(),
        fail: false,
    }));
    ok(
        &mut service,
        Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Images,
            enabled: false,
        },
    );
    let before = ok(&mut service, Command::Interaction);
    let queued = ok(
        &mut service,
        Command::CaptureMediaFrame {
            session: Some(token),
        },
    );
    let job_id = serde_json::from_value(queued["job_id"].clone()).unwrap();
    let status = poll(&mut service, job_id);
    assert_eq!(status["state"], "succeeded", "{status}");
    let result = &status["result"];
    assert_eq!(result["event"]["kind"], "managed_image");
    assert_eq!(result["event"]["source"], "video_frame");
    assert_eq!(result["clipboard"]["ok"], true);
    assert_eq!(
        *copied.lock().unwrap(),
        std::fs::read(result["event"]["path"].as_str().unwrap()).unwrap()
    );
    assert_eq!(
        std::fs::read(folder.path().join("sample.mp4")).unwrap(),
        b"synthetic video"
    );
    assert_eq!(
        (
            result["width"].as_u64(),
            result["height"].as_u64(),
            result["position_ms"].as_u64()
        ),
        (Some(2), Some(1), Some(1234))
    );
    let png =
        image::load_from_memory(&std::fs::read(result["event"]["path"].as_str().unwrap()).unwrap())
            .unwrap()
            .to_rgba8();
    assert_eq!(png.into_raw(), vec![255, 0, 0, 255, 0, 255, 0, 255]);
    let after = ok(&mut service, Command::Interaction);
    assert_eq!(before["preview"], after["preview"]);
    assert_eq!(before["tab"], after["tab"]);
    assert_eq!(before["cursor"], after["cursor"]);
    assert_eq!(
        ok(&mut service, Command::MediaState)["snapshot"]["state"],
        "paused"
    );
}
#[test]
fn rejects_playing_stale_token_and_late_step_capture() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let release = Arc::new(AtomicBool::new(false));
    let playing = Arc::new(AtomicBool::new(true));
    let mut service = Service::open(root.path(), 1_000_000).unwrap();
    let token = fixture(
        &mut service,
        folder.path(),
        release.clone(),
        playing.clone(),
    );
    assert_eq!(
        call(
            &mut service,
            Command::CaptureMediaFrame {
                session: Some(token)
            }
        )["error"]["code"],
        "media_capture_requires_pause"
    );
    playing.store(false, Ordering::Release);
    assert_eq!(
        call(
            &mut service,
            Command::CaptureMediaFrame {
                session: Some(token + 1)
            }
        )["error"]["code"],
        "stale_media_session"
    );
    let queued = ok(
        &mut service,
        Command::BeginJob {
            command: Box::new(Command::CaptureMediaFrame { session: None }),
        },
    );
    let job_id = serde_json::from_value(queued["job_id"].clone()).unwrap();
    ok(
        &mut service,
        Command::ControlMedia {
            session: token,
            action: Action::StepFrame { backwards: false },
        },
    );
    release.store(true, Ordering::Release);
    let status = poll(&mut service, job_id);
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(status["error"]["code"], "stale_media_context");
    assert_eq!(
        ok(&mut service, Command::List { limit: 100 })["events"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
#[test]
fn changed_video_discards_pending_capture_without_new_image() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let release = Arc::new(AtomicBool::new(false));
    let playing = Arc::new(AtomicBool::new(false));
    let mut service = Service::open(root.path(), 1_000_000).unwrap();
    let token = fixture(&mut service, folder.path(), release.clone(), playing);
    let queued = ok(
        &mut service,
        Command::CaptureMediaFrame {
            session: Some(token),
        },
    );
    let job_id = serde_json::from_value(queued["job_id"].clone()).unwrap();
    std::fs::write(
        folder.path().join("sample.mp4"),
        b"modified synthetic video with new length",
    )
    .unwrap();
    release.store(true, Ordering::Release);
    let status = poll(&mut service, job_id);
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(
        ok(&mut service, Command::List { limit: 100 })["events"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn clipboard_failure_preserves_saved_image_and_reports_partial_success() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1_000_000).unwrap();
    let token = fixture(
        &mut service,
        folder.path(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicBool::new(false)),
    );
    service.set_media_capture_clipboard_sink(Box::new(FakeClipboard {
        bytes: Arc::new(std::sync::Mutex::new(Vec::new())),
        fail: true,
    }));
    let queued = ok(
        &mut service,
        Command::CaptureMediaFrame {
            session: Some(token),
        },
    );
    let job_id = serde_json::from_value(queued["job_id"].clone()).unwrap();
    let status = poll(&mut service, job_id);
    assert_eq!(status["state"], "succeeded", "{status}");
    assert_eq!(status["result"]["clipboard"]["ok"], false);
    assert_eq!(
        status["result"]["clipboard"]["error"],
        "synthetic clipboard failure"
    );
    assert!(std::path::Path::new(status["result"]["event"]["path"].as_str().unwrap()).is_file());
    assert_eq!(
        std::fs::read(folder.path().join("sample.mp4")).unwrap(),
        b"synthetic video"
    );
}

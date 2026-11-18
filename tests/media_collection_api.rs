use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
    media_playback::{Action, Backend, Frame, PlaybackState, Snapshot, Source},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Observed {
    paths: Vec<PathBuf>,
    exists_on_close: Vec<bool>,
}
struct Fake(Arc<Mutex<Observed>>);
impl Backend for Fake {
    fn open(&mut self, source: Source) -> katran::Result<()> {
        self.0.lock().unwrap().paths.push(source.path);
        Ok(())
    }
    fn control(&mut self, _: Action) -> katran::Result<()> {
        Ok(())
    }
    fn snapshot(&mut self) -> katran::Result<Snapshot> {
        Ok(Snapshot {
            state: PlaybackState::Paused,
            duration_ms: Some(100),
            position_ms: 0,
            volume: 1.0,
            has_audio: true,
            width: 1,
            height: 1,
            error: None,
            acceleration: None,
            step_pending: false,
        })
    }
    fn frame(&mut self, _: u64) -> katran::Result<Option<Frame>> {
        Ok(None)
    }
    fn close(&mut self) {
        let mut state = self.0.lock().unwrap();
        let exists = state.paths.last().is_some_and(|p| p.exists());
        state.exists_on_close.push(exists);
    }
}
fn call(service: &mut Service, command: Command) -> Value {
    service.execute(Request {
        api_version: 1,
        request_id: "synthetic-collection-media".into(),
        command,
    })
}
fn ok(service: &mut Service, command: Command) -> Value {
    let result = call(service, command);
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
}
fn finish(service: &mut Service, job: Value) -> Value {
    let id: JobId = serde_json::from_value(job["job_id"].clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = ok(service, Command::PollJob { job_id: id });
        if !matches!(
            status["state"].as_str(),
            Some("queued" | "running" | "ready")
        ) {
            return status;
        }
        assert!(Instant::now() < deadline, "{status}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn archive(service: &mut Service) -> (i64, String) {
    use baloh_collection::bs::{self, FileEntry};
    let mut bytes = Vec::new();
    bs::write(
        &mut bytes,
        &[
            FileEntry {
                name: "first.mp4".into(),
                bytes: b"video-one".to_vec(),
            },
            FileEntry {
                name: "second.webm".into(),
                bytes: b"video-two".to_vec(),
            },
            FileEntry {
                name: "note.txt".into(),
                bytes: b"not video".to_vec(),
            },
        ],
    )
    .unwrap();
    let event = ok(
        service,
        Command::IngestBytes {
            name: "synthetic.bs".into(),
            bytes,
            source: "bs_collection".into(),
        },
    );
    (
        event["id"].as_i64().unwrap(),
        event["revision"].as_str().unwrap().to_owned(),
    )
}
fn select(service: &mut Service, id: i64) {
    ok(
        service,
        Command::SetTabEnabled {
            tab: ShelfTab::Files,
            enabled: true,
        },
    );
    ok(
        service,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    ok(service, Command::SetVisible { ids: vec![id] });
    ok(
        service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
}
fn start(service: &mut Service, id: i64, revision: &str, index: usize) -> Value {
    let job = ok(
        service,
        Command::OpenCollectionMedia {
            id,
            revision: revision.into(),
            index,
        },
    );
    finish(service, job)
}
#[test]
fn collection_video_is_one_session_with_owned_snapshot_and_no_capture() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1_000_000).unwrap();
    let observed = Arc::new(Mutex::new(Observed::default()));
    service
        .set_media_backend(Box::new(Fake(observed.clone())))
        .unwrap();
    let (id, revision) = archive(&mut service);
    select(&mut service, id);
    let status = start(&mut service, id, &revision, 0);
    assert_eq!(status["state"], "succeeded", "{status}");
    assert_eq!(status["result"]["collection_index"], 0);
    let first_token = status["result"]["token"].as_u64().unwrap();
    let path = observed.lock().unwrap().paths.last().unwrap().clone();
    assert_eq!(std::fs::read(&path).unwrap(), b"video-one");
    assert_eq!(
        call(
            &mut service,
            Command::CaptureMediaFrame {
                session: Some(first_token)
            }
        )["error"]["code"],
        "media_capture_unavailable"
    );
    let next = start(&mut service, id, &revision, 1);
    assert_eq!(next["state"], "succeeded", "{next}");
    assert!(!path.exists());
    assert_eq!(
        call(
            &mut service,
            Command::ControlMedia {
                session: first_token,
                action: Action::Play
            }
        )["error"]["code"],
        "stale_media_session"
    );
    let second_path = observed.lock().unwrap().paths.last().unwrap().clone();
    assert_eq!(std::fs::read(&second_path).unwrap(), b"video-two");
    let token = next["result"]["token"].as_u64().unwrap();
    ok(
        &mut service,
        Command::ControlMedia {
            session: token,
            action: Action::Stop,
        },
    );
    assert!(!second_path.exists());
    assert!(observed.lock().unwrap().exists_on_close.iter().all(|x| *x));
}
#[test]
fn invalid_or_stale_collection_context_never_opens_decoder() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1_000_000).unwrap();
    let observed = Arc::new(Mutex::new(Observed::default()));
    service
        .set_media_backend(Box::new(Fake(observed.clone())))
        .unwrap();
    let (id, revision) = archive(&mut service);
    select(&mut service, id);
    let bad = start(&mut service, id, &revision, 2);
    assert_eq!(bad["state"], "failed", "{bad}");
    assert_eq!(bad["error"]["code"], "invalid_media_source");
    let job = ok(
        &mut service,
        Command::OpenCollectionMedia {
            id,
            revision: revision.clone(),
            index: 0,
        },
    );
    ok(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let result = finish(&mut service, job);
    assert_eq!(result["state"], "failed", "{result}");
    assert_eq!(result["error"]["code"], "stale_media_context");
    assert!(observed.lock().unwrap().paths.is_empty());
}

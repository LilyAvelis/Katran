use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, View},
    media_playback::{Action, Backend, Frame, PlaybackState, Snapshot, Source},
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
#[derive(Default)]
struct Counts {
    opens: usize,
    closes: usize,
}
struct Fake(Arc<Mutex<Counts>>);
impl Backend for Fake {
    fn open(&mut self, _: Source) -> katran::Result<()> {
        self.0.lock().unwrap().opens += 1;
        Ok(())
    }
    fn control(&mut self, _: Action) -> katran::Result<()> {
        Ok(())
    }
    fn snapshot(&mut self) -> katran::Result<Snapshot> {
        Ok(Snapshot {
            state: PlaybackState::Loading,
            duration_ms: Some(100),
            position_ms: 0,
            volume: 1.0,
            has_audio: true,
            width: 1,
            height: 1,
            error: None,
        })
    }
    fn frame(&mut self, _: u64) -> katran::Result<Option<Frame>> {
        Ok(Some(Frame {
            serial: 1,
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 4],
        }))
    }
    fn close(&mut self) {
        self.0.lock().unwrap().closes += 1;
    }
}
fn call(s: &mut Service, command: Command) -> Value {
    s.execute(Request {
        api_version: 1,
        request_id: "test".into(),
        command,
    })
}
fn ok(s: &mut Service, command: Command) -> Value {
    let r = call(s, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn preview_controls_validate_context_and_source() {
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::open(temp.path(), 100000).unwrap();
    let counts = Arc::new(Mutex::new(Counts::default()));
    service
        .set_media_backend(Box::new(Fake(counts.clone())))
        .unwrap();
    let event = ok(
        &mut service,
        Command::IngestBytes {
            name: "test.mp4".into(),
            bytes: vec![1, 2, 3],
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    let revision = event["revision"].as_str().unwrap().to_owned();
    ok(&mut service, Command::SetVisible { ids: vec![id] });
    ok(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    ok(
        &mut service,
        Command::SetView {
            view: View::Details,
        },
    );
    let session = ok(
        &mut service,
        Command::OpenMedia {
            id,
            revision: revision.clone(),
        },
    );
    let token = session["token"].as_u64().unwrap();
    let frame = ok(
        &mut service,
        Command::MediaFrame {
            session: token,
            after_serial: 0,
        },
    );
    assert_eq!(frame["frame"]["rgba_base64"], "AQIDBA==");
    assert_eq!(
        call(
            &mut service,
            Command::ControlMedia {
                session: token + 1,
                action: Action::Play
            }
        )["error"]["code"],
        "stale_media_session"
    );
    ok(&mut service, Command::OpenCurrent);
    assert_eq!(ok(&mut service, Command::MediaState), Value::Null);
    assert_eq!(counts.lock().unwrap().closes, 1);
    let token2 = ok(&mut service, Command::OpenMedia { id, revision })["token"]
        .as_u64()
        .unwrap();
    std::fs::write(event["path"].as_str().unwrap(), [9, 8, 7, 6]).unwrap();
    assert_eq!(
        call(
            &mut service,
            Command::MediaFrame {
                session: token2,
                after_serial: 0
            }
        )["error"]["code"],
        "source_changed"
    );
    assert_eq!(counts.lock().unwrap().closes, 2);
}
#[test]
fn readonly_client_rejects_media_commands() {
    let temp = tempfile::tempdir().unwrap();
    drop(Service::open(temp.path(), 100000).unwrap());
    let mut service = Service::open_read_only(temp.path()).unwrap();
    assert_eq!(
        call(&mut service, Command::MediaState)["error"]["code"],
        "read_only"
    );
}
#[test]
fn background_open_cannot_resume_after_leaving_and_returning() {
    use std::time::{Duration, Instant};
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::open(temp.path(), 100000).unwrap();
    let counts = Arc::new(Mutex::new(Counts::default()));
    service
        .set_media_backend(Box::new(Fake(counts.clone())))
        .unwrap();
    let event = ok(
        &mut service,
        Command::IngestBytes {
            name: "async.mp4".into(),
            bytes: vec![1, 2, 3],
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    let revision = event["revision"].as_str().unwrap().to_owned();
    ok(&mut service, Command::SetVisible { ids: vec![id] });
    ok(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    ok(
        &mut service,
        Command::SetView {
            view: View::Details,
        },
    );
    let job = ok(
        &mut service,
        Command::BeginJob {
            command: Box::new(Command::OpenMedia { id, revision }),
        },
    );
    let job_id = serde_json::from_value(job["job_id"].clone()).unwrap();
    ok(&mut service, Command::SetView { view: View::List });
    ok(
        &mut service,
        Command::SetView {
            view: View::Details,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = ok(&mut service, Command::PollJob { job_id });
        if result["state"] == "failed" {
            assert_eq!(result["error"]["code"], "stale_media_context");
            break;
        }
        assert_ne!(result["state"], "succeeded", "{result}");
        assert!(Instant::now() < deadline, "{result}");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(counts.lock().unwrap().opens, 0);
}
#[test]
fn metadata_edits_keep_session_but_visibility_and_settings_close_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::open(temp.path(), 100000).unwrap();
    let counts = Arc::new(Mutex::new(Counts::default()));
    service
        .set_media_backend(Box::new(Fake(counts.clone())))
        .unwrap();
    let event = ok(
        &mut service,
        Command::IngestBytes {
            name: "fixture.mp4".into(),
            bytes: vec![4, 5, 6],
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    let revision = event["revision"].as_str().unwrap().to_owned();
    ok(&mut service, Command::SetVisible { ids: vec![id] });
    ok(
        &mut service,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    ok(
        &mut service,
        Command::SetView {
            view: View::Details,
        },
    );
    let token = ok(
        &mut service,
        Command::OpenMedia {
            id,
            revision: revision.clone(),
        },
    )["token"]
        .as_u64()
        .unwrap();
    ok(&mut service, Command::TogglePins { ids: vec![id] });
    ok(
        &mut service,
        Command::AssignTagSlot {
            ids: vec![id],
            slot: Some(1),
        },
    );
    assert_eq!(ok(&mut service, Command::MediaState)["token"], token);
    ok(&mut service, Command::ShowSettings);
    assert_eq!(ok(&mut service, Command::MediaState), Value::Null);
    ok(&mut service, Command::CloseSettings);
    ok(&mut service, Command::OpenMedia { id, revision });
    ok(
        &mut service,
        Command::SetWindowVisibility { visible: false },
    );
    assert_eq!(ok(&mut service, Command::MediaState), Value::Null);
    assert_eq!(counts.lock().unwrap().closes, 2);
}

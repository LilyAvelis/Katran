use katran::{
    ApiError, Result,
    media_playback::{Action, Backend, Frame, Identity, PlaybackState, Player, Snapshot, Source},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Data {
    calls: Vec<String>,
    fail_open: bool,
    duration: Option<u64>,
    frame: Option<Frame>,
}
struct Fake(Arc<Mutex<Data>>);
fn error(code: &str) -> ApiError {
    ApiError {
        code: code.into(),
        message: "synthetic".into(),
    }
}
impl Backend for Fake {
    fn open(&mut self, s: Source) -> Result<()> {
        let mut d = self.0.lock().unwrap();
        d.calls.push(format!("open:{}", s.path.display()));
        if d.fail_open {
            Err(error("synthetic_open_failure"))
        } else {
            Ok(())
        }
    }
    fn control(&mut self, a: Action) -> Result<()> {
        self.0.lock().unwrap().calls.push(format!("control:{a:?}"));
        Ok(())
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        let d = self.0.lock().unwrap();
        Ok(Snapshot {
            state: PlaybackState::Loading,
            duration_ms: d.duration,
            position_ms: 0,
            volume: 0.5,
            has_audio: true,
            width: 0,
            height: 0,
            error: None,
        })
    }
    fn frame(&mut self, _: u64) -> Result<Option<Frame>> {
        Ok(self.0.lock().unwrap().frame.clone())
    }
    fn close(&mut self) {
        self.0.lock().unwrap().calls.push("close".into());
    }
}
fn identity(id: i64) -> Identity {
    Identity {
        id,
        revision: format!("rev-{id}"),
        context: "images:details".into(),
    }
}
fn source(path: &str) -> Source {
    Source {
        path: PathBuf::from(path),
    }
}
#[test]
fn replacement_closes_previous_before_attempt_and_failure_never_resurrects_it() {
    let data = Arc::new(Mutex::new(Data::default()));
    let mut player = Player::new(Box::new(Fake(data.clone())));
    let old = player.open(identity(1), source("a")).unwrap().token;
    data.lock().unwrap().fail_open = true;
    assert_eq!(
        player.open(identity(2), source("b")).unwrap_err().code,
        "synthetic_open_failure"
    );
    assert!(player.state().unwrap().is_none());
    assert_eq!(
        player.control(old, Action::Play).unwrap_err().code,
        "stale_media_session"
    );
    assert_eq!(
        data.lock().unwrap().calls,
        vec!["open:a", "close", "open:b", "close"]
    );
    drop(player);
    assert_eq!(data.lock().unwrap().calls.len(), 4);
}
#[test]
fn stale_controls_frames_and_replaced_players_cannot_reuse_tokens() {
    let data = Arc::new(Mutex::new(Data::default()));
    let mut player = Player::new(Box::new(Fake(data.clone())));
    let old = player.open(identity(1), source("a")).unwrap().token;
    let current = player.open(identity(2), source("b")).unwrap().token;
    assert_ne!(old, current);
    assert_eq!(
        player.control(old, Action::Pause).unwrap_err().code,
        "stale_media_session"
    );
    assert_eq!(
        player.frame(old, 0).unwrap_err().code,
        "stale_media_session"
    );
    let mut other = Player::new(Box::new(Fake(data)));
    let newer = other.open(identity(3), source("c")).unwrap().token;
    assert_ne!(old, newer);
    assert_ne!(current, newer);
}
#[test]
fn loading_stop_volume_and_seek_bounds_have_explicit_semantics() {
    let data = Arc::new(Mutex::new(Data::default()));
    let mut player = Player::new(Box::new(Fake(data.clone())));
    let token = player.open(identity(1), source("a")).unwrap().token;
    assert_eq!(
        player
            .control(token, Action::Seek { position_ms: 0 })
            .unwrap_err()
            .code,
        "media_seek_unavailable"
    );
    for volume in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        assert_eq!(
            player
                .control(token, Action::SetVolume { volume })
                .unwrap_err()
                .code,
            "invalid_media_volume"
        );
    }
    data.lock().unwrap().duration = Some(1000);
    assert_eq!(
        player
            .control(token, Action::Seek { position_ms: 1001 })
            .unwrap_err()
            .code,
        "invalid_media_seek"
    );
    assert!(
        player
            .control(token, Action::Seek { position_ms: 1000 })
            .unwrap()
            .is_some()
    );
    assert!(
        player
            .control(token, Action::SetVolume { volume: 0.0 })
            .unwrap()
            .is_some()
    );
    assert!(player.control(token, Action::Stop).unwrap().is_none());
    assert_eq!(
        player.control(token, Action::Stop).unwrap_err().code,
        "stale_media_session"
    );
    drop(player);
    assert_eq!(
        data.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|call| *call == "close")
            .count(),
        1
    );
}
#[test]
fn frames_must_be_new_bounded_and_sized_before_exposure() {
    let data = Arc::new(Mutex::new(Data::default()));
    let mut player = Player::new(Box::new(Fake(data.clone())));
    let token = player.open(identity(1), source("a")).unwrap().token;
    for frame in [
        Frame {
            serial: 2,
            width: 1,
            height: 1,
            rgba: vec![0; 3],
        },
        Frame {
            serial: 2,
            width: 1281,
            height: 1,
            rgba: vec![0; 5124],
        },
        Frame {
            serial: 1,
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        },
    ] {
        data.lock().unwrap().frame = Some(frame);
        assert_eq!(
            player.frame(token, 1).unwrap_err().code,
            "invalid_media_frame"
        );
    }
    data.lock().unwrap().frame = Some(Frame {
        serial: 2,
        width: 1,
        height: 1,
        rgba: vec![0; 4],
    });
    assert_eq!(player.frame(token, 1).unwrap().unwrap().serial, 2);
}

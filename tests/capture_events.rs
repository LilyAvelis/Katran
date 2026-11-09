use katran::{
    ApiError, Command, Request, Service,
    capture_events::{Clock, RETRY_DELAYS_MS, Source, Trigger},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

struct FakeClock(Arc<AtomicU64>);
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
}
type Reply = katran::Result<Value>;
struct FakeSource {
    replies: VecDeque<Reply>,
    calls: Arc<AtomicU64>,
}
impl Source for FakeSource {
    fn capture(&mut self, _: &mut Service) -> Reply {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.replies
            .pop_front()
            .expect("unexpected clipboard attempt")
    }
}
fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "synthetic-events".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn setup(replies: Vec<Reply>) -> (tempfile::TempDir, Service, Arc<AtomicU64>, Arc<AtomicU64>) {
    let directory = tempfile::tempdir().unwrap();
    let mut api = Service::open(directory.path(), 1024).unwrap();
    let clock = Arc::new(AtomicU64::new(0));
    let calls = Arc::new(AtomicU64::new(0));
    api.set_capture_clock(Box::new(FakeClock(clock.clone())));
    api.set_capture_source(Box::new(FakeSource {
        replies: replies.into(),
        calls: calls.clone(),
    }));
    call(&mut api, Command::SetCaptureEnabled { enabled: true });
    (directory, api, clock, calls)
}
fn changed(api: &mut Service) -> Value {
    call(
        api,
        Command::CaptureClipboard {
            trigger: Trigger::Changed,
        },
    )
}
fn retry(api: &mut Service, ticket: u64) -> Value {
    call(
        api,
        Command::CaptureClipboard {
            trigger: Trigger::Retry { ticket },
        },
    )
}

#[test]
fn short_retry_waits_then_success_stops_all_future_attempts() {
    let (_dir, mut api, clock, calls) = setup(vec![
        Err(ApiError {
            code: "clipboard_busy".into(),
            message: "synthetic lock".into(),
        }),
        Ok(json!({"captured":1})),
    ]);
    let first = changed(&mut api);
    assert_eq!(first["read_ok"], false);
    assert_eq!(first["capture_error"]["code"], "clipboard_busy");
    assert_eq!(first["capture_schedule"]["retry_after_ms"], 25);
    let ticket = first["capture_schedule"]["ticket"].as_u64().unwrap();
    assert_eq!(retry(&mut api, ticket)["state"], "waiting");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    clock.store(25, Ordering::SeqCst);
    let success = retry(&mut api, ticket);
    assert_eq!(success["captured"], 1);
    assert!(success["capture_schedule"]["retry_after_ms"].is_null());
    assert_eq!(retry(&mut api, ticket)["state"], "superseded");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn exhausted_budget_is_explicit_and_new_notification_recovers() {
    let mut replies = (0..8)
        .map(|_| Ok(json!({"captured":0,"state":"retry"})))
        .collect::<Vec<_>>();
    replies.push(Ok(json!({"captured":0,"state":"unsupported_format"})));
    let (_dir, mut api, clock, calls) = setup(replies);
    let mut response = changed(&mut api);
    let ticket = response["capture_schedule"]["ticket"].as_u64().unwrap();
    for delay in RETRY_DELAYS_MS {
        assert_eq!(response["capture_schedule"]["retry_after_ms"], delay);
        clock.fetch_add(delay, Ordering::SeqCst);
        response = retry(&mut api, ticket);
    }
    assert_eq!(response["capture_schedule"]["exhausted"], true);
    assert!(response["capture_schedule"]["retry_after_ms"].is_null());
    assert_eq!(retry(&mut api, ticket)["state"], "superseded");
    assert_eq!(calls.load(Ordering::SeqCst), 8);
    assert_eq!(changed(&mut api)["state"], "unsupported_format");
    assert_eq!(calls.load(Ordering::SeqCst), 9);
}

#[test]
fn stale_retries_pause_and_reenable_do_not_read_old_pending_change() {
    let (_dir, mut api, clock, calls) = setup(vec![
        Err(ApiError {
            code: "clipboard_error".into(),
            message: "busy image".into(),
        }),
        Ok(json!({"captured":0})),
    ]);
    let old = changed(&mut api)["capture_schedule"]["ticket"]
        .as_u64()
        .unwrap();
    assert_eq!(changed(&mut api)["read_ok"], true); // latest change supersedes old retry
    clock.store(1000, Ordering::SeqCst);
    assert_eq!(retry(&mut api, old)["state"], "superseded");
    call(&mut api, Command::SetCaptureEnabled { enabled: false });
    assert_eq!(changed(&mut api)["state"], "paused");
    call(&mut api, Command::SetCaptureEnabled { enabled: true });
    assert_eq!(retry(&mut api, old)["state"], "superseded");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn partial_item_results_and_permanent_errors_are_not_retried() {
    let (_dir, mut api, _, calls) = setup(vec![
        Ok(json!({"captured":1,"items":[{"ok":true},{"ok":false,"error":"missing"}]})),
        Err(ApiError {
            code: "image_error".into(),
            message: "synthetic invalid pixels".into(),
        }),
    ]);
    let batch = changed(&mut api);
    assert_eq!(batch["items"][1]["ok"], false);
    assert!(batch["capture_schedule"]["retry_after_ms"].is_null());
    let failure = changed(&mut api);
    assert_eq!(failure["read_ok"], false);
    assert!(failure["capture_schedule"]["retry_after_ms"].is_null());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

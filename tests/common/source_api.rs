//! Synthetic tests drive the same asynchronous source lifecycle as every client.
#![allow(dead_code)]
use katran::{Command, Request, Service, source_policy::SourceId};
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "synthetic-source".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
pub fn poll_until(
    api: &mut Service,
    description: &str,
    mut done: impl FnMut(&mut Service, &Value) -> bool,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let batch = call(api, Command::PollSources { limit: 8 });
        if done(api, &batch) {
            return batch;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for {description}: {batch}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub fn wait_event(api: &mut Service, source_id: SourceId, path: &Path) -> Value {
    call(api, Command::RescanSource { source_id });
    let expected = path.canonicalize().unwrap();
    let mut event = None;
    poll_until(api, "trusted source event", |_, batch| {
        assert!(
            batch["source_errors"].as_array().unwrap().is_empty(),
            "{batch}"
        );
        for item in batch["items"].as_array().unwrap() {
            assert_eq!(item["ok"], true, "{batch}");
            if item["event"]["path"]
                .as_str()
                .is_some_and(|p| Path::new(p) == expected)
            {
                event = Some(item["event"].clone());
            }
        }
        event.is_some()
    });
    let event = event.unwrap();
    assert_eq!(event["delete_policy"]["kind"], "live_original");
    event
}
/// Let real workers finish discovery and metadata stability checks. A quiet
/// observation interval is required because backlog zero alone can precede a
/// worker's second stability observation. Returns every commit/failure item.
pub fn settle(api: &mut Service, source_id: SourceId) -> Vec<Value> {
    call(api, Command::RescanSource { source_id });
    let started = Instant::now();
    let mut quiet = Instant::now();
    let mut items = Vec::new();
    poll_until(api, "source discovery to settle", |_, batch| {
        let batch_items = batch["items"].as_array().unwrap();
        if !batch_items.is_empty() || batch["refresh"] == true {
            quiet = Instant::now();
        }
        items.extend(batch_items.iter().cloned());
        let sources = batch["sources"].as_array().unwrap();
        let healthy = sources
            .iter()
            .filter(|s| s["enabled"] == true)
            .all(|s| s["state"] == "watching" && s["backlog"] == 0);
        started.elapsed() >= Duration::from_millis(200)
            && quiet.elapsed() >= Duration::from_millis(100)
            && healthy
    });
    items
}

pub fn observe_for(api: &mut Service, duration: Duration) -> Vec<Value> {
    let deadline = Instant::now() + duration;
    let mut batches = Vec::new();
    while Instant::now() < deadline {
        batches.push(call(api, Command::PollSources { limit: 8 }));
        std::thread::sleep(Duration::from_millis(5));
    }
    batches
}

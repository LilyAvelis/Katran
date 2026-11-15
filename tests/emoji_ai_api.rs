use katran::{Command, Request, Service, emoji_ai::Backend, interaction::ShelfTab};
use serde_json::{Value, json};

struct Fake;
impl Backend for Fake {
    fn search(&mut self, _: &str, offset: u32, limit: u32) -> katran::Result<Value> {
        let all = ["🐄", "☕", "🐮"];
        assert_eq!(limit, 2);
        let start = offset as usize;
        Ok(
            json!({"offset":offset,"next_offset":offset+limit,"results":all[start..start+limit as usize].iter().enumerate().map(|(i, emoji)|json!({"emoji":emoji,"rank":start+i+1,"score":0.9-i as f64*0.1})).collect::<Vec<_>>()}),
        )
    }
}
struct Malformed(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Backend for Malformed {
    fn search(&mut self, _: &str, _: u32, _: u32) -> katran::Result<Value> {
        Ok(
            json!({"offset":0,"next_offset":1,"results":[{"emoji":"not-an-emoji","rank":1,"score":0.9}]}),
        )
    }
    fn invalidate(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
fn invoke(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "synthetic".into(),
        command,
    })
}
fn success(api: &mut Service, command: Command) -> Value {
    let result = invoke(api, command);
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
}
fn query(query: &str, offset: u32, limit: u32) -> Command {
    Command::SearchEmojiAi {
        query: query.into(),
        offset,
        limit,
    }
}

#[test]
fn ai_search_is_catalogue_only_global_stable_and_validated() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    api.set_emoji_ai_backend(Box::new(Fake));
    assert_eq!(
        invoke(&mut api, query("корова", 0, 2))["error"]["code"],
        "invalid_tab"
    );
    success(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    success(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    assert_eq!(
        invoke(&mut api, query("корова", 0, 2))["error"]["code"],
        "invalid_tab"
    );
    success(&mut api, Command::SetEmojiCatalogue { enabled: true });
    success(
        &mut api,
        Command::SetEmojiCategory {
            group: Some("Smileys & Emotion".into()),
        },
    );
    for invalid in [
        query(" ", 0, 2),
        query("x", 3781, 2),
        query("x", 0, 0),
        query(&"x".repeat(513), 0, 2),
    ] {
        assert_eq!(
            invoke(&mut api, invalid)["error"]["code"],
            "invalid_argument"
        );
    }
    let first = success(&mut api, query("корова", 0, 2));
    assert_eq!(first["next_offset"], 2);
    assert_eq!(first["events"][0]["text_preview"], "🐄");
    assert_eq!(first["events"][1]["text_preview"], "☕");
    assert_eq!(first["events"][0]["rank"], 1);
    assert_eq!(first["events"][0]["score"], 0.9);
    let id = first["events"][0]["id"].as_i64().unwrap();
    let item = success(&mut api, Command::Get { id });
    assert_eq!(item["text_preview"], "🐄", "{item}");
    let second = success(&mut api, query("корова", 1, 2));
    assert_eq!(first["events"][1]["id"], second["events"][0]["id"]);
    assert_eq!(second["events"][0]["rank"], 2);
    let invalidated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    api.set_emoji_ai_backend(Box::new(Malformed(invalidated.clone())));
    assert_eq!(
        invoke(&mut api, query("secret", 0, 1))["error"]["code"],
        "emoji_ai_protocol"
    );
    assert!(invalidated.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
#[ignore = "requires local experimental Python environment, model and CLDR index"]
fn real_resident_worker_smoke() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    success(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    success(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    success(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let first = success(&mut api, query("корова", 0, 10));
    assert_eq!(first["events"][0]["text_preview"], "🐄");
    let second = success(&mut api, query("корова", 10, 10));
    assert_eq!(second["events"][0]["rank"], 11);
    assert_ne!(first["events"][0]["id"], second["events"][0]["id"]);
}

struct GatedBackend {
    started: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
impl Backend for GatedBackend {
    fn search(&mut self, _: &str, _: u32, _: u32) -> katran::Result<Value> {
        self.started.send(()).unwrap();
        self.release
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        Ok(json!({"offset":0,"next_offset":1,"results":[{"emoji":"🐄","rank":1,"score":0.9}]}))
    }
}

#[test]
fn background_search_is_responsive_cancellable_and_context_checked() {
    use katran::domain_jobs::JobId;
    use std::time::{Duration, Instant};
    fn begin(api: &mut Service) -> JobId {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let response = invoke(
                api,
                Command::BeginJob {
                    command: Box::new(query("корова", 0, 1)),
                },
            );
            if response["ok"] == true {
                return serde_json::from_value(response["result"]["job_id"].clone()).unwrap();
            }
            assert_eq!(response["error"]["code"], "job_capacity");
            assert!(Instant::now() < deadline, "{response}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn terminal(api: &mut Service, job_id: JobId) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = success(api, Command::PollJob { job_id });
            if matches!(
                status["state"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                return status;
            }
            assert!(Instant::now() < deadline, "{status}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn gated(api: &mut Service) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        api.set_emoji_ai_backend(Box::new(GatedBackend {
            started: started_tx,
            release: release_rx,
        }));
        (started_rx, release_tx)
    }
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let invalid = invoke(
        &mut api,
        Command::BeginJob {
            command: Box::new(query("x", 0, 1)),
        },
    );
    assert_eq!(invalid["error"]["code"], "invalid_tab");
    success(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    success(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    success(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let invalid = invoke(
        &mut api,
        Command::BeginJob {
            command: Box::new(query(" ", 0, 1)),
        },
    );
    assert_eq!(invalid["error"]["code"], "invalid_argument");

    let (started, release) = gated(&mut api);
    let job = begin(&mut api);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    // Even a backend that cannot be interrupted cannot occupy Discovery.
    let discovery: JobId = serde_json::from_value(
        success(
            &mut api,
            Command::BeginJob {
                command: Box::new(Command::ListDesktopAssets),
            },
        )["job_id"]
            .clone(),
    )
    .unwrap();
    assert_eq!(terminal(&mut api, discovery)["state"], "succeeded");
    let full = invoke(
        &mut api,
        Command::BeginJob {
            command: Box::new(query("other", 0, 1)),
        },
    );
    assert_eq!(full["error"]["code"], "job_capacity");
    let instant = Instant::now();
    success(&mut api, Command::Interaction);
    let elapsed = instant.elapsed();
    release.send(()).unwrap();
    assert!(elapsed < Duration::from_millis(500));
    let status = terminal(&mut api, job);
    assert_eq!(status["state"], "succeeded", "{status}");
    assert_eq!(status["result"]["events"][0]["text_preview"], "🐄");

    let (started, release) = gated(&mut api);
    let job = begin(&mut api);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let cancelled = success(&mut api, Command::CancelJob { job_id: job });
    assert_eq!(cancelled["state"], "cancelled");
    // A non-cooperative injected backend holds its one bounded slot until it returns.
    let full = invoke(
        &mut api,
        Command::BeginJob {
            command: Box::new(query("other", 0, 1)),
        },
    );
    assert_eq!(full["error"]["code"], "job_capacity");
    release.send(()).unwrap();
    assert_eq!(terminal(&mut api, job)["state"], "cancelled");

    let (started, release) = gated(&mut api);
    let job = begin(&mut api);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    success(&mut api, Command::SetEmojiCatalogue { enabled: false });
    success(&mut api, Command::SetEmojiCatalogue { enabled: true });
    release.send(()).unwrap();
    let status = terminal(&mut api, job);
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(status["error"]["code"], "stale_context");
}

struct CancellableBackend(std::sync::mpsc::Sender<()>);
impl Backend for CancellableBackend {
    fn search(&mut self, _: &str, _: u32, _: u32) -> katran::Result<Value> {
        unreachable!("background search must use cancellation-aware entry point")
    }
    fn search_cancellable(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        cancel: &katran::domain_jobs::Cancellation,
    ) -> katran::Result<Value> {
        self.0.send(()).unwrap();
        loop {
            cancel.check()?;
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

#[test]
fn cancellation_interrupts_cooperative_backend_and_releases_ai_lane() {
    use katran::domain_jobs::JobId;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    success(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    success(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    success(&mut api, Command::SetEmojiCatalogue { enabled: true });
    let (started, observed) = std::sync::mpsc::channel();
    api.set_emoji_ai_backend(Box::new(CancellableBackend(started)));
    let begin = |api: &mut Service| -> Value {
        invoke(
            api,
            Command::BeginJob {
                command: Box::new(query("cow", 0, 1)),
            },
        )
    };
    let first: JobId = serde_json::from_value(begin(&mut api)["result"]["job_id"].clone()).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        success(&mut api, Command::CancelJob { job_id: first })["state"],
        "cancelled"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let second = loop {
        let response = begin(&mut api);
        if response["ok"] == true {
            break response;
        }
        assert_eq!(response["error"]["code"], "job_capacity");
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let second: JobId = serde_json::from_value(second["result"]["job_id"].clone()).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        success(&mut api, Command::CancelJob { job_id: second })["state"],
        "cancelled"
    );
}

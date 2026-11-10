use katran::{Command, Request, SelectionRef, Service, domain_jobs::JobId, interaction::ShelfTab};
use serde_json::Value;
use std::{
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

fn execute(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "synthetic-job".into(),
        command,
    })
}
fn result(api: &mut Service, command: Command) -> Value {
    let response = execute(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn begin(api: &mut Service, command: Command) -> JobId {
    let response = result(
        api,
        Command::BeginJob {
            command: Box::new(command),
        },
    );
    serde_json::from_value(response["job_id"].clone()).unwrap()
}
fn terminal(api: &mut Service, job_id: JobId) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = result(api, Command::PollJob { job_id });
        if matches!(
            status["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            return status;
        }
        assert!(Instant::now() < deadline, "Job did not complete: {status}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn fixture() -> (tempfile::TempDir, Service, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.pdf");
    std::fs::write(&path, b"synthetic original document").unwrap();
    let api = Service::open(&root.path().join("shelf"), 1024 * 1024).unwrap();
    (root, api, path)
}

struct BlockedLauncher {
    started: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl katran::quicklook::Launcher for BlockedLauncher {
    fn launch(&self, _: &Path) -> katran::Result<()> {
        self.started.send(()).unwrap();
        self.release
            .recv_timeout(Duration::from_secs(5))
            .expect("test must release fake preview");
        Ok(())
    }
}

#[test]
fn blocked_preview_keeps_interaction_responsive_and_reports_irreversible_effect() {
    let (_root, mut api, path) = fixture();
    let event = result(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "test".into(),
        },
    );
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    api.set_preview_launcher(Box::new(BlockedLauncher {
        started: started_tx,
        release: release_rx,
    }));
    let job_id = begin(
        &mut api,
        Command::PreviewExternal {
            id: event["id"].as_i64().unwrap(),
        },
    );
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    result(&mut api, Command::Interaction);
    result(&mut api, Command::SetTab { tab: ShelfTab::All });
    result(&mut api, Command::Status);
    let elapsed = started.elapsed();
    let cancellation = execute(&mut api, Command::CancelJob { job_id });
    // Always release the worker before assertions, including regression failures.
    release_tx.send(()).unwrap();
    assert!(
        elapsed < Duration::from_millis(500),
        "Dispatcher blocked for {elapsed:?}"
    );
    assert_eq!(
        cancellation["error"]["code"], "job_not_cancellable",
        "{cancellation}"
    );
    let status = terminal(&mut api, job_id);
    assert_eq!(status["state"], "succeeded", "{status}");
    assert_eq!(status["result"]["backend"], "quicklook");
    assert_eq!(
        status["result"]["receipt"]["acknowledgement"],
        "launcher_returned"
    );
    assert_eq!(std::fs::read(path).unwrap(), b"synthetic original document");
}

#[test]
fn asynchronous_ingestion_commits_one_shelf_only_reference() {
    let (_root, mut api, path) = fixture();
    let job_id = begin(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "test".into(),
        },
    );
    let status = terminal(&mut api, job_id);
    assert_eq!(status["state"], "succeeded", "{status}");
    let event = &status["result"];
    assert_eq!(event["storage"], "reference");
    assert_eq!(event["delete_policy"]["kind"], "shelf_only");
    let id = event["id"].as_i64().unwrap();
    assert_eq!(
        result(&mut api, Command::Get { id })["revision"],
        event["revision"]
    );
    assert_eq!(
        result(&mut api, Command::PollJob { job_id })["result"]["id"],
        id
    );
    assert_eq!(
        result(&mut api, Command::List { limit: 10 })["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(std::fs::read(path).unwrap(), b"synthetic original document");
}

#[test]
fn asynchronous_drag_rejects_deleted_selection_before_creating_native_handoff() {
    let (_root, mut api, path) = fixture();
    let event = result(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    let job_id = begin(
        &mut api,
        Command::DragEventsChecked {
            items: vec![SelectionRef {
                id,
                revision: event["revision"].as_str().unwrap().into(),
            }],
        },
    );
    result(
        &mut api,
        Command::DeleteEvents {
            ids: vec![id],
            confirmed: false,
        },
    );
    let status = terminal(&mut api, job_id);
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(status["error"]["code"], "stale_selection", "{status}");
    assert!(status["result"].is_null());
    assert_eq!(std::fs::read(path).unwrap(), b"synthetic original document");
}

fn bytes_job(api: &mut Service, name: &str, bytes: &[u8]) -> JobId {
    begin(
        api,
        Command::IngestBytes {
            name: name.into(),
            bytes: bytes.to_vec(),
            source: "test".into(),
        },
    )
}

#[test]
fn asynchronous_owned_bytes_are_cached_and_duplicates_do_not_consume_more_quota() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"synthetic owned byte payload";
    let mut api = Service::open(&root.path().join("shelf"), bytes.len() as u64).unwrap();
    let first_job = bytes_job(&mut api, "first.bin", bytes);
    let first = terminal(&mut api, first_job);
    assert_eq!(first["state"], "succeeded", "{first}");
    assert_eq!(first["result"]["storage"], "cached");
    assert_eq!(first["result"]["delete_policy"]["kind"], "shelf_only");
    assert_eq!(
        std::fs::read(first["result"]["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    let second_job = bytes_job(&mut api, "duplicate.bin", bytes);
    let second = terminal(&mut api, second_job);
    assert_eq!(second["state"], "succeeded", "{second}");
    assert_ne!(first["result"]["id"], second["result"]["id"]);
    assert_eq!(first["result"]["path"], second["result"]["path"]);
    assert_eq!(
        result(&mut api, Command::Status)["cached_bytes"],
        bytes.len()
    );
    assert_eq!(
        result(&mut api, Command::List { limit: 10 })["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let overflow = bytes_job(&mut api, "over-quota.bin", b"different unique contents");
    let overflow = terminal(&mut api, overflow);
    assert_eq!(overflow["state"], "failed", "{overflow}");
    assert_eq!(overflow["error"]["code"], "quota_exceeded", "{overflow}");
    assert_eq!(
        result(&mut api, Command::Status)["cached_bytes"],
        bytes.len()
    );
}

#[test]
fn asynchronous_duplicate_bytes_refuse_corrupt_existing_blob_without_replacing_it() {
    let (_root, mut api, original) = fixture();
    let bytes = b"synthetic cached integrity fixture";
    let first_job = bytes_job(&mut api, "first.bin", bytes);
    let first = terminal(&mut api, first_job);
    assert_eq!(first["state"], "succeeded", "{first}");
    let cached_path = first["result"]["path"].as_str().unwrap();
    std::fs::write(cached_path, b"synthetic corruption").unwrap();
    let duplicate = bytes_job(&mut api, "second.bin", bytes);
    let failed = terminal(&mut api, duplicate);
    assert_eq!(failed["state"], "failed", "{failed}");
    assert_eq!(failed["error"]["code"], "integrity_error", "{failed}");
    assert_eq!(
        result(&mut api, Command::List { limit: 10 })["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(std::fs::read(cached_path).unwrap(), b"synthetic corruption");
    assert_eq!(
        std::fs::read(original).unwrap(),
        b"synthetic original document"
    );
}

#[test]
fn cancelled_queued_owned_bytes_never_commit_an_event_or_cache_usage() {
    let (_root, mut api, original) = fixture();
    let event = result(
        &mut api,
        Command::IngestPath {
            path: original.clone(),
            source: "test".into(),
        },
    );
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    api.set_preview_launcher(Box::new(BlockedLauncher {
        started: started_tx,
        release: release_rx,
    }));
    let preview = begin(
        &mut api,
        Command::PreviewExternal {
            id: event["id"].as_i64().unwrap(),
        },
    );
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let bytes = bytes_job(&mut api, "cancelled.bin", b"synthetic cancelled bytes");
    let cancellation = execute(&mut api, Command::CancelJob { job_id: bytes });
    release_tx.send(()).unwrap();
    assert_eq!(cancellation["ok"], true, "{cancellation}");
    assert_eq!(terminal(&mut api, preview)["state"], "succeeded");
    assert_eq!(terminal(&mut api, bytes)["state"], "cancelled");
    assert_eq!(result(&mut api, Command::Status)["cached_bytes"], 0);
    let events = result(&mut api, Command::List { limit: 10 });
    assert_eq!(events["events"].as_array().unwrap().len(), 1);
    assert_eq!(events["events"][0]["id"], event["id"]);
    assert_eq!(
        std::fs::read(original).unwrap(),
        b"synthetic original document"
    );
}

#[test]
fn identical_byte_jobs_prepared_before_either_commit_share_one_blob() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("barrier.pdf");
    std::fs::write(&original, b"synthetic preview barrier").unwrap();
    let bytes = b"synthetic concurrent deduplicated cache";
    let mut api = Service::open(&root.path().join("shelf"), bytes.len() as u64).unwrap();
    let barrier_event = result(
        &mut api,
        Command::IngestPath {
            path: original.clone(),
            source: "test".into(),
        },
    );
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    api.set_preview_launcher(Box::new(BlockedLauncher {
        started: started_tx,
        release: release_rx,
    }));
    let first = bytes_job(&mut api, "first.bin", bytes);
    let second = bytes_job(&mut api, "second.bin", bytes);
    let barrier = begin(
        &mut api,
        Command::PreviewExternal {
            id: barrier_event["id"].as_i64().unwrap(),
        },
    );
    // The foreground worker is FIFO: reaching this launcher proves both byte
    // preparations completed without a PollJob committing either result.
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    release_tx.send(()).unwrap();
    let first = terminal(&mut api, first);
    let second = terminal(&mut api, second);
    assert_eq!(first["state"], "succeeded", "{first}");
    assert_eq!(second["state"], "succeeded", "{second}");
    assert_ne!(first["result"]["id"], second["result"]["id"]);
    assert_eq!(first["result"]["path"], second["result"]["path"]);
    assert_eq!(terminal(&mut api, barrier)["state"], "succeeded");
    assert_eq!(
        result(&mut api, Command::Status)["cached_bytes"],
        bytes.len()
    );
    assert_eq!(
        result(&mut api, Command::List { limit: 10 })["events"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        std::fs::read(first["result"]["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    assert_eq!(
        std::fs::read(original).unwrap(),
        b"synthetic preview barrier"
    );
}

#[test]
fn queued_export_rejects_changed_arrival_and_removes_partial_output() {
    let (root, mut api, path) = fixture();
    let event = result(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "test".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    api.set_preview_launcher(Box::new(BlockedLauncher {
        started: started_tx,
        release: release_rx,
    }));
    let preview = begin(&mut api, Command::PreviewExternal { id });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let destination = root.path().join("exports");
    std::fs::create_dir(&destination).unwrap();
    let job = begin(
        &mut api,
        Command::Import {
            ids: vec![id],
            destination: destination.clone(),
        },
    );
    std::fs::write(&path, b"different current bytes").unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(terminal(&mut api, preview)["state"], "succeeded");
    let status = terminal(&mut api, job);
    assert_eq!(status["state"], "succeeded", "{status}");
    assert_eq!(status["result"]["items"][0]["ok"], false);
    assert_eq!(
        status["result"]["items"][0]["error"]["code"],
        "source_changed"
    );
    assert_eq!(std::fs::read_dir(destination).unwrap().count(), 0);
}

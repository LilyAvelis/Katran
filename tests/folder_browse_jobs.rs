use katran::{Command, Request, Service, domain_jobs::JobId, interaction::ShelfTab};
use serde_json::Value;
use std::time::{Duration, Instant};

fn call(service: &mut Service, command: Command) -> Value {
    let reply = service.execute(Request {
        api_version: 1,
        request_id: "folder-job".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
fn begin(service: &mut Service, cursor: Option<String>) -> JobId {
    let result = call(
        service,
        Command::BeginJob {
            command: Box::new(Command::BrowseShelfSorted {
                query: String::new(),
                limit: 1,
                cursor,
            }),
        },
    );
    assert_eq!(result["state"], "queued");
    serde_json::from_value(result["job_id"].clone()).unwrap()
}
fn finish(service: &mut Service, id: JobId) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = call(service, Command::PollJob { job_id: id });
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

#[test]
fn worker_pages_preserve_ids_and_reject_changed_files() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("a"), b"a").unwrap();
    std::fs::write(folder.path().join("b"), b"b").unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let job = begin(&mut service, None);
    let first = finish(&mut service, job);
    assert_eq!(first["state"], "succeeded", "{first}");
    let id = first["result"]["events"][0]["id"].as_i64().unwrap();
    assert_eq!(call(&mut service, Command::Get { id })["id"], id);
    let cursor = first["result"]["next_cursor"].as_str().unwrap().to_owned();
    std::fs::write(folder.path().join("a"), b"changed length").unwrap();
    let job = begin(&mut service, Some(cursor));
    let stale = finish(&mut service, job);
    assert_eq!(stale["state"], "failed", "{stale}");
    assert_eq!(stale["error"]["code"], "stale_cursor");
}

#[test]
fn worker_does_not_publish_after_context_or_newer_request() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("a"), b"a").unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    call(
        &mut service,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let older = begin(&mut service, None);
    let newer = begin(&mut service, None);
    let status = finish(&mut service, older);
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(status["error"]["code"], "stale_folder");
    let status = finish(&mut service, newer);
    assert_eq!(status["state"], "succeeded", "{status}");
    let job = begin(&mut service, None);
    call(
        &mut service,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    let status = finish(&mut service, job);
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(status["error"]["code"], "stale_folder");
}

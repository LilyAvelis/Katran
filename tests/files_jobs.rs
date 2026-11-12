use katran::{Command, Request, Service, domain_jobs::JobId};
use serde_json::Value;
use std::time::{Duration, Instant};

fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "files-job".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

#[test]
fn async_add_files_reports_each_input_and_commits_only_valid_pointers() {
    let data = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let good = originals.path().join("good.bin");
    let missing = originals.path().join("missing.bin");
    std::fs::write(&good, b"synthetic").unwrap();
    let mut api = Service::open(data.path(), 100_000).unwrap();
    let submitted = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::AddFiles {
                paths: vec![good.clone(), missing],
            }),
        },
    );
    let job_id: JobId = serde_json::from_value(submitted["job_id"].clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let terminal = loop {
        let state = call(&mut api, Command::PollJob { job_id });
        if matches!(
            state["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            break state;
        }
        assert!(Instant::now() < deadline, "Timed out: {state}");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(terminal["state"], "succeeded", "{terminal}");
    assert_eq!(terminal["result"]["items"][0]["ok"], true);
    assert_eq!(terminal["result"]["items"][1]["ok"], false);
    let id = terminal["result"]["items"][0]["event"]["id"]
        .as_i64()
        .unwrap();
    assert_eq!(std::fs::read(&good).unwrap(), b"synthetic");
    drop(api);
    let mut api = Service::open(data.path(), 100_000).unwrap();
    assert_eq!(call(&mut api, Command::Get { id })["source"], "files");
}

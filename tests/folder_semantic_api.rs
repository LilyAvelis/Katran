use katran::{
    ApiError, Command, Request, Result, Service,
    domain_jobs::{Cancellation, JobId},
    folder_semantic::{Backend, Snapshot, Source},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
struct FakeSource;
impl Source for FakeSource {
    fn snapshot(&mut self, _: &str, _: &Cancellation) -> Result<Snapshot> {
        Ok(Snapshot {
            indexed: vec![PathBuf::from("C:/Synthetic/cooking")],
            // recipes is an ordinary-search candidate absent from the broad index
            // and is not an exact basename match for the query "food".
            exact_candidates: vec![
                PathBuf::from("C:/Synthetic/food"),
                PathBuf::from("C:/Synthetic/recipes"),
            ],
            incomplete: true,
        })
    }
}
struct FakeBackend {
    mode: &'static str,
}
impl Backend for FakeBackend {
    fn search(
        &mut self,
        _: &str,
        _: &[PathBuf],
        _: u32,
        cancel: &Cancellation,
    ) -> Result<Vec<usize>> {
        match self.mode {
            "missing" => Err(ApiError {
                code: "model_unavailable".into(),
                message: "synthetic model missing".into(),
            }),
            "timeout" => Err(ApiError {
                code: "folder_ai_unavailable".into(),
                message: "synthetic timeout".into(),
            }),
            "bad" => Ok(vec![999]),
            "cancel" => loop {
                cancel.check()?;
                std::thread::sleep(Duration::from_millis(5));
            },
            _ => Ok(vec![1, 0]),
        }
    }
}
fn invoke(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "folder-semantic".into(),
        command,
    })
}
fn success(api: &mut Service, command: Command) -> Value {
    let v = invoke(api, command);
    assert_eq!(v["ok"], true, "{v}");
    v["result"].clone()
}
fn begin(api: &mut Service) -> JobId {
    serde_json::from_value(
        success(
            api,
            Command::BeginJob {
                command: Box::new(Command::SearchFoldersSemantic {
                    query: "food".into(),
                    limit: 10,
                }),
            },
        )["job_id"]
            .clone(),
    )
    .unwrap()
}
fn terminal(api: &mut Service, id: JobId) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = success(api, Command::PollJob { job_id: id });
        if matches!(
            status["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            return status;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn semantic_candidates_exact_priority_and_explicit_coverage() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    api.set_folder_semantic_backend(Box::new(FakeBackend { mode: "ok" }), Box::new(FakeSource));
    assert_eq!(
        invoke(
            &mut api,
            Command::SearchFoldersSemantic {
                query: "food".into(),
                limit: 10
            }
        )["error"]["code"],
        "background_required"
    );
    let id = begin(&mut api);
    let result = terminal(&mut api, id);
    assert_eq!(result["state"], "succeeded");
    assert!(
        result["result"]["items"][0]["path"]
            .as_str()
            .unwrap()
            .ends_with("food")
    );
    assert!(
        result["result"]["items"][1]["path"]
            .as_str()
            .unwrap()
            .ends_with("recipes")
    );
    assert_eq!(result["result"]["indexed_count"], 3);
    assert_eq!(result["result"]["incomplete"], true);
}
#[test]
fn model_errors_invalid_response_and_cancellation_are_not_lexical_success() {
    for mode in ["missing", "timeout", "bad", "cancel"] {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100_000).unwrap();
        api.set_folder_semantic_backend(Box::new(FakeBackend { mode }), Box::new(FakeSource));
        let id = begin(&mut api);
        if mode == "cancel" {
            success(&mut api, Command::CancelJob { job_id: id });
            assert_eq!(terminal(&mut api, id)["state"], "cancelled");
        } else {
            let status = terminal(&mut api, id);
            assert_eq!(status["state"], "failed", "{status}");
            assert_eq!(
                status["error"]["code"],
                if mode == "missing" {
                    "model_unavailable"
                } else {
                    "folder_ai_unavailable"
                }
            );
        }
    }
}

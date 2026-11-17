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
        _: bool,
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

struct PageSource(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Source for PageSource {
    fn snapshot(&mut self, _: &str, _: &Cancellation) -> Result<Snapshot> {
        let mut paths: Vec<_> = (0..25)
            .map(|i| PathBuf::from(format!("C:/Synthetic/entry-{i:02}")))
            .collect();
        if self.0.load(std::sync::atomic::Ordering::SeqCst) {
            paths.push(PathBuf::from("C:/Synthetic/added"));
        }
        Ok(Snapshot {
            indexed: paths,
            exact_candidates: vec![],
            incomplete: false,
        })
    }
}
struct PageBackend(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Backend for PageBackend {
    fn search(
        &mut self,
        _: &str,
        paths: &[PathBuf],
        limit: u32,
        gpu: bool,
        _: &Cancellation,
    ) -> Result<Vec<usize>> {
        self.0.store(gpu, std::sync::atomic::Ordering::SeqCst);
        Ok((0..paths.len().min(limit as usize)).collect())
    }
}
fn page(api: &mut Service, offset: u32, snapshot: Option<String>) -> Value {
    let id: JobId = serde_json::from_value(
        success(
            api,
            Command::BeginJob {
                command: Box::new(Command::SearchFoldersSemanticPage {
                    query: "synthetic".into(),
                    offset,
                    limit: 10,
                    snapshot,
                }),
            },
        )["job_id"]
            .clone(),
    )
    .unwrap();
    terminal(api, id)
}
#[test]
fn pages_are_disjoint_and_stale_snapshots_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let changed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let gpu_seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    api.set_folder_semantic_backend(
        Box::new(PageBackend(gpu_seen.clone())),
        Box::new(PageSource(changed.clone())),
    );
    let first = page(&mut api, 0, None);
    assert_eq!(first["state"], "succeeded");
    let first = &first["result"];
    assert_eq!(first["items"].as_array().unwrap().len(), 10);
    assert_eq!(first["next_offset"], 10);
    assert_eq!(first["has_more"], true);
    assert_eq!(first["query"], "synthetic");
    let token = first["snapshot"].as_str().unwrap().to_owned();
    let first_items = first["items"].clone();
    let second = page(&mut api, 10, Some(token.clone()));
    assert_eq!(second["state"], "succeeded");
    let second = &second["result"];
    assert_eq!(second["items"].as_array().unwrap().len(), 10);
    assert_eq!(second["next_offset"], 20);
    for item in second["items"].as_array().unwrap() {
        assert!(!first_items.as_array().unwrap().contains(item));
    }
    let third = page(&mut api, 20, Some(token.clone()));
    assert_eq!(third["result"]["items"].as_array().unwrap().len(), 5);
    assert_eq!(third["result"]["has_more"], false);
    changed.store(true, std::sync::atomic::Ordering::SeqCst);
    let stale = page(&mut api, 10, Some(token));
    assert_eq!(stale["state"], "failed");
    assert_eq!(stale["error"]["code"], "folder_ai_snapshot_changed");
    let old = success(&mut api, Command::GetSettings);
    assert!(old.get("folder_ai_gpu").is_none());
    let mut settings: katran::settings::Settings = serde_json::from_value(old).unwrap();
    settings.folder_ai_gpu = true;
    success(&mut api, Command::SetSettings { settings });
    let gpu_page = page(&mut api, 0, None);
    assert_eq!(gpu_page["state"], "succeeded");
    assert!(gpu_seen.load(std::sync::atomic::Ordering::SeqCst));
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert_eq!(
        success(&mut api, Command::GetSettings)["folder_ai_gpu"],
        true
    );
}

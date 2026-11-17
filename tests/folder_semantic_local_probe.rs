//! Explicit optional model/IPC probe. Ordinary tests never read a personal index.
use katran::{
    Command, Request, Service,
    domain_jobs::{Cancellation, JobId},
    folder_semantic::{EverythingSource, Resident, Snapshot, Source},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct SyntheticSource;
impl Source for SyntheticSource {
    fn snapshot(&mut self, query: &str, _: &Cancellation) -> katran::Result<Snapshot> {
        let mut indexed = vec![
            PathBuf::from("C:/Synthetic/Pictures/China-Travel-2026"),
            PathBuf::from("C:/Synthetic/Finance/Invoices"),
            PathBuf::from("C:/Synthetic/Craft/Florist"),
        ];
        indexed.extend(
            (0..8189).map(|i| PathBuf::from(format!("C:/Synthetic/Archive/Folder-{i:05}"))),
        );
        if query == "цветочная мастерская" {
            *indexed.last_mut().unwrap() = PathBuf::from("C:/Synthetic/New/Added-After-Index");
        }
        Ok(Snapshot {
            indexed,
            exact_candidates: Vec::new(),
            incomplete: false,
        })
    }
}
fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "local-probe".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{}", reply["error"]["code"]);
    reply["result"].clone()
}

#[test]
#[ignore = "Requires explicit KATRAN_FOLDER_PROBE_PROFILE and installed optional local runtime"]
fn resident_model_and_dispatcher_cold_warm_probe() {
    let profile =
        std::env::var_os("KATRAN_FOLDER_PROBE_PROFILE").expect("explicit runtime profile");
    let live = std::env::var_os("KATRAN_FOLDER_PROBE_LIVE_INDEX").is_some();
    let temp = tempfile::tempdir().unwrap();
    let mut api = Service::open(temp.path(), 100_000).unwrap();
    let source: Box<dyn Source> = if live {
        Box::new(EverythingSource)
    } else {
        Box::new(SyntheticSource)
    };
    api.set_folder_semantic_backend(Box::new(Resident::new(PathBuf::from(profile))), source);
    for (query, target) in if live {
        [
            ("Katran", "Katran"),
            ("cat run", "Katran"),
            ("cat run", "Katran"),
        ]
    } else {
        [
            ("фотографии поездки в Китай", "China-Travel-2026"),
            ("счета и квитанции", "Invoices"),
            ("цветочная мастерская", "Florist"),
        ]
    } {
        let start = Instant::now();
        let begun = call(
            &mut api,
            Command::BeginJob {
                command: Box::new(Command::SearchFoldersSemantic {
                    query: query.into(),
                    limit: 20,
                }),
            },
        );
        let id = JobId(begun["job_id"].as_u64().unwrap());
        loop {
            let status = call(&mut api, Command::PollJob { job_id: id });
            if status["state"] == "succeeded" {
                let result = &status["result"];
                assert!(result["indexed_count"].as_u64().unwrap() > 0);
                let items = result["items"].as_array().unwrap();
                assert!(!items.is_empty());
                let rank = items
                    .iter()
                    .position(|item| item["path"].as_str().is_some_and(|p| p.ends_with(target)))
                    .map(|i| i + 1);
                // Deliberately omit paths and query text from the evidence output.
                println!(
                    "live={live} indexed={} incomplete={} elapsed_ms={} target_rank={rank:?}",
                    result["indexed_count"],
                    result["incomplete"],
                    start.elapsed().as_millis()
                );
                break;
            }
            assert_ne!(status["state"], "failed", "{}", status["error"]["code"]);
            assert_ne!(status["state"], "cancelled");
            assert!(start.elapsed() < Duration::from_secs(240));
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

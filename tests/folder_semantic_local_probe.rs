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

struct SyntheticSource {
    count: usize,
}
impl Source for SyntheticSource {
    fn snapshot(&mut self, query: &str, _: &Cancellation) -> katran::Result<Snapshot> {
        let mut indexed = vec![
            PathBuf::from("C:/Synthetic/Pictures/China-Travel-2026"),
            PathBuf::from("C:/Synthetic/Finance/Invoices"),
            PathBuf::from("C:/Synthetic/Craft/Florist"),
        ];
        indexed.extend(
            (0..self.count.saturating_sub(3))
                .map(|i| PathBuf::from(format!("C:/Synthetic/Archive/Folder-{i:05}"))),
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
    if std::env::var_os("KATRAN_FOLDER_PROBE_GPU").is_some() {
        let mut settings: katran::settings::Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
        settings.folder_ai_gpu = true;
        call(&mut api, Command::SetSettings { settings });
    }
    let count = std::env::var("KATRAN_FOLDER_PROBE_COUNT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8192)
        .clamp(4, 8192);
    let source: Box<dyn Source> = if live {
        Box::new(EverythingSource)
    } else {
        Box::new(SyntheticSource { count })
    };
    api.set_folder_semantic_backend(
        Box::new(Resident::with_cache_directory(
            PathBuf::from(profile),
            temp.path().join("derived-cache"),
        )),
        source,
    );
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

#[test]
#[ignore = "Requires explicit runtime profile; verifies persistence with a real local model"]
fn resident_cache_survives_worker_restart_and_encodes_only_changed_descriptor() {
    let profile = PathBuf::from(
        std::env::var_os("KATRAN_FOLDER_PROBE_PROFILE").expect("explicit runtime profile"),
    );
    let gpu = std::env::var_os("KATRAN_FOLDER_PROBE_GPU").is_some();
    let count = std::env::var("KATRAN_FOLDER_PROBE_COUNT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(128)
        .clamp(4, 8192);
    let temp = tempfile::tempdir().unwrap();
    let cache_dir = temp.path().join("derived-cache");
    for (phase, query, expected_encoded) in [
        (0, "счета и квитанции", count),
        (1, "счета и квитанции", 0),
        (2, "цветочная мастерская", 1),
    ] {
        let mut api =
            Service::open(&temp.path().join(format!("service-{phase}")), 100_000).unwrap();
        let mut settings: katran::settings::Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
        settings.folder_ai_gpu = gpu;
        call(&mut api, Command::SetSettings { settings });
        api.set_folder_semantic_backend(
            Box::new(Resident::with_cache_directory(
                profile.clone(),
                cache_dir.clone(),
            )),
            Box::new(SyntheticSource { count }),
        );
        let started = Instant::now();
        let begun = call(
            &mut api,
            Command::BeginJob {
                command: Box::new(Command::SearchFoldersSemanticPage {
                    query: query.into(),
                    offset: 0,
                    limit: 10,
                    snapshot: None,
                }),
            },
        );
        let id = JobId(begun["job_id"].as_u64().unwrap());
        loop {
            let status = call(&mut api, Command::PollJob { job_id: id });
            if status["state"] == "succeeded" {
                let cache = &status["result"]["cache"];
                assert_eq!(
                    cache["encoded"].as_u64(),
                    Some(expected_encoded as u64),
                    "phase={phase},cache={cache}"
                );
                assert_eq!(cache["persisted"], true, "phase={phase},cache={cache}");
                if phase > 0 {
                    assert_eq!(
                        cache["reused"].as_u64(),
                        Some((count - expected_encoded) as u64)
                    );
                }
                println!(
                    "restart_phase={phase} gpu={gpu} elapsed_ms={} cache={cache}",
                    started.elapsed().as_millis()
                );
                break;
            }
            assert_ne!(status["state"], "failed", "{}", status["error"]["code"]);
            assert!(started.elapsed() < Duration::from_secs(240));
            std::thread::sleep(Duration::from_millis(25));
        }
        // Dropping Service kills its resident child, forcing disk reload next phase.
        drop(api);
    }
}

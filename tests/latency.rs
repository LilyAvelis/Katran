//! Explicit synthetic measurements, not machine-dependent CI deadlines.
#[path = "common/source_api.rs"]
mod source_api;
use katran::{Command, Event, Request, Service, settings::Settings};
use std::{path::Path, time::Instant};

const QUOTA: u64 = 256 * 1024 * 1024;

fn call(api: &mut Service, command: Command) -> serde_json::Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "synthetic-latency".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

fn measure(label: &str, count: usize, mut operation: impl FnMut(usize)) {
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let started = Instant::now();
        operation(index);
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    report(label, samples);
}

fn report(label: &str, mut samples: Vec<f64>) {
    let count = samples.len();
    samples.sort_by(f64::total_cmp);
    let percentile = |percent: usize| samples[(count * percent).div_ceil(100) - 1];
    println!(
        "LATENCY {label}: n={count} p50_ms={:.3} p95_ms={:.3} max_ms={:.3}",
        percentile(50),
        percentile(95),
        samples[count - 1]
    );
}

fn ingest(api: &mut Service, path: &Path, source: &str) -> Event {
    serde_json::from_value(call(
        api,
        Command::IngestPath {
            path: path.into(),
            source: source.into(),
        },
    ))
    .unwrap()
}

#[test]
#[ignore = "explicit synthetic latency report; run in release with --ignored --nocapture"]
fn synthetic_latency_report() {
    println!(
        "LATENCY profile={} os={} arch={}; cold means empty application cache, not cold OS cache",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let mut api = Service::open(&data, QUOTA).unwrap();
    let picture = image::RgbaImage::from_fn(1920, 1080, |x, y| {
        let n = x
            .wrapping_mul(1664525)
            .wrapping_add(y.wrapping_mul(1013904223));
        image::Rgba([n as u8, (n >> 8) as u8, (n >> 16) as u8, 255])
    });
    let picture_path = temp.path().join("synthetic-1920x1080.png");
    picture.save(&picture_path).unwrap();
    let png = std::fs::read(&picture_path).unwrap();
    println!("LATENCY fixture image=1920x1080 png_bytes={}", png.len());
    measure("ingest_clipboard_image_new_blob", 20, |index| {
        let mut bytes = png.clone();
        // A unique PNG trailing payload exercises durable new-blob publication.
        bytes.extend_from_slice(&(index as u64).to_le_bytes());
        call(
            &mut api,
            Command::IngestBytes {
                name: "synthetic.png".into(),
                bytes,
                source: "clipboard_image".into(),
            },
        );
    });
    measure("ingest_clipboard_image_existing_blob", 30, |_| {
        call(
            &mut api,
            Command::IngestBytes {
                name: "synthetic.png".into(),
                bytes: png.clone(),
                source: "clipboard_image".into(),
            },
        );
    });
    let event = ingest(&mut api, &picture_path, "synthetic");
    for edge in [280, 512] {
        // One independent empty cache per sample avoids timing cache deletion.
        let caches: Vec<_> = (0..10).map(|_| tempfile::tempdir().unwrap()).collect();
        measure(
            &format!("file_visual_image_cold_edge_{edge}"),
            10,
            |index| {
                let visual =
                    katran::file_visual::render(caches[index].path(), &event, edge).unwrap();
                assert_eq!(visual["visual_kind"], "image");
                assert_eq!(visual["cache_hit"], false);
            },
        );
        let cache = tempfile::tempdir().unwrap();
        katran::file_visual::render(cache.path(), &event, edge).unwrap();
        measure(&format!("file_visual_image_warm_edge_{edge}"), 30, |_| {
            let visual = katran::file_visual::render(cache.path(), &event, edge).unwrap();
            assert_eq!(visual["cache_hit"], true);
        });
    }
    let history_data = temp.path().join("history");
    let mut history = Service::open(&history_data, QUOTA).unwrap();
    for target in [100, 1000] {
        let previous = if target == 100 { 0 } else { 100 };
        for index in previous..target {
            call(
                &mut history,
                Command::IngestBytes {
                    name: format!("synthetic-{index}.png"),
                    bytes: vec![1, 2, 3],
                    source: "clipboard_image".into(),
                },
            );
        }
        measure(&format!("browse_history_{target}_page_100"), 30, |_| {
            let page = call(
                &mut history,
                Command::BrowseShelf {
                    query: String::new(),
                    limit: 100,
                    after_id: None,
                },
            );
            assert_eq!(page["events"].as_array().unwrap().len(), 100);
        });
    }
    let shots = temp.path().join("shots");
    std::fs::create_dir(&shots).unwrap();
    let small_png = temp.path().join("small.png");
    image::RgbaImage::new(32, 32).save(&small_png).unwrap();
    let small_png = std::fs::read(small_png).unwrap();
    for index in 0..100 {
        std::fs::write(shots.join(format!("synthetic-{index}.png")), &small_png).unwrap();
    }
    let mut settings = Settings::defaults(QUOTA);
    settings.sharex_folder = Some(shots.clone());
    settings.downloads_enabled = false;
    measure(
        "supervisor_configure_discover_hash_commit_100_files",
        1,
        |_| {
            call(
                &mut history,
                Command::SetSettings {
                    settings: settings.clone(),
                },
            );
            let mut committed = std::collections::HashSet::new();
            source_api::poll_until(&mut history, "100 synthetic source commits", |_, batch| {
                assert!(
                    batch["source_errors"].as_array().unwrap().is_empty(),
                    "{batch}"
                );
                for item in batch["items"].as_array().unwrap() {
                    assert_eq!(item["ok"], true, "{batch}");
                    committed.insert(item["event"]["id"].as_i64().unwrap());
                }
                committed.len() == 100
            });
        },
    );
    measure(
        "schedule_sharex_rescan_unchanged_100_files_1100_history",
        30,
        |_| {
            let scheduled = call(
                &mut history,
                Command::RescanSource {
                    source_id: katran::source_policy::SourceId::Sharex,
                },
            );
            assert_eq!(scheduled["scheduled"], true);
        },
    );
    println!(
        "LATENCY unchanged rescan above measures scheduling only; no scan-completion token is exposed"
    );
    assert!(source_api::settle(&mut history, katran::source_policy::SourceId::Sharex).is_empty());
    let mut scanner = katran::sharex::SharexScanner::default();
    scanner.scan(&shots).unwrap();
    scanner.scan(&shots).unwrap();
    measure("sharex_discovery_only_100_files", 30, |_| {
        scanner.scan(&shots).unwrap();
    });
    let mut downloads = katran::downloads::DownloadsScanner::default();
    downloads.scan(&shots).unwrap();
    downloads.scan(&shots).unwrap();
    measure("downloads_discovery_only_100_files", 30, |_| {
        downloads.scan(&shots).unwrap();
    });

    #[cfg(windows)]
    {
        use katran::{interaction::ShelfTab, source_policy::SourceId};
        for (label, bytes) in [
            ("32px", small_png.as_slice()),
            ("1920x1080", png.as_slice()),
        ] {
            let folder = tempfile::tempdir().unwrap();
            let mut settings = Settings::defaults(QUOTA);
            settings.sharex_folder = Some(folder.path().into());
            settings.downloads_enabled = false;
            call(&mut history, Command::SetSettings { settings });
            call(
                &mut history,
                Command::SetTab {
                    tab: ShelfTab::Screenshots,
                },
            );
            source_api::poll_until(
                &mut history,
                "configured source native readiness",
                |_, batch| {
                    batch["sources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|s| s["source_id"] == "sharex" && s["state"] == "watching")
                },
            );
            let mut committed_samples = Vec::new();
            measure(
                &format!("native_write_supervisor_commit_browse_{label}_history_1100"),
                20,
                |index| {
                    let start = Instant::now();
                    let path = folder.path().join(format!("native-{index}.png"));
                    std::fs::write(&path, bytes).unwrap();
                    let expected = path.canonicalize().unwrap();
                    let mut committed = None;
                    source_api::poll_until(&mut history, "native synthetic commit", |_, batch| {
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
                                committed = item["event"]["id"].as_i64();
                            }
                        }
                        committed.is_some()
                    });
                    committed_samples.push(start.elapsed().as_secs_f64() * 1000.0);
                    let page = call(
                        &mut history,
                        Command::BrowseShelf {
                            query: String::new(),
                            limit: 100,
                            after_id: None,
                        },
                    );
                    assert!(
                        page["events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|e| e["id"].as_i64() == committed)
                    );
                },
            );
            report(
                &format!("native_write_to_supervisor_committed_{label}"),
                committed_samples,
            );
            // Requesting a rescan is still a source API operation, not receipt injection.
            call(
                &mut history,
                Command::RescanSource {
                    source_id: SourceId::Sharex,
                },
            );
        }
    }
}

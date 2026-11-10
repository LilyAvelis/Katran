//! Explicit synthetic measurements, not machine-dependent CI deadlines.
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
    call(&mut history, Command::SetSettings { settings });
    measure("scan_sharex_first_observation_100_files", 1, |_| {
        call(&mut history, Command::ScanSharex);
    });
    measure("scan_sharex_first_ingest_100_files", 1, |_| {
        call(&mut history, Command::ScanSharex);
    });
    measure("scan_sharex_unchanged_100_files_1100_history", 30, |_| {
        call(&mut history, Command::ScanSharex);
    });
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
        use katran::folder_watch::{FolderConfig, FolderEvent, FolderSource, FolderWatcher};
        for (label, bytes) in [
            ("32px", small_png.as_slice()),
            ("1920x1080", png.as_slice()),
        ] {
            let folder = tempfile::tempdir().unwrap();
            let mut settings = Settings::defaults(QUOTA);
            settings.sharex_folder = Some(folder.path().into());
            call(&mut history, Command::SetSettings { settings });
            let watcher = FolderWatcher::start(vec![FolderConfig {
                source: FolderSource::Sharex,
                root: folder.path().into(),
            }])
            .unwrap();
            let mut ready = Vec::new();
            measure(
                &format!("native_write_ready_commit_browse_{label}_history_1100"),
                20,
                |index| {
                    let start = Instant::now();
                    let path = folder.path().join(format!("native-{index}.png"));
                    std::fs::write(&path, bytes).unwrap();
                    let deadline = start + std::time::Duration::from_secs(10);
                    let candidate = loop {
                        match watcher.try_recv() {
                            Some(FolderEvent::Candidate(candidate)) => break candidate,
                            Some(FolderEvent::Error { message, .. }) => panic!("{message}"),
                            _ => {}
                        }
                        assert!(
                            Instant::now() < deadline,
                            "native synthetic watcher timeout"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    };
                    ready.push(start.elapsed().as_secs_f64() * 1000.0);
                    let result = call(
                        &mut history,
                        Command::CommitFolderCandidates {
                            items: vec![candidate],
                        },
                    );
                    assert_eq!(result["items"][0]["ok"], true, "{result}");
                    call(
                        &mut history,
                        Command::BrowseShelf {
                            query: String::new(),
                            limit: 100,
                            after_id: None,
                        },
                    );
                },
            );
            report(&format!("native_write_to_prepared_{label}"), ready);
        }
    }
}

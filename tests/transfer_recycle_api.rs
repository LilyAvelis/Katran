#![cfg(windows)]
use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[path = "common/source_api.rs"]
mod source_api;
fn call(api: &mut Service, command: Command) -> Value {
    let result = api.execute(Request {
        api_version: 1,
        request_id: "recycle-only".into(),
        command,
    });
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
}
struct FakeTrash {
    original: PathBuf,
    mode: u8,
    calls: Arc<AtomicUsize>,
}
impl katran::downloads::Trash for FakeTrash {
    fn recycle(&self, staged: &Path) -> katran::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(std::fs::read(staged)?, b"synthetic original");
        assert!(!self.original.exists());
        if self.mode == 0 {
            std::fs::remove_file(staged)?;
            return Ok(());
        }
        if self.mode == 3 {
            return Ok(());
        }
        if self.mode == 2 {
            std::fs::write(&self.original, b"replacement")?;
        }
        Err(std::io::Error::other("synthetic trash failure").into())
    }
}
fn wait_job(api: &mut Service) -> Value {
    let job = call(
        api,
        Command::BeginJob {
            command: Box::new(Command::TransferSelection),
        },
    );
    let id = serde_json::from_value(job["job_id"].clone()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let state = call(api, Command::PollJob { job_id: id });
        if state["state"] == "succeeded" {
            return state["result"].clone();
        }
        assert!(
            state["state"] != "failed" && std::time::Instant::now() < deadline,
            "{state}"
        );
        std::thread::yield_now();
    }
}
#[test]
fn external_moves_use_trash_and_report_recovery_for_each_outcome() {
    for managed in [false, true] {
        for job in [false, true] {
            for mode in [0u8, 1, 2, 3] {
                let root = tempfile::tempdir().unwrap();
                let source = tempfile::tempdir().unwrap();
                let target = tempfile::tempdir().unwrap();
                let path = source.path().join("synthetic.bin");
                std::fs::write(&path, b"synthetic original").unwrap();
                let mut api = Service::open(root.path(), 1_000_000).unwrap();
                let calls = Arc::new(AtomicUsize::new(0));
                api.set_trash(Box::new(FakeTrash {
                    original: path.clone(),
                    mode,
                    calls: calls.clone(),
                }));
                let event = if managed {
                    let mut settings: katran::settings::Settings =
                        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
                    settings.sharex_folder = None;
                    settings.downloads_enabled = true;
                    settings.downloads_folder = Some(source.path().into());
                    call(&mut api, Command::SetSettings { settings });
                    source_api::wait_event(
                        &mut api,
                        katran::source_policy::SourceId::Downloads,
                        &path,
                    )
                } else {
                    call(
                        &mut api,
                        Command::IngestPath {
                            path: path.clone(),
                            source: "synthetic".into(),
                        },
                    )
                };
                let id = event["id"].as_i64().unwrap();
                call(
                    &mut api,
                    Command::ChooseFolder {
                        path: target.path().into(),
                    },
                );
                call(
                    &mut api,
                    Command::SetTab {
                        tab: ShelfTab::Images,
                    },
                );
                call(&mut api, Command::SetVisible { ids: vec![id] });
                call(
                    &mut api,
                    Command::Select {
                        id,
                        mode: SelectionMode::Replace,
                    },
                );
                let result = if job {
                    wait_job(&mut api)
                } else {
                    call(&mut api, Command::TransferSelection)
                };
                let item = &result["items"][0];
                assert_eq!(calls.load(Ordering::SeqCst), 1, "{result}");
                assert_eq!(
                    std::fs::read(target.path().join("synthetic.bin")).unwrap(),
                    b"synthetic original"
                );
                if mode == 0 {
                    assert_eq!(item["status"], "moved", "{result}");
                    assert_eq!(item["recycle"]["recycled"], true);
                    assert_eq!(item["original_removed"], true);
                    assert!(!path.exists());
                } else {
                    assert_eq!(item["error"]["code"], "recycle_failed", "{result}");
                    let response = api.execute(Request {
                        api_version: 1,
                        request_id: "still-on-shelf".into(),
                        command: Command::Get { id },
                    });
                    assert_eq!(response["ok"], true, "{response}");
                    if mode == 1 {
                        assert_eq!(item["status"], "source_not_moved", "{result}");
                        assert_eq!(item["original_removed"], false);
                        assert_eq!(std::fs::read(&path).unwrap(), b"synthetic original");
                    } else {
                        assert_eq!(item["status"], "recovery_required", "{result}");
                        assert_eq!(item["original_removed"], true);
                        if mode == 2 {
                            assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
                        } else {
                            assert!(!path.exists());
                        }
                        assert_eq!(
                            std::fs::read(item["recovery_path"].as_str().unwrap()).unwrap(),
                            b"synthetic original"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn screenshot_move_uses_configured_source_root_and_fake_trash() {
    struct ImageTrash {
        bytes: Vec<u8>,
        calls: Arc<AtomicUsize>,
    }
    impl katran::downloads::Trash for ImageTrash {
        fn recycle(&self, staged: &Path) -> katran::Result<()> {
            assert_eq!(std::fs::read(staged)?, self.bytes);
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::fs::remove_file(staged)?;
            Ok(())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let path = source.path().join("synthetic.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([5, 10, 15, 255]))
        .save(&path)
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_trash(Box::new(ImageTrash {
        bytes: bytes.clone(),
        calls: calls.clone(),
    }));
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings.sharex_folder = Some(source.path().into());
    settings.downloads_enabled = false;
    call(&mut api, Command::SetSettings { settings });
    let event = source_api::wait_event(&mut api, katran::source_policy::SourceId::Sharex, &path);
    let id = event["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: target.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let result = wait_job(&mut api);
    assert_eq!(result["items"][0]["status"], "moved", "{result}");
    assert_eq!(result["items"][0]["recycle"]["recycled"], true);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read(target.path().join("synthetic.png")).unwrap(),
        bytes
    );
    assert!(!path.exists());
}

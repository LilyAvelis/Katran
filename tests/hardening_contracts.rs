use katran::{
    Command, Request, Service,
    source_policy::{DeletePolicy, SourceId},
};
use serde_json::Value;
use std::time::{Duration, Instant};
fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "hardening".into(),
        command,
    })
}
fn configure(api: &mut Service, root: &std::path::Path) {
    let mut settings = katran::settings::Settings::defaults(1000000);
    settings.downloads_enabled = false;
    settings.sharex_folder = Some(root.into());
    assert_eq!(call(api, Command::SetSettings { settings })["ok"], true);
}
fn source_event(api: &mut Service) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let r = call(api, Command::PollSources { limit: 8 });
        assert_eq!(r["ok"], true, "{r}");
        if let Some(items) = r["result"]["items"].as_array()
            && let Some(event) = items.iter().find_map(|i| i.get("event"))
        {
            return event.clone();
        }
        assert!(Instant::now() < deadline, "{r}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn provenance_is_not_delete_authority_and_supervisor_is() {
    struct FakeTrash(std::path::PathBuf);
    impl katran::downloads::Trash for FakeTrash {
        fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
            assert!(path.components().any(|c| {
                c.as_os_str()
                    .to_string_lossy()
                    .starts_with(".katran-recovery-")
            }));
            std::fs::rename(path, self.0.join(path.file_name().unwrap())).map_err(Into::into)
        }
    }
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let path = folder.path().join("fixture.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    let mut api = Service::open(data.path(), 1000000).unwrap();
    api.set_trash(Box::new(FakeTrash(bin.path().into())));
    configure(&mut api, folder.path());
    let generic = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "sharex".into(),
        },
    );
    assert_eq!(generic["result"]["delete_policy"]["kind"], "shelf_only");
    let id = generic["result"]["id"].as_i64().unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::DeleteSelection {
                ids: vec![id],
                confirmed: false
            }
        )["ok"],
        true
    );
    assert!(path.exists());
    let event = source_event(&mut api);
    let event: katran::Event = serde_json::from_value(event).unwrap();
    assert!(matches!(
        event.delete_policy,
        DeletePolicy::LiveOriginal {
            source_id: SourceId::Sharex,
            ..
        }
    ));
    let result = call(
        &mut api,
        Command::DeleteSelection {
            ids: vec![event.id],
            confirmed: false,
        },
    );
    assert_eq!(result["result"]["deleted"], 1, "{result}");
    assert!(!path.exists());
    assert!(bin.path().join("fixture.png").is_file());
    assert_eq!(result["result"]["items"][0]["recycle"]["recycled"], true);
}
#[test]
fn changing_configuration_immediately_revokes_old_original_authority() {
    let data = tempfile::tempdir().unwrap();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let path = a.path().join("fixture.png");
    image::RgbaImage::new(1, 1).save(&path).unwrap();
    let mut api = Service::open(data.path(), 1000000).unwrap();
    configure(&mut api, a.path());
    let event = source_event(&mut api);
    configure(&mut api, b.path());
    configure(&mut api, a.path());
    let result = call(
        &mut api,
        Command::DeleteSelection {
            ids: vec![event["id"].as_i64().unwrap()],
            confirmed: false,
        },
    );
    assert_eq!(
        result["result"]["items"][0]["error"]["code"], "stale_source_generation",
        "{result}"
    );
    assert!(path.exists());
}
#[test]
fn future_database_version_is_rejected_without_downgrade() {
    let data = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(data.path().join("index.sqlite3")).unwrap();
    db.execute_batch("PRAGMA user_version=77; CREATE TABLE proof(value TEXT);")
        .unwrap();
    drop(db);
    assert!(matches!(Service::open(data.path(),1000),Err(e) if e.code=="unsupported_schema"));
    let db = rusqlite::Connection::open(data.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        77
    );
}

#[test]
fn raw_worker_claims_cannot_forge_identity_or_original_membership() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("fixture.png");
    image::RgbaImage::new(1, 1).save(&path).unwrap();
    let mut api = Service::open(data.path(), 1000000).unwrap();
    configure(&mut api, folder.path());
    let metadata = path.metadata().unwrap();
    let candidate = katran::folder_watch::PreparedCandidate {
        source: katran::folder_watch::FolderSource::Sharex,
        root: folder.path().canonicalize().unwrap(),
        path: path.clone(),
        size: metadata.len(),
        modified: metadata.modified().unwrap(),
        content_hash: "0".repeat(64),
    };
    assert_eq!(
        call(
            &mut api,
            Command::CommitFolderCandidates {
                items: vec![candidate]
            }
        )["error"]["code"],
        "untrusted_worker_result"
    );
    assert_eq!(
        call(
            &mut api,
            Command::CommitScreenshotMatches {
                pairs: vec![katran::screenshot_identity::MatchPair {
                    clipboard_id: 1,
                    clipboard_revision: "blob:red".into(),
                    source_id: 2,
                    source_revision: "ref:blue".into()
                }]
            }
        )["error"]["code"],
        "untrusted_worker_result"
    );
    assert!(path.exists());
}
#[test]
fn continuing_classifier_drains_257_arrivals_and_keeps_verified_copies_hidden_after_original_removal()
 {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("fixture.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([44, 55, 66, 255]))
        .save(&path)
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for n in 0..128 {
        std::fs::write(folder.path().join(format!("duplicate-{n}.png")), &bytes).unwrap();
    }
    let mut api = Service::open(data.path(), 1000000).unwrap();
    configure(&mut api, folder.path());
    for _ in 0..257 {
        assert_eq!(
            call(
                &mut api,
                Command::IngestBytes {
                    name: "clipboard.png".into(),
                    bytes: bytes.clone(),
                    source: "clipboard_image".into()
                }
            )["ok"],
            true
        );
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut source_count = 0usize;
    loop {
        let result = call(&mut api, Command::PollSources { limit: 8 });
        source_count += result["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["event"]["source"] == "sharex")
            .count();
        if source_count == 129 && result["result"]["classification"]["queued"] == 0 {
            assert_eq!(result["result"]["classification"]["failed"], 0);
            break;
        }
        assert!(Instant::now() < deadline, "{result}");
        std::thread::sleep(Duration::from_millis(2));
    }
    let db = rusqlite::Connection::open(data.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM verified_source_digests", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM verified_screenshot_matches",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "Identical clips and sources must not expand into a cross product"
    );
    drop(db);
    call(
        &mut api,
        Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        },
    );
    let browse = call(
        &mut api,
        Command::BrowseShelf {
            query: String::new(),
            limit: 1000,
            after_id: None,
        },
    );
    assert_eq!(
        browse["result"]["events"].as_array().unwrap().len(),
        0,
        "{browse}"
    );
    std::fs::remove_file(&path).unwrap();
    call(
        &mut api,
        Command::RescanSource {
            source_id: SourceId::Sharex,
        },
    );
    for _ in 0..40 {
        call(&mut api, Command::PollSources { limit: 8 });
        std::thread::sleep(Duration::from_millis(5));
    }
    let browse = call(
        &mut api,
        Command::BrowseShelf {
            query: String::new(),
            limit: 1000,
            after_id: None,
        },
    );
    assert!(browse["result"]["events"].as_array().unwrap().is_empty());
}

#[test]
fn mixed_original_batch_uses_injected_recycler_and_reports_each_failure() {
    struct FakeTrash {
        bin: std::path::PathBuf,
        calls: std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>,
    }
    impl katran::downloads::Trash for FakeTrash {
        fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
            self.calls.lock().unwrap().push(path.into());
            if path.file_name().unwrap() == "fail.txt" {
                return Err(std::io::Error::other("synthetic recycle failure").into());
            }
            std::fs::rename(path, self.bin.join(path.file_name().unwrap()))?;
            Ok(())
        }
    }
    let data = tempfile::tempdir().unwrap();
    let shots = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let shot = shots.path().join("shot.png");
    image::RgbaImage::new(1, 1).save(&shot).unwrap();
    std::fs::write(downloads.path().join("good.txt"), b"good").unwrap();
    std::fs::write(downloads.path().join("fail.txt"), b"keep").unwrap();
    let mut api = Service::open(data.path(), 1000000).unwrap();
    let mut settings = katran::settings::Settings::defaults(1000000);
    settings.sharex_folder = Some(shots.path().into());
    settings.downloads_folder = Some(downloads.path().into());
    call(&mut api, Command::SetSettings { settings });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut ids = Vec::new();
    loop {
        let r = call(&mut api, Command::PollSources { limit: 8 });
        for item in r["result"]["items"].as_array().unwrap() {
            if let Some(id) = item["event"]["id"].as_i64() {
                ids.push(id);
            }
        }
        if ids.len() == 3 {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let shelf = call(
        &mut api,
        Command::IngestBytes {
            name: "copy.png".into(),
            source: "test".into(),
            bytes: vec![9],
        },
    )["result"]["id"]
        .as_i64()
        .unwrap();
    ids.extend([shelf, 99999]);
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    api.set_trash(Box::new(FakeTrash {
        bin: bin.path().into(),
        calls: calls.clone(),
    }));
    let r = call(
        &mut api,
        Command::DeleteSelection {
            ids: ids.clone(),
            confirmed: false,
        },
    );
    assert_eq!(r["result"]["confirmation_required"], true);
    assert!(calls.lock().unwrap().is_empty());
    assert!(shot.exists());
    let r = call(
        &mut api,
        Command::DeleteSelection {
            ids,
            confirmed: true,
        },
    );
    assert_eq!(r["result"]["deleted"], 3, "{r}");
    let items = r["result"]["items"].as_array().unwrap();
    assert_eq!(items.iter().filter(|i| i["ok"] == false).count(), 2);
    assert_eq!(
        std::fs::read(downloads.path().join("fail.txt")).unwrap(),
        b"keep"
    );
    assert!(!shot.exists());
    assert_eq!(std::fs::read(bin.path().join("good.txt")).unwrap(), b"good");
    assert!(bin.path().join("shot.png").is_file());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    let mut names: Vec<_> = calls
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(names, ["fail.txt", "good.txt", "shot.png"]);
    assert!(calls.iter().all(|path| path.components().any(|c| {
        c.as_os_str()
            .to_string_lossy()
            .starts_with(".katran-recovery-")
    })));
    let failed = items
        .iter()
        .find(|item| item["recycle"]["status"] == "restored")
        .unwrap();
    assert_eq!(failed["ok"], false);
}

#[test]
fn current_schema_restart_recovers_unfinished_classification_without_migration() {
    let data = tempfile::tempdir().unwrap();
    let screenshots = tempfile::tempdir().unwrap();
    let mut api = Service::open(data.path(), 1000000).unwrap();
    configure(&mut api, screenshots.path());
    call(
        &mut api,
        Command::IngestBytes {
            name: "clipboard.png".into(),
            source: "clipboard_image".into(),
            bytes: vec![1, 2, 3],
        },
    );
    drop(api);
    let db = rusqlite::Connection::open(data.path().join("index.sqlite3")).unwrap();
    db.execute("UPDATE classification_work SET state='inflight'", [])
        .unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        10
    );
    drop(db);
    let mut api = Service::open(data.path(), 1000000).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let r = call(&mut api, Command::PollSources { limit: 8 });
        if r["result"]["classification"]["queued"] == 0 {
            assert_eq!(r["result"]["classification"]["failed"], 1);
            break;
        }
        assert!(Instant::now() < deadline, "{r}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn trusted_folder_observation_does_not_change_existing_generic_shelf_ownership() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("fixture.png");
    image::RgbaImage::new(1, 1).save(&path).unwrap();
    let mut api = Service::open(data.path(), 1000000).unwrap();
    let generic = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "sharex".into(),
        },
    )["result"]["id"]
        .as_i64()
        .unwrap();
    configure(&mut api, folder.path());
    let source = source_event(&mut api);
    assert_ne!(source["id"], generic);
    assert_eq!(
        call(&mut api, Command::Get { id: generic })["result"]["delete_policy"]["kind"],
        "shelf_only"
    );
    assert_eq!(
        call(
            &mut api,
            Command::DeleteSelection {
                ids: vec![generic],
                confirmed: false
            }
        )["ok"],
        true
    );
    assert!(path.exists());
}

#[test]
fn migration_saves_consistent_checkpoint_before_version_change() {
    let data = tempfile::tempdir().unwrap();
    let mut api = Service::open(data.path(), 1000000).unwrap();
    let id = call(
        &mut api,
        Command::IngestBytes {
            name: "saved.png".into(),
            source: "test".into(),
            bytes: vec![9],
        },
    )["result"]["id"]
        .as_i64()
        .unwrap();
    drop(api);
    let db = rusqlite::Connection::open(data.path().join("index.sqlite3")).unwrap();
    db.execute_batch("PRAGMA user_version=1;").unwrap();
    drop(db);
    let mut api = Service::open(data.path(), 1000000).unwrap();
    assert_eq!(call(&mut api, Command::Get { id })["ok"], true);
    let backups: Vec<_> = std::fs::read_dir(data.path().join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("SELECT COUNT(*) FROM events WHERE id=?1", [id], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

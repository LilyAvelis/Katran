#[path = "common/source_api.rs"]
mod source_api;
use katran::{Command, Service, settings::Settings};
use serde_json::Value;

#[cfg(windows)]
struct SyntheticTrash;
#[cfg(windows)]
impl katran::downloads::Trash for SyntheticTrash {
    fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
        assert!(path.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .starts_with(".katran-recovery-")
        }));
        std::fs::remove_file(path).map_err(Into::into)
    }
}

fn ingest(service: &mut Service, path: &std::path::Path, source: &str) -> i64 {
    service
        .test_dispatch(Command::IngestPath {
            path: path.into(),
            source: source.into(),
        })
        .unwrap()["id"]
        .as_i64()
        .unwrap()
}
fn delete(service: &mut Service, ids: Vec<i64>, confirmed: bool) -> Value {
    service
        .test_dispatch(Command::DeleteSelection { ids, confirmed })
        .unwrap()
}

#[test]
fn group_requires_confirmation_and_changed_revision_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("screenshots");
    std::fs::create_dir(&folder).unwrap();
    let path = folder.join("capture.png");
    std::fs::write(&path, b"synthetic screenshot").unwrap();
    let mut service = Service::open(&temp.path().join("shelf"), 1_000_000).unwrap();
    service
        .test_dispatch(Command::SetSettings {
            settings: Settings {
                sharex_folder: Some(folder),
                downloads_enabled: false,
                ..Default::default()
            },
        })
        .unwrap();
    let id =
        source_api::wait_event(&mut service, katran::source_policy::SourceId::Sharex, &path)["id"]
            .as_i64()
            .unwrap();
    let other = temp.path().join("other.txt");
    std::fs::write(&other, b"other synthetic").unwrap();
    let second = ingest(&mut service, &other, "test");
    let pending = delete(&mut service, vec![id, second, id], false);
    assert_eq!(pending["ids"], serde_json::json!([id, second]));
    assert_eq!(pending["confirmation_required"], true);
    assert!(path.exists());
    std::fs::write(&path, b"different screenshot").unwrap();
    let failed = delete(&mut service, vec![id], false);
    assert_eq!(failed["deleted"], 0);
    assert_eq!(failed["items"][0]["error"]["code"], "stale_selection");
    assert!(path.exists());
}

#[cfg(windows)]
#[test]
fn single_original_deletes_immediately_and_duplicates_are_one_item() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("single.png");
    std::fs::write(&path, b"synthetic single").unwrap();
    let mut service = Service::open(&temp.path().join("shelf"), 1_000_000).unwrap();
    service.set_trash(Box::new(SyntheticTrash));
    service
        .test_dispatch(Command::SetSettings {
            settings: Settings {
                sharex_folder: Some(temp.path().into()),
                downloads_enabled: false,
                ..Default::default()
            },
        })
        .unwrap();
    let id =
        source_api::wait_event(&mut service, katran::source_policy::SourceId::Sharex, &path)["id"]
            .as_i64()
            .unwrap();
    // A pre-v8 live ShareX event may retain its old permanent policy. The
    // effect must still use the recycle-only adapter and report that fact.
    let db = rusqlite::Connection::open(temp.path().join("shelf/index.sqlite3")).unwrap();
    let policy: String = db
        .query_row(
            "SELECT delete_policy_json FROM events WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let mut policy: Value = serde_json::from_str(&policy).unwrap();
    policy["mode"] = serde_json::json!("permanent");
    db.execute(
        "UPDATE events SET delete_policy_json=?1 WHERE id=?2",
        rusqlite::params![policy.to_string(), id],
    )
    .unwrap();
    drop(db);
    let result = delete(&mut service, vec![id, id], false);
    assert_eq!(result["confirmation_required"], false);
    assert_eq!(result["deleted"], 1);
    assert_eq!(result["permanent_files"], false);
    assert_eq!(result["recycle_bin"], true);
    assert_eq!(result["items"][0]["recycle"]["recycled"], true);
    assert_eq!(result["ids"], serde_json::json!([id]));
    assert!(!path.exists());
}

#[cfg(windows)]
#[test]
fn mixed_batch_deletes_only_authorized_original_and_reports_each_item() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("screenshots");
    std::fs::create_dir(&folder).unwrap();
    let original = folder.join("capture.png");
    let outside = temp.path().join("outside.png");
    let clipboard = temp.path().join("clipboard.png");
    for path in [&original, &outside, &clipboard] {
        std::fs::write(path, b"synthetic").unwrap();
    }
    let mut service = Service::open(&temp.path().join("shelf"), 1_000_000).unwrap();
    service.set_trash(Box::new(SyntheticTrash));
    service
        .test_dispatch(Command::SetSettings {
            settings: Settings {
                sharex_folder: Some(folder),
                downloads_enabled: false,
                ..Default::default()
            },
        })
        .unwrap();
    let first = source_api::wait_event(
        &mut service,
        katran::source_policy::SourceId::Sharex,
        &original,
    )["id"]
        .as_i64()
        .unwrap();
    let unsafe_id = ingest(&mut service, &outside, "sharex");
    let ordinary = ingest(&mut service, &clipboard, "test");
    let result = delete(&mut service, vec![first, unsafe_id, ordinary, 99999], true);
    assert_eq!(result["deleted"], 3);
    assert_eq!(
        result["items"][1]["ok"], true,
        "a provenance label grants only shelf deletion"
    );
    assert_eq!(result["items"][3]["error"]["code"], "not_found");
    assert_eq!(result["items"].as_array().unwrap().len(), 4);
    assert!(!original.exists());
    assert!(outside.exists());
    assert!(clipboard.exists());
}

#[test]
fn missing_original_can_remove_its_shelf_record() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("missing.png");
    std::fs::write(&path, b"synthetic").unwrap();
    let mut service = Service::open(&temp.path().join("shelf"), 1_000_000).unwrap();
    service
        .test_dispatch(Command::SetSettings {
            settings: Settings {
                sharex_folder: Some(temp.path().into()),
                downloads_enabled: false,
                ..Default::default()
            },
        })
        .unwrap();
    let id =
        source_api::wait_event(&mut service, katran::source_policy::SourceId::Sharex, &path)["id"]
            .as_i64()
            .unwrap();
    std::fs::remove_file(path).unwrap();
    let result = delete(&mut service, vec![id], true);
    assert_eq!(result["deleted"], 1);
    assert_eq!(result["items"][0]["original_missing"], true);
}

#[cfg(windows)]
#[test]
fn recycle_failure_preserves_shelf_and_restores_or_reports_recovery() {
    use std::path::{Path, PathBuf};
    struct FailedTrash {
        original: PathBuf,
        replace: bool,
    }
    impl katran::downloads::Trash for FailedTrash {
        fn recycle(&self, staged: &Path) -> katran::Result<()> {
            assert_eq!(std::fs::read(staged).unwrap(), b"synthetic original");
            assert!(!self.original.exists());
            if self.replace {
                std::fs::write(&self.original, b"independent replacement").unwrap();
            }
            Err(std::io::Error::other("synthetic trash failure").into())
        }
    }
    for replace in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("shots");
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("capture.png");
        std::fs::write(&path, b"synthetic original").unwrap();
        let mut service = Service::open(&temp.path().join("shelf"), 1_000_000).unwrap();
        service.set_trash(Box::new(FailedTrash {
            original: path.clone(),
            replace,
        }));
        service
            .test_dispatch(Command::SetSettings {
                settings: Settings {
                    sharex_folder: Some(folder),
                    downloads_enabled: false,
                    ..Default::default()
                },
            })
            .unwrap();
        let id = source_api::wait_event(
            &mut service,
            katran::source_policy::SourceId::Sharex,
            &path,
        )["id"]
            .as_i64()
            .unwrap();
        let result = delete(&mut service, vec![id], false);
        assert_eq!(result["deleted"], 0);
        assert_eq!(result["items"][0]["ok"], false);
        assert_eq!(result["items"][0]["recycle"]["recycled"], false);
        assert!(service.test_dispatch(Command::Get { id }).is_ok());
        if replace {
            assert_eq!(std::fs::read(&path).unwrap(), b"independent replacement");
            assert_eq!(result["items"][0]["recycle"]["status"], "recovery_required");
            let recovery = result["items"][0]["recycle"]["recovery_path"]
                .as_str()
                .unwrap();
            assert_eq!(std::fs::read(recovery).unwrap(), b"synthetic original");
        } else {
            assert_eq!(result["items"][0]["recycle"]["status"], "restored");
            assert_eq!(std::fs::read(&path).unwrap(), b"synthetic original");
        }
    }
}

trait TestDispatch {
    fn test_dispatch(&mut self, command: Command) -> Result<Value, Value>;
}
impl TestDispatch for Service {
    fn test_dispatch(&mut self, command: Command) -> Result<Value, Value> {
        let response = self.execute(katran::Request {
            api_version: 1,
            request_id: "delete-test".into(),
            command,
        });
        if response["ok"] == true {
            Ok(response["result"].clone())
        } else {
            Err(response)
        }
    }
}

use katran::{Command, Service, settings::Settings};
use serde_json::Value;

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
                ..Default::default()
            },
        })
        .unwrap();
    let id = ingest(&mut service, &path, "sharex");
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

#[test]
fn single_original_deletes_immediately_and_duplicates_are_one_item() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("single.png");
    std::fs::write(&path, b"synthetic single").unwrap();
    let mut service = Service::open(&temp.path().join("shelf"), 1_000_000).unwrap();
    service
        .test_dispatch(Command::SetSettings {
            settings: Settings {
                sharex_folder: Some(temp.path().into()),
                ..Default::default()
            },
        })
        .unwrap();
    let id = ingest(&mut service, &path, "sharex");
    let result = delete(&mut service, vec![id, id], false);
    assert_eq!(result["confirmation_required"], false);
    assert_eq!(result["deleted"], 1);
    assert_eq!(result["ids"], serde_json::json!([id]));
    assert!(!path.exists());
}

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
    service
        .test_dispatch(Command::SetSettings {
            settings: Settings {
                sharex_folder: Some(folder),
                ..Default::default()
            },
        })
        .unwrap();
    let first = ingest(&mut service, &original, "sharex");
    let unsafe_id = ingest(&mut service, &outside, "sharex");
    let ordinary = ingest(&mut service, &clipboard, "test");
    let result = delete(&mut service, vec![first, unsafe_id, ordinary, 99999], true);
    assert_eq!(result["deleted"], 2);
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
                ..Default::default()
            },
        })
        .unwrap();
    let id = ingest(&mut service, &path, "sharex");
    std::fs::remove_file(path).unwrap();
    let result = delete(&mut service, vec![id], true);
    assert_eq!(result["deleted"], 1);
    assert_eq!(result["items"][0]["original_missing"], true);
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

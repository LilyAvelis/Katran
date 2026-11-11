#[cfg(windows)]
mod windows_tests {
    use katran::{Command, Request, Service};
    use serde_json::Value;
    use std::{
        path::Path,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    fn call(api: &mut Service, command: Command) -> Value {
        let response = api.execute(Request {
            api_version: 1,
            request_id: "folder-delete".into(),
            command,
        });
        assert_eq!(response["ok"], true, "{response}");
        response["result"].clone()
    }

    struct SyntheticTrash {
        expected: Mutex<Vec<Vec<u8>>>,
        fail: bool,
        calls: Arc<AtomicUsize>,
    }

    impl SyntheticTrash {
        fn new(expected: Vec<Vec<u8>>, fail: bool, calls: Arc<AtomicUsize>) -> Self {
            Self {
                expected: Mutex::new(expected),
                fail,
                calls,
            }
        }
    }

    impl katran::downloads::Trash for SyntheticTrash {
        fn recycle(&self, staged: &Path) -> katran::Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(staged.ancestors().any(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".katran-recovery-"))
            }));
            let bytes = std::fs::read(staged)?;
            let mut expected = self.expected.lock().unwrap();
            let position = expected
                .iter()
                .position(|candidate| candidate == &bytes)
                .expect("recycler received the exact synthetic original");
            expected.remove(position);
            if self.fail {
                return Err(std::io::Error::other("synthetic recycle failure").into());
            }
            std::fs::remove_file(staged)?;
            Ok(())
        }
    }

    fn browse_folder(api: &mut Service) -> Value {
        call(
            api,
            Command::BrowseShelfSorted {
                query: String::new(),
                limit: 100,
                cursor: None,
            },
        )
    }

    #[test]
    fn folder_delete_requires_exact_distinct_confirmation_and_recycles_only_files() {
        let temp = tempfile::tempdir().unwrap();
        let shelf = temp.path().join("shelf");
        let folder = temp.path().join("working");
        let subdir = folder.join("nested");
        std::fs::create_dir_all(&subdir).unwrap();
        let alpha = folder.join("alpha.txt");
        let beta = folder.join("beta.txt");
        let nested = subdir.join("kept.txt");
        std::fs::write(&alpha, b"synthetic alpha").unwrap();
        std::fs::write(&beta, b"synthetic beta").unwrap();
        std::fs::write(&nested, b"synthetic nested").unwrap();

        let calls = Arc::new(AtomicUsize::new(0));
        let mut api = Service::open(&shelf, 1_000_000).unwrap();
        api.set_trash(Box::new(SyntheticTrash::new(
            vec![b"synthetic alpha".to_vec(), b"synthetic beta".to_vec()],
            false,
            calls.clone(),
        )));
        call(
            &mut api,
            Command::ChooseFolder {
                path: folder.clone(),
            },
        );
        let page = browse_folder(&mut api);
        let events = page["events"].as_array().unwrap();
        assert_eq!(events.len(), 2, "Folder listing is nonrecursive: {page}");
        let alpha_id = events
            .iter()
            .find(|event| event["name"] == "alpha.txt")
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        let beta_id = events
            .iter()
            .find(|event| event["name"] == "beta.txt")
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        assert!(alpha_id < 0 && beta_id < 0);

        let pending = call(
            &mut api,
            Command::DeleteSelection {
                ids: vec![beta_id, alpha_id, beta_id],
                confirmed: false,
            },
        );
        assert_eq!(pending["confirmation_required"], true);
        assert_eq!(pending["ids"], serde_json::json!([beta_id, alpha_id]));
        assert_eq!(pending["deleted"], 0);
        assert_eq!(std::fs::read(&alpha).unwrap(), b"synthetic alpha");
        assert_eq!(std::fs::read(&beta).unwrap(), b"synthetic beta");
        assert!(subdir.is_dir());
        assert_eq!(std::fs::read(&nested).unwrap(), b"synthetic nested");

        let confirmed_ids: Vec<i64> = pending["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_i64().unwrap())
            .collect();
        let result = call(
            &mut api,
            Command::DeleteSelection {
                ids: confirmed_ids.clone(),
                confirmed: true,
            },
        );
        assert_eq!(result["ids"], serde_json::json!(confirmed_ids));
        assert_eq!(result["confirmation_required"], false);
        assert_eq!(result["deleted"], 2);
        assert_eq!(result["items"].as_array().unwrap().len(), 2);
        assert!(result["items"].as_array().unwrap().iter().all(|item| {
            item["ok"] == true
                && item["recycle"]["recycled"] == true
                && item["original_removed"] == true
        }));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(!alpha.exists() && !beta.exists());
        assert!(folder.is_dir() && subdir.is_dir());
        assert_eq!(std::fs::read(&nested).unwrap(), b"synthetic nested");
    }

    #[test]
    fn pinned_manual_alias_protects_same_file_in_folder() {
        let temp = tempfile::tempdir().unwrap();
        let shelf = temp.path().join("shelf");
        let folder = temp.path().join("working");
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("shared.txt");
        std::fs::write(&path, b"synthetic shared original").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));

        let mut api = Service::open(&shelf, 1_000_000).unwrap();
        api.set_trash(Box::new(SyntheticTrash::new(
            Vec::new(),
            false,
            calls.clone(),
        )));
        let alias = call(
            &mut api,
            Command::IngestPath {
                path: path.clone(),
                source: "manual".into(),
            },
        );
        let alias_id = alias["id"].as_i64().unwrap();
        call(&mut api, Command::TogglePins { ids: vec![alias_id] });
        call(
            &mut api,
            Command::ChooseFolder {
                path: folder.clone(),
            },
        );
        let page = browse_folder(&mut api);
        assert_eq!(page["events"].as_array().unwrap().len(), 1);
        let folder_id = page["events"][0]["id"].as_i64().unwrap();
        assert!(folder_id < 0);

        let result = call(
            &mut api,
            Command::DeleteSelection {
                ids: vec![folder_id],
                confirmed: false,
            },
        );
        assert_eq!(result["deleted"], 0);
        assert_eq!(result["items"][0]["skipped_pinned"], true);
        assert_eq!(result["items"][0]["status"], "skipped_pinned");
        assert_eq!(std::fs::read(&path).unwrap(), b"synthetic shared original");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn recycle_failure_restores_folder_original_and_reports_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let shelf = temp.path().join("shelf");
        let folder = temp.path().join("working");
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("restore.txt");
        std::fs::write(&path, b"synthetic restore me").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));

        let mut api = Service::open(&shelf, 1_000_000).unwrap();
        api.set_trash(Box::new(SyntheticTrash::new(
            vec![b"synthetic restore me".to_vec()],
            true,
            calls.clone(),
        )));
        call(
            &mut api,
            Command::ChooseFolder {
                path: folder.clone(),
            },
        );
        let page = browse_folder(&mut api);
        let id = page["events"][0]["id"].as_i64().unwrap();
        assert!(id < 0);

        let result = call(
            &mut api,
            Command::DeleteSelection {
                ids: vec![id],
                confirmed: false,
            },
        );
        assert_eq!(result["deleted"], 0);
        assert_eq!(result["items"][0]["ok"], false);
        assert_eq!(result["items"][0]["recycle"]["recycled"], false);
        assert_eq!(result["items"][0]["recycle"]["status"], "restored");
        assert_eq!(result["items"][0]["original_removed"], false);
        assert_eq!(std::fs::read(&path).unwrap(), b"synthetic restore me");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

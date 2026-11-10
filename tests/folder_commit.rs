#[path = "common/source_api.rs"]
mod source_api;
use katran::{
    Command, Request, Service,
    folder_watch::{FolderSource, PreparedCandidate},
};
fn call(service: &mut Service, command: Command) -> serde_json::Value {
    service.execute(Request {
        api_version: 1,
        request_id: "prepared".into(),
        command,
    })
}
#[test]
fn prepared_commits_validate_configuration_and_metadata_and_keep_sources() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("test.zip");
    std::fs::write(&path, b"synthetic archive").unwrap();
    let mut service = Service::open(data.path(), 10000).unwrap();
    let mut settings = katran::settings::Settings::defaults(10000);
    settings.downloads_folder = Some(folder.path().to_owned());
    settings.downloads_enabled = true;
    assert_eq!(
        call(&mut service, Command::SetSettings { settings })["ok"],
        true
    );
    let metadata = path.metadata().unwrap();
    let candidate = PreparedCandidate {
        source: FolderSource::Downloads,
        root: folder.path().canonicalize().unwrap(),
        path: path.clone(),
        size: metadata.len(),
        modified: metadata.modified().unwrap(),
        content_hash: katran::preview::content_hash(&path).unwrap(),
    };
    let result = call(
        &mut service,
        Command::CommitFolderCandidates {
            items: vec![candidate.clone()],
        },
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "untrusted_worker_result");
    let forged = PreparedCandidate {
        content_hash: "0".repeat(64),
        ..candidate.clone()
    };
    assert_eq!(
        call(
            &mut service,
            Command::CommitFolderCandidates {
                items: vec![forged]
            }
        )["error"]["code"],
        "untrusted_worker_result"
    );
    let history = source_api::call(
        &mut service,
        Command::Search {
            query: String::new(),
            limit: 100,
        },
    );
    assert!(
        history["events"].as_array().unwrap().is_empty(),
        "untrusted receipts must not create events"
    );
    let event = source_api::wait_event(
        &mut service,
        katran::source_policy::SourceId::Downloads,
        &path,
    );
    assert_eq!(event["revision"], format!("ref:{}", candidate.content_hash));
    assert_eq!(event["delete_policy"]["source_id"], "downloads");
    assert!(
        source_api::settle(&mut service, katran::source_policy::SourceId::Downloads).is_empty(),
        "unchanged trusted discovery must not duplicate an event"
    );
    std::fs::write(&path, b"changed original").unwrap();
    assert_eq!(
        call(
            &mut service,
            Command::CommitFolderCandidates {
                items: vec![candidate]
            }
        )["error"]["code"],
        "untrusted_worker_result"
    );
    let updated = source_api::wait_event(
        &mut service,
        katran::source_policy::SourceId::Downloads,
        &path,
    );
    assert_ne!(event["id"], updated["id"]);
    assert_eq!(
        updated["revision"],
        format!("ref:{}", katran::preview::content_hash(&path).unwrap())
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"changed original");
}

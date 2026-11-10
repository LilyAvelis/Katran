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
    assert_eq!(result["result"]["items"][0]["ok"], true);
    assert!(
        call(
            &mut service,
            Command::CommitFolderCandidates {
                items: vec![candidate.clone()]
            }
        )["result"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    std::fs::write(&path, b"changed original").unwrap();
    assert_eq!(
        call(
            &mut service,
            Command::CommitFolderCandidates {
                items: vec![candidate]
            }
        )["result"]["items"][0]["error"]["code"],
        "source_changed"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"changed original");
}

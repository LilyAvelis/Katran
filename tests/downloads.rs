use katran::{
    Command, Request, Service,
    downloads::{DownloadsScanner, Trash, recycle_original},
    settings::Settings,
};
use std::{cell::RefCell, fs, path::Path};
fn call(api: &mut Service, command: Command) -> serde_json::Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "downloads-test".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
#[test]
fn every_regular_file_type_including_empty_files_is_stable_and_temporary_files_are_skipped() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    for name in ["a.pdf", "b.exe", "nested/README", "empty.zip", "image.png"] {
        fs::write(root.path().join(name), b"").unwrap();
    }
    for name in [
        "pending.CRDownload",
        "a.part",
        "b.partial",
        "c.tmp",
        "d.download",
    ] {
        fs::write(root.path().join(name), b"pending").unwrap();
    }
    let mut scanner = DownloadsScanner::default();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert_eq!(scanner.scan(root.path()).unwrap().len(), 5);
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    fs::write(root.path().join("a.pdf"), b"synthetic revision").unwrap();
    assert!(scanner.scan(root.path()).unwrap().is_empty());
    assert_eq!(
        scanner.scan(root.path()).unwrap(),
        vec![root.path().join("a.pdf").canonicalize().unwrap()]
    );
}
#[test]
fn scan_ingests_through_api_and_receipts_survive_restart() {
    let shelf = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("synthetic.pdf"), b"synthetic PDF bytes").unwrap();
    let mut api = Service::open(shelf.path(), 100_000).unwrap();
    let mut settings = Settings::defaults(100_000);
    settings.downloads_folder = Some(folder.path().to_owned());
    settings.downloads_enabled = true;
    call(&mut api, Command::SetSettings { settings });
    assert!(
        call(&mut api, Command::ScanDownloads)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let result = call(&mut api, Command::ScanDownloads);
    assert_eq!(result["items"][0]["event"]["source"], "downloads");
    drop(api);
    let mut api = Service::open(shelf.path(), 100_000).unwrap();
    call(&mut api, Command::ScanDownloads);
    assert!(
        call(&mut api, Command::ScanDownloads)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs::read(folder.path().join("synthetic.pdf")).unwrap(),
        b"synthetic PDF bytes"
    );
}
struct FakeTrash(RefCell<Vec<std::path::PathBuf>>);
impl Trash for FakeTrash {
    fn recycle(&self, path: &Path) -> katran::api::Result<()> {
        self.0.borrow_mut().push(path.to_owned());
        Ok(())
    }
}
#[test]
fn recycling_checks_revision_and_containment_before_injected_boundary() {
    let shelf = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let path = folder.path().join("synthetic.bin");
    fs::write(&path, b"synthetic").unwrap();
    let mut api = Service::open(shelf.path(), 100_000).unwrap();
    let value = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "downloads".into(),
        },
    );
    let mut event: katran::Event = serde_json::from_value(value).unwrap();
    let trash = FakeTrash(RefCell::new(vec![]));
    assert!(!recycle_original(&event, folder.path(), &trash).unwrap());
    assert_eq!(trash.0.borrow().len(), 1);
    assert!(path.is_file());
    assert!(recycle_original(&event, outside.path(), &trash).is_err());
    fs::write(&path, b"changed synthetic bytes").unwrap();
    assert_eq!(
        recycle_original(&event, folder.path(), &trash)
            .unwrap_err()
            .code,
        "stale_selection"
    );
    event.source = "clipboard".into();
    assert!(recycle_original(&event, folder.path(), &trash).is_err());
    assert_eq!(trash.0.borrow().len(), 1);
}

use katran::{
    Command, Request, Service,
    known_folders::{KnownFolder, Provider},
};
use std::path::PathBuf;

struct Fake {
    redirected: PathBuf,
}
impl Provider for Fake {
    fn resolve(&self, folder: KnownFolder) -> Result<PathBuf, String> {
        if folder == KnownFolder::Videos {
            Err("unavailable".into())
        } else {
            Ok(self.redirected.join(format!("{folder:?}")))
        }
    }
}
fn request(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "known-folders".into(),
        command,
    })
}
#[test]
fn injected_folders_preserve_redirects_partial_failure_and_open_revalidates() {
    let root = tempfile::tempdir().unwrap();
    let redirected = tempfile::tempdir().unwrap();
    let desktop = redirected.path().join("Desktop");
    std::fs::create_dir(&desktop).unwrap();
    let mut service = Service::open(root.path(), 100_000).unwrap();
    service.set_known_folder_provider(Box::new(Fake {
        redirected: redirected.path().to_path_buf(),
    }));
    let found = request(&mut service, Command::ListKnownFolders);
    assert_eq!(found["ok"], true, "{found}");
    let items = found["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 6);
    assert_eq!(items[0]["folder"], "desktop");
    assert_eq!(items[0]["path"], desktop.to_string_lossy().as_ref());
    assert_eq!(items[4]["folder"], "videos");
    assert_eq!(items[4]["error"], "unavailable");
    assert!(items[4]["path"].is_null());
    assert_eq!(
        request(
            &mut service,
            Command::ChooseFolder {
                path: desktop.clone()
            }
        )["ok"],
        true
    );
    std::fs::remove_dir(&desktop).unwrap();
    assert_eq!(
        request(&mut service, Command::ChooseFolder { path: desktop })["ok"],
        false
    );
}

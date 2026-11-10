use katran::{
    Event,
    quicklook::{self, Launcher},
};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};
#[derive(Default)]
struct Fake(RefCell<Vec<PathBuf>>);
impl Launcher for Fake {
    fn launch(&self, path: &Path) -> katran::Result<()> {
        self.0.borrow_mut().push(path.to_owned());
        Ok(())
    }
}
fn event(path: PathBuf) -> Event {
    Event {
        delete_policy: Default::default(),
        revision: format!("ref:{}", katran::preview::content_hash(&path).unwrap()),
        origin: Default::default(),
        id: 42,
        detected_at_ms: 0,
        source: "synthetic".into(),
        name: path.file_name().unwrap().to_string_lossy().into(),
        storage: "reference".into(),
        size: 9,
        available: true,
        path,
    }
}
#[test]
fn arbitrary_formats_are_preview_requests_with_the_unchanged_path() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::default();
    for name in [
        "document space.docx",
        "document & argument.pdf",
        "never execute.exe",
        "unknown.xyz",
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, b"synthetic").unwrap();
        let result = quicklook::preview(&event(path.clone()), &fake).unwrap();
        assert_eq!(result["requested"], true);
        assert_eq!(result["backend"], "quicklook");
        assert_eq!(fake.0.borrow().last(), Some(&path));
        assert_eq!(std::fs::read(path).unwrap(), b"synthetic");
    }
    assert_eq!(fake.0.borrow().len(), 4);
}
#[test]
fn stale_missing_and_directory_sources_never_reach_launcher() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.pdf");
    std::fs::write(&path, b"synthetic").unwrap();
    let mut event = event(path.clone());
    let fake = Fake::default();
    std::fs::write(&path, b"different").unwrap();
    assert_eq!(
        quicklook::preview(&event, &fake).unwrap_err().code,
        "source_changed"
    );
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        quicklook::preview(&event, &fake).unwrap_err().code,
        "preview_unavailable"
    );
    event.path = dir.path().to_owned();
    assert_eq!(
        quicklook::preview(&event, &fake).unwrap_err().code,
        "preview_unavailable"
    );
    assert!(fake.0.borrow().is_empty());
}
#[test]
fn launcher_errors_are_not_reported_as_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.pdf");
    std::fs::write(&path, b"synthetic").unwrap();
    struct Failure;
    impl Launcher for Failure {
        fn launch(&self, _: &Path) -> katran::Result<()> {
            Err(katran::ApiError {
                code: "quicklook_not_found".into(),
                message: "synthetic failure".into(),
            })
        }
    }
    assert_eq!(
        quicklook::preview(&event(path), &Failure).unwrap_err().code,
        "quicklook_not_found"
    );
}
#[test]
fn only_internal_decoder_formats_use_native_image_preview() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic");
    std::fs::write(&path, b"synthetic").unwrap();
    let mut sample = event(path);
    for name in [
        "photo.PNG",
        "photo.jpeg",
        "photo.webp",
        "photo.gif",
        "photo.bmp",
        "photo.jpg",
    ] {
        sample.name = name.into();
        assert!(quicklook::is_native_image(&sample));
    }
    for name in [
        "document.pdf",
        "vector.svg",
        "photo.tiff",
        "installer.exe",
        "file",
    ] {
        sample.name = name.into();
        assert!(!quicklook::is_native_image(&sample));
    }
}
#[test]
fn windows_protocol_normalizes_paths_and_rejects_separators() {
    for (source, expected) in [
        (
            r"\\?\C:\space name\Документ.pdf",
            r"C:\space name\Документ.pdf",
        ),
        (r"\\?\UNC\server\share\file.pdf", r"\\server\share\file.pdf"),
        (r"C:\simple.pdf", r"C:\simple.pdf"),
    ] {
        assert_eq!(
            quicklook::protocol_path(Path::new(source)).unwrap(),
            expected
        );
        assert_eq!(
            quicklook::toggle_message(Path::new(source)).unwrap(),
            format!("QuickLook.App.PipeMessages.Toggle|{expected}|\n")
        );
    }
    for source in [
        "C:\\bad|file.pdf",
        "C:\\bad\nfile.pdf",
        "C:\\bad\rfile.pdf",
        r"\\?\Volume{synthetic}\file.pdf",
    ] {
        assert_eq!(
            quicklook::protocol_path(Path::new(source))
                .unwrap_err()
                .code,
            "invalid_argument"
        );
    }
}

use katran::{
    Command, Event, Request, Service,
    file_visual::{self, Provider, ShellVisual},
};
use std::{cell::RefCell, path::Path};

fn call(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "file-visual-test".into(),
        command,
    })
}
fn ingest(api: &mut Service, path: &Path) -> Event {
    let response = call(
        api,
        Command::IngestPath {
            path: path.into(),
            source: "synthetic".into(),
        },
    );
    assert_eq!(response["ok"], true, "{response}");
    serde_json::from_value(response["result"].clone()).unwrap()
}
struct Fake {
    calls: RefCell<Vec<ShellVisual>>,
    thumbnail: bool,
    icon: bool,
    oversized: bool,
}
impl Provider for Fake {
    fn extract(&self, _: &Path, edge: u32, kind: ShellVisual) -> Result<image::RgbaImage, String> {
        self.calls.borrow_mut().push(kind);
        if (kind == ShellVisual::Thumbnail && self.thumbnail)
            || (kind == ShellVisual::Icon && self.icon)
        {
            Ok(image::RgbaImage::from_pixel(
                if self.oversized { edge + 1 } else { edge / 2 },
                edge / 2,
                image::Rgba([3, 6, 9, 255]),
            ))
        } else {
            Err("synthetic_unavailable".into())
        }
    }
}
fn fake(thumbnail: bool, icon: bool) -> Fake {
    Fake {
        calls: RefCell::new(vec![]),
        thumbnail,
        icon,
        oversized: false,
    }
}

#[test]
fn fallback_order_cache_and_generic_are_independent_of_file_transfer() {
    for (thumbnail, icon, expected, calls) in [
        (true, true, "shell_thumbnail", vec![ShellVisual::Thumbnail]),
        (
            false,
            true,
            "shell_icon",
            vec![ShellVisual::Thumbnail, ShellVisual::Icon],
        ),
        (
            false,
            false,
            "generic_icon",
            vec![ShellVisual::Thumbnail, ShellVisual::Icon],
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("synthetic.katran-unknown");
        std::fs::write(&source, b"arbitrary file bytes").unwrap();
        let mut api = Service::open(&root.path().join("data"), 10000).unwrap();
        let event = ingest(&mut api, &source);
        let provider = fake(thumbnail, icon);
        let visual =
            file_visual::render_with_provider(&root.path().join("data"), &event, 64, &provider)
                .unwrap();
        assert_eq!(visual["visual_kind"], expected);
        assert_eq!(*provider.calls.borrow(), calls);
        assert_eq!(visual["cache_hit"], false);
        assert!(!visual["fallback_reasons"].as_array().unwrap().is_empty());
        let cached =
            file_visual::render_with_provider(&root.path().join("data"), &event, 64, &provider)
                .unwrap();
        assert_eq!(cached["cache_hit"], true);
        assert_eq!(*provider.calls.borrow(), calls);
        // Preparation creates export snapshots only; never publishes the clipboard.
        let plan =
            katran::copy_out::prepare(&root.path().join("data"), std::slice::from_ref(&event))
                .unwrap();
        assert!(plan.failures.is_empty());
        assert_eq!(
            std::fs::read(&plan.paths[0]).unwrap(),
            b"arbitrary file bytes"
        );
        let drag = call(
            &mut api,
            Command::DragEventsChecked {
                items: vec![katran::SelectionRef {
                    id: event.id,
                    revision: event.revision,
                }],
            },
        );
        assert_eq!(drag["ok"], true, "{drag}");
        call(
            &mut api,
            Command::CancelPreparedDrag {
                token: drag["result"]["drag_token"].as_str().unwrap().into(),
            },
        );
        assert_eq!(std::fs::read(source).unwrap(), b"arbitrary file bytes");
    }
}

#[test]
fn image_path_preserves_existing_decoder_and_never_calls_shell() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image.png");
    image::RgbaImage::from_pixel(200, 100, image::Rgba([12, 34, 56, 255]))
        .save(&path)
        .unwrap();
    let mut api = Service::open(&root.path().join("data"), 10000).unwrap();
    let event = ingest(&mut api, &path);
    let provider = fake(true, true);
    let visual =
        file_visual::render_with_provider(&root.path().join("data"), &event, 64, &provider)
            .unwrap();
    assert_eq!(visual["visual_kind"], "image");
    assert_eq!(visual["width"], 64);
    assert_eq!(visual["height"], 32);
    assert!(provider.calls.borrow().is_empty());
}

#[test]
fn invalid_edges_and_missing_or_changed_sources_remain_explicit_api_errors() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.txt");
    std::fs::write(&path, b"first").unwrap();
    let mut api = Service::open(&root.path().join("data"), 10000).unwrap();
    let event = ingest(&mut api, &path);
    for max_edge in [0, 31, 513, u32::MAX] {
        assert_eq!(
            call(
                &mut api,
                Command::PreviewFile {
                    id: event.id,
                    max_edge
                }
            )["error"]["code"],
            "invalid_argument"
        );
    }
    assert_eq!(
        call(
            &mut api,
            Command::PreviewFile {
                id: 999,
                max_edge: 64
            }
        )["error"]["code"],
        "not_found"
    );
    std::fs::write(&path, b"other").unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::PreviewFile {
                id: event.id,
                max_edge: 64
            }
        )["error"]["code"],
        "source_changed"
    );
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::PreviewFile {
                id: event.id,
                max_edge: 64
            }
        )["error"]["code"],
        "preview_unavailable"
    );
}

#[test]
fn malformed_provider_dimensions_fall_back_without_large_buffers() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.file");
    std::fs::write(&path, b"hello").unwrap();
    let mut api = Service::open(&root.path().join("data"), 10000).unwrap();
    let event = ingest(&mut api, &path);
    let provider = Fake {
        oversized: true,
        ..fake(true, true)
    };
    let visual =
        file_visual::render_with_provider(&root.path().join("data"), &event, 32, &provider)
            .unwrap();
    assert_eq!(visual["visual_kind"], "generic_icon");
    assert_eq!(visual["width"], 32);
    assert!(
        visual["fallback_reasons"]
            .to_string()
            .contains("invalid_dimensions")
    );
}

#[test]
fn stale_cached_sources_and_changes_during_extraction_are_rejected() {
    struct ChangesSource;
    impl Provider for ChangesSource {
        fn extract(&self, path: &Path, _: u32, _: ShellVisual) -> Result<image::RgbaImage, String> {
            std::fs::write(path, b"changed during extraction").unwrap();
            Ok(image::RgbaImage::new(32, 32))
        }
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.file");
    std::fs::write(&path, b"original").unwrap();
    let data = root.path().join("data");
    let mut api = Service::open(&data, 10000).unwrap();
    let event = ingest(&mut api, &path);
    assert_eq!(
        file_visual::render_with_provider(&data, &event, 64, &ChangesSource)
            .unwrap_err()
            .code,
        "source_changed"
    );
    std::fs::write(&path, b"original").unwrap();
    file_visual::render_with_provider(&data, &event, 64, &fake(false, true)).unwrap();
    std::fs::write(&path, b"changed cached source").unwrap();
    assert_eq!(
        file_visual::render_with_provider(&data, &event, 64, &fake(true, true))
            .unwrap_err()
            .code,
        "source_changed"
    );
}

#[test]
fn worker_api_supports_arbitrary_extensions_and_persistent_cache() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.katran-unknown");
    std::fs::write(&path, b"hello").unwrap();
    let data = root.path().join("data");
    let mut api = Service::open(&data, 10000).unwrap();
    let event = ingest(&mut api, &path);
    let id = event.id;
    let result = std::thread::spawn(move || {
        let mut api = Service::open_read_only(&data).unwrap();
        let response = call(&mut api, Command::PreviewFile { id, max_edge: 64 });
        assert_eq!(response["ok"], true, "{response}");
        assert!(matches!(
            response["result"]["visual_kind"].as_str(),
            Some("shell_thumbnail" | "shell_icon" | "generic_icon")
        ));
        let cached = call(&mut api, Command::PreviewFile { id, max_edge: 64 });
        assert_eq!(cached["result"]["cache_hit"], true);
        response
    })
    .join()
    .unwrap();
    assert_eq!(result["result"]["revision"], event.revision);
    assert_eq!(
        call(&mut api, Command::PreviewImage { id, max_edge: 64 })["ok"],
        false
    );
    assert_eq!(
        call(&mut api, Command::List { limit: 100 })["result"]["events"][0]["id"],
        id
    );
}

#[cfg(windows)]
#[test]
fn native_icon_extraction_on_worker_supports_existing_com_apartment() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.txt");
    std::fs::write(&path, b"synthetic icon extraction only").unwrap();
    std::thread::spawn(move || {
        use windows::Win32::System::Com::*;
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
        }
        let result = file_visual::NativeShell.extract(&path, 64, ShellVisual::Icon);
        unsafe {
            CoUninitialize();
        }
        let icon = result.expect("Windows text file associated icon");
        assert!(icon.width() > 0 && icon.width() <= 64 && icon.height() > 0 && icon.height() <= 64);
        assert!(icon.pixels().any(|p| p[3] > 0));
    })
    .join()
    .unwrap();
}

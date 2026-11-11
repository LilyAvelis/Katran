use katran::{
    Command, Request, Service, clipboard_files,
    interaction::{SelectionMode, ShelfTab},
};
use serde_json::Value;
use std::path::PathBuf;

struct NoClipboard;
impl clipboard_files::Source for NoClipboard {
    fn sequence(&self) -> u32 {
        panic!("Direct transfer must not read clipboard sequence")
    }
    fn files(&self) -> katran::Result<Vec<PathBuf>> {
        panic!("Direct transfer must not read clipboard files")
    }
}
fn request(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "direct".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let response = request(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn select(api: &mut Service, ids: &[i64]) {
    call(api, Command::SetVisible { ids: ids.to_vec() });
    for (index, id) in ids.iter().enumerate() {
        call(
            api,
            Command::Select {
                id: *id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
}

#[test]
fn missing_folder_keeps_selection_and_never_reads_clipboard() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "keep.png".into(),
            bytes: vec![1],
            source: "synthetic".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    select(&mut api, &[id]);
    let result = request(&mut api, Command::TransferSelection);
    assert_eq!(result["error"]["code"], "folder_not_selected");
    let state = call(&mut api, Command::Interaction);
    assert_eq!(state["selected"][0], id);
    assert_eq!(state["tab"], "images");
    assert!(std::path::Path::new(event["path"].as_str().unwrap()).exists());
}

#[cfg(windows)]
#[test]
fn direct_move_commits_verified_destination_without_clipboard() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "move.png".into(),
            bytes: vec![4, 5, 6],
            source: "synthetic".into(),
        },
    );
    let id = event["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    select(&mut api, &[id]);
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["items"][0]["status"], "moved", "{result}");
    assert_eq!(
        std::fs::read(folder.path().join("move.png")).unwrap(),
        vec![4, 5, 6]
    );
    assert_eq!(request(&mut api, Command::Get { id })["ok"], false);
}

#[test]
fn pinned_and_missing_sources_are_reported_individually() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let pinned = call(
        &mut api,
        Command::IngestBytes {
            name: "pinned.png".into(),
            bytes: vec![1],
            source: "synthetic".into(),
        },
    );
    let missing = call(
        &mut api,
        Command::IngestBytes {
            name: "missing.png".into(),
            bytes: vec![2],
            source: "synthetic".into(),
        },
    );
    let pinned_id = pinned["id"].as_i64().unwrap();
    let missing_id = missing["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![pinned_id],
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    select(&mut api, &[pinned_id, missing_id]);
    std::fs::remove_file(missing["path"].as_str().unwrap()).unwrap();
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["direction"], "to_folder");
    assert_eq!(result["interaction"]["tab"], "folder");
    assert_eq!(result["items"][0]["status"], "pinned_copy");
    assert_eq!(result["items"][1]["status"], "failed");
    assert_eq!(
        call(&mut api, Command::Get { id: pinned_id })["id"],
        pinned_id
    );
}

#[test]
fn folder_copy_reports_unsupported_and_stale_and_preserves_folder_context() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("note.txt", "text"),
        ("unknown.pdf", "pdf"),
        ("stale.txt", "old"),
    ] {
        std::fs::write(folder.path().join(name), contents).unwrap();
    }
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let lookup = |name: &str| {
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["name"] == name)
            .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    let ids = [
        lookup("note.txt"),
        lookup("unknown.pdf"),
        lookup("stale.txt"),
    ];
    select(&mut api, &ids);
    std::fs::write(folder.path().join("stale.txt"), b"changed length").unwrap();
    let result = call(&mut api, Command::CopySelection);
    assert_eq!(result["direction"], "from_folder");
    assert_eq!(result["interaction"]["tab"], "folder");
    assert_eq!(result["items"][0]["event"]["kind"], "text");
    assert_eq!(result["items"][1]["error"]["code"], "unsupported_import");
    assert_eq!(result["items"][2]["error"]["code"], "stale_selection");
    assert_eq!(
        std::fs::read_to_string(folder.path().join("note.txt")).unwrap(),
        "text"
    );
}

#[cfg(windows)]
#[test]
fn folder_copy_stays_selected_and_move_imports_managed_image() {
    struct SyntheticTrash;
    impl katran::downloads::Trash for SyntheticTrash {
        fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
            assert_eq!(
                path.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str()),
                Some("data")
            );
            std::fs::remove_file(path)?;
            Ok(())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("example.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([30, 50, 70, 255]))
        .save(&path)
        .unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    api.set_trash(Box::new(SyntheticTrash));
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let id = page["events"][0]["id"].as_i64().unwrap();
    select(&mut api, &[id]);
    for _ in 0..2 {
        let result = call(&mut api, Command::CopySelection);
        assert_eq!(result["interaction"]["tab"], "folder");
        assert_eq!(result["interaction"]["selected"][0], id);
        assert_eq!(result["items"][0]["event"]["event"]["storage"], "cached");
        assert_eq!(
            result["items"][0]["event"]["event"]["modified_at_ms"],
            page["events"][0]["modified_at_ms"]
        );
        assert!(path.exists());
    }
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["items"][0]["status"], "moved", "{result}");
    assert_eq!(result["items"][0]["event"]["event"]["storage"], "cached");
    assert_eq!(result["interaction"]["tab"], "images");
    assert!(!path.exists());
}

#[cfg(windows)]
#[test]
fn images_move_across_three_folders_via_managed_imports() {
    struct SyntheticTrash;
    impl katran::downloads::Trash for SyntheticTrash {
        fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
            assert_eq!(
                path.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str()),
                Some("data")
            );
            std::fs::remove_file(path)?;
            Ok(())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let folders: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    api.set_trash(Box::new(SyntheticTrash));
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        4,
        3,
        image::Rgba([40, 60, 80, 255]),
    ))
    .write_to(&mut encoded, image::ImageFormat::Png)
    .unwrap();
    let event = call(
        &mut api,
        Command::IngestBytes {
            name: "roundtrip.png".into(),
            bytes: encoded.into_inner(),
            source: "synthetic".into(),
        },
    );
    let mut id = event["id"].as_i64().unwrap();
    for folder in &folders {
        call(
            &mut api,
            Command::ChooseFolder {
                path: folder.path().into(),
            },
        );
        call(
            &mut api,
            Command::SetTab {
                tab: ShelfTab::Images,
            },
        );
        select(&mut api, &[id]);
        let moved = call(&mut api, Command::TransferSelection);
        assert_eq!(moved["items"][0]["status"], "moved", "{moved}");
        let path = folder.path().join("roundtrip.png");
        assert!(path.exists());
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 100,
                cursor: None,
            },
        );
        let folder_id = page["events"][0]["id"].as_i64().unwrap();
        select(&mut api, &[folder_id]);
        let imported = call(&mut api, Command::TransferSelection);
        assert_eq!(imported["items"][0]["status"], "moved", "{imported}");
        id = imported["items"][0]["event"]["event"]["id"]
            .as_i64()
            .unwrap();
        assert!(!path.exists());
    }
}

#[cfg(windows)]
#[test]
fn folder_move_retains_pinned_original_and_rejects_managed_source() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("pinned.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([3, 5, 7, 255]))
        .save(&path)
        .unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let reference = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "synthetic".into(),
        },
    );
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![reference["id"].as_i64().unwrap()],
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    select(&mut api, &[page["events"][0]["id"].as_i64().unwrap()]);
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["items"][0]["status"], "pinned_copy", "{result}");
    assert!(path.exists());
    let managed = root.path().join("synthetic.png");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([20, 30, 40, 255]))
        .save(&managed)
        .unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: root.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let event = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "synthetic.png")
        .unwrap();
    select(&mut api, &[event["id"].as_i64().unwrap()]);
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["items"][0]["status"], "source_not_moved", "{result}");
    assert_eq!(result["items"][0]["error"]["code"], "managed_source");
    assert!(managed.exists());
}

#[cfg(windows)]
#[test]
fn batch_can_repeat_moves_through_different_folders_after_return_import() {
    struct Trash;
    impl katran::downloads::Trash for Trash {
        fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
            std::fs::remove_file(path)?;
            Ok(())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let folders = [
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    ];
    let mut api = Service::open(root.path(), 1000000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    api.set_trash(Box::new(Trash));
    let mut ids = Vec::new();
    for n in 0..2 {
        let path = source.path().join(format!("batch{n}.png"));
        image::RgbaImage::from_pixel(4, 3, image::Rgba([n as u8, 60, 90, 255]))
            .save(&path)
            .unwrap();
        ids.push(
            call(
                &mut api,
                Command::IngestBytes {
                    name: format!("batch{n}.png"),
                    bytes: std::fs::read(path).unwrap(),
                    source: "synthetic".into(),
                },
            )["id"]
                .as_i64()
                .unwrap(),
        );
    }
    for folder in &folders {
        call(
            &mut api,
            Command::ChooseFolder {
                path: folder.path().into(),
            },
        );
        call(
            &mut api,
            Command::SetTab {
                tab: ShelfTab::Images,
            },
        );
        select(&mut api, &ids);
        let moved = call(&mut api, Command::TransferSelection);
        assert!(
            moved["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["status"] == "moved"),
            "{moved}"
        );
        let page = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 100,
                cursor: None,
            },
        );
        let folder_ids = page["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_i64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(folder_ids.len(), 2);
        select(&mut api, &folder_ids);
        let imported = call(&mut api, Command::TransferSelection);
        assert!(
            imported["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["status"] == "moved"),
            "{imported}"
        );
        ids = imported["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["event"]["event"]["id"].as_i64().unwrap())
            .collect();
        assert!(
            std::fs::read_dir(folder.path())
                .unwrap()
                .filter_map(|e| e.ok())
                .all(|e| !e.path().is_file())
        );
    }
}
#[cfg(windows)]
#[test]
fn explicit_move_allows_legacy_reference_but_clipboard_cut_keeps_old_protection() {
    struct Clipboard;
    impl clipboard_files::Source for Clipboard {
        fn sequence(&self) -> u32 {
            7
        }
        fn files(&self) -> katran::Result<Vec<PathBuf>> {
            Ok(vec![])
        }
    }
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = source.path().join("legacy.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([3, 4, 5, 255]))
        .save(&path)
        .unwrap();
    let mut api = Service::open(root.path(), 1000000).unwrap();
    api.set_clipboard_files_source(Box::new(Clipboard));
    let e = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "synthetic".into(),
        },
    );
    let id = e["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::StageTransfer {
            items: vec![katran::SelectionRef {
                id,
                revision: e["revision"].as_str().unwrap().into(),
            }],
            cut: true,
        },
    );
    let old = call(&mut api, Command::PasteToFolder);
    assert_eq!(old["items"][0]["error"]["code"], "source_not_movable");
    assert!(path.exists());
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    select(&mut api, &[id]);
    let result = call(&mut api, Command::TransferSelection);
    assert_eq!(result["items"][0]["status"], "moved", "{result}");
    assert!(!path.exists());
    assert!(std::path::Path::new(result["items"][0]["path"].as_str().unwrap()).exists());
}

#[cfg(windows)]
#[test]
fn legacy_alias_of_pinned_managed_blob_cannot_be_moved_or_removed() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = source.path().join("legacy.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([4, 5, 6, 255]))
        .save(&path)
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut api = Service::open(root.path(), 1000000).unwrap();
    api.set_clipboard_files_source(Box::new(NoClipboard));
    let owned = call(
        &mut api,
        Command::IngestBytes {
            name: "owned.png".into(),
            bytes: bytes.clone(),
            source: "synthetic".into(),
        },
    );
    let owned_id = owned["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::TogglePins {
            ids: vec![owned_id],
        },
    );
    let legacy = call(
        &mut api,
        Command::IngestPath {
            path,
            source: "legacy".into(),
        },
    );
    let legacy_id = legacy["id"].as_i64().unwrap();
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute(
        "UPDATE events SET path=?1 WHERE id=?2",
        rusqlite::params![owned["path"].as_str().unwrap(), legacy_id],
    )
    .unwrap();
    drop(db);
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    select(&mut api, &[legacy_id]);
    let moved = call(&mut api, Command::TransferSelection);
    assert_eq!(
        moved["items"][0]["error"]["code"], "managed_source",
        "{moved}"
    );
    assert_eq!(
        std::fs::read(owned["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    assert_eq!(
        call(&mut api, Command::Get { id: owned_id })["available"],
        true
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    select(&mut api, &[legacy_id]);
    let copied = call(&mut api, Command::CopySelection);
    assert_eq!(copied["items"][0]["status"], "copied");
    assert_eq!(
        call(&mut api, Command::Get { id: owned_id })["available"],
        true
    );
}

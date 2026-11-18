use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
    open_file::Operation,
};
use serde_json::{Value, json};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "bs-fixture".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let response = reply(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn finish(api: &mut Service, command: Command) -> Value {
    let job = JobId(call(api, command)["job_id"].as_u64().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = call(api, Command::PollJob { job_id: job });
        match result["state"].as_str() {
            Some("succeeded") => return result["result"].clone(),
            Some("queued" | "running" | "ready") if Instant::now() < deadline => {}
            _ => panic!("{result}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn id(event: &Value) -> i64 {
    event["id"].as_i64().unwrap()
}
fn revision(event: &Value) -> String {
    event["revision"].as_str().unwrap().to_owned()
}
fn files(api: &mut Service, tab: ShelfTab) -> Vec<i64> {
    call(
        api,
        Command::BrowseTabSnapshot {
            tab,
            query: String::new(),
            limit: 20,
        },
    )["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(id)
        .collect()
}
fn select(api: &mut Service, ids: &[i64]) {
    call(api, Command::SetVisible { ids: ids.to_vec() });
    for (index, &id) in ids.iter().enumerate() {
        call(
            api,
            Command::Select {
                id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            },
        );
    }
}
fn action(api: &mut Service) -> Value {
    finish(
        api,
        Command::RunAction {
            id: "create_bs_collection".into(),
        },
    )
}

fn unified(api: &mut Service) -> Value {
    finish(
        api,
        Command::RunAction {
            id: "create_image_collection".into(),
        },
    )
}
fn add(api: &mut Service, paths: Vec<PathBuf>) -> Vec<i64> {
    call(api, Command::AddFiles { paths })["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            assert_eq!(item["ok"], true, "{item}");
            id(&item["event"])
        })
        .collect()
}

#[test]
fn mixed_collection_packs_and_unpacks_more_than_32_files() {
    let profile = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Files,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    let paths: Vec<_> = (0..33)
        .map(|index| {
            let name = format!(
                "item-{index:02}.{}",
                if index % 2 == 0 { "txt" } else { "bin" }
            );
            let path = originals.path().join(name);
            std::fs::write(&path, format!("fixture-{index:02}")).unwrap();
            path
        })
        .collect();
    let ids = add(&mut api, paths.clone());
    select(&mut api, &ids);
    let packed = action(&mut api);
    assert_eq!(packed["count"], 33);
    assert_eq!(packed["bs_collection"], true);
    let card = &packed["event"];
    let entries =
        baloh_collection::bs::read(std::fs::File::open(card["path"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(entries.len(), 33);
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry.name,
            paths[index].file_name().unwrap().to_string_lossy()
        );
        assert_eq!(entry.bytes, format!("fixture-{index:02}").as_bytes());
    }
    select(&mut api, &[id(card)]);
    let unpacked = action(&mut api);
    assert_eq!(unpacked["items"].as_array().unwrap().len(), 33);
    for path in paths {
        assert!(path.exists());
    }
}

type Opened = (String, Vec<u8>, Operation);
struct Launcher(Arc<Mutex<Vec<Opened>>>);
impl katran::open_file::Launcher for Launcher {
    fn launch(&self, path: &Path, operation: Operation) -> katran::Result<()> {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.0
            .lock()
            .unwrap()
            .push((name, std::fs::read(path)?, operation));
        Ok(())
    }
}

#[test]
fn heterogeneous_bs_packs_merges_previews_and_restores_without_touching_disk_sources() {
    let profile = tempfile::tempdir().unwrap();
    let originals = tempfile::tempdir().unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Files,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    let png = {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(26, 20)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    };
    let paths: Vec<_> = [
        ("photo.png", png.as_slice()),
        ("memo.txt", b"archive source"),
        ("fixture.cmd", b"echo synthetic"),
    ]
    .into_iter()
    .map(|(name, bytes)| {
        let path = originals.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    })
    .collect();
    let ids = add(&mut api, paths.clone());
    select(&mut api, &[ids[1], ids[0]]);
    assert!(
        call(&mut api, Command::Actions)
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == "create_image_collection")
    );
    let first = action(&mut api)["event"].clone();
    assert!(files(&mut api, ShelfTab::Images).is_empty());
    assert_eq!(files(&mut api, ShelfTab::Files).len(), 2);
    select(&mut api, &[ids[2]]);
    let second = action(&mut api)["event"].clone();
    select(&mut api, &[id(&first), id(&second)]);
    let merged = action(&mut api);
    assert_eq!(merged["bs_collection"], true);
    assert_eq!(merged["merged"], true);
    assert_eq!(merged["count"], 3);
    let card = &merged["event"];
    assert_eq!(files(&mut api, ShelfTab::Files), vec![id(card)]);
    let entries =
        baloh_collection::bs::read(std::fs::File::open(card["path"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["memo.txt", "photo.png", "fixture.cmd"]
    );
    assert_eq!(entries[1].bytes, png);
    let export = katran::copy_out::prepare_with_budget(
        &profile.path().join("bs-test-export"),
        &[serde_json::from_value(card.clone()).unwrap()],
        katran::DEFAULT_QUOTA,
    )
    .unwrap();
    assert_eq!(export.paths.len(), 1);
    assert_eq!(export.paths[0].extension().unwrap(), "bs");
    let page = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: id(card),
            revision: revision(card),
            index: 1,
        },
    );
    assert_eq!(page["file"]["name"], "photo.png");
    assert!(
        page["file"]["preview_png"]
            .as_str()
            .is_some_and(|preview| !preview.is_empty())
    );
    assert_eq!(
        reply(
            &mut api,
            Command::CopyCollectionPage {
                id: id(card),
                revision: revision(card),
                index: 1
            }
        )["ok"],
        false
    );

    let opened = Arc::new(Mutex::new(Vec::new()));
    api.set_file_launcher(Box::new(Launcher(opened.clone())));
    assert_eq!(
        finish(
            &mut api,
            Command::OpenBsEntry {
                id: id(card),
                revision: revision(card),
                index: 0
            }
        )["opened_bs_entry"],
        true
    );
    assert_eq!(
        finish(
            &mut api,
            Command::OpenBsEntry {
                id: id(card),
                revision: revision(card),
                index: 2
            }
        )["revealed"],
        true
    );
    assert_eq!(
        opened.lock().unwrap().as_slice(),
        &[
            (
                "memo.txt".into(),
                b"archive source".to_vec(),
                Operation::Open
            ),
            (
                "fixture.cmd".into(),
                b"echo synthetic".to_vec(),
                Operation::Reveal
            )
        ]
    );

    let other = originals.path().join("other.bull");
    std::fs::write(&other, b"portable marker fixture").unwrap();
    let typed = add(&mut api, vec![other])[0];
    select(&mut api, &[id(card), typed]);
    assert_eq!(reply(&mut api, Command::CreateBsCollection)["ok"], false);
    assert_eq!(reply(&mut api, Command::CreateImageCollection)["ok"], false);

    std::fs::write(&paths[1], b"edited separately").unwrap();
    select(&mut api, &[id(card)]);
    assert_eq!(action(&mut api)["items"], json!([ids[1], ids[0], ids[2]]));
    let recovered = call(&mut api, Command::Get { id: ids[1] });
    assert_eq!(recovered["source"], "bs_extracted_file");
    assert_eq!(
        std::fs::read(recovered["path"].as_str().unwrap()).unwrap(),
        b"archive source"
    );
    assert_eq!(std::fs::read(&paths[1]).unwrap(), b"edited separately");
    assert_eq!(files(&mut api, ShelfTab::Files).len(), 4);

    let received_path = originals.path().join("received.bs");
    std::fs::copy(&export.paths[0], &received_path).unwrap();
    let received = add(&mut api, vec![received_path])[0];
    let event = call(&mut api, Command::Get { id: received });
    let page = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: received,
            revision: revision(&event),
            index: 0,
        },
    );
    assert_eq!(page["file"]["name"], "memo.txt");
}

#[test]
fn one_action_packs_homogeneous_files_into_bull_and_deletes_hidden_shelf_members() {
    let profile = tempfile::tempdir().unwrap();
    let disk = tempfile::tempdir().unwrap();
    let a = disk.path().join("a.mp4");
    let b = disk.path().join("b.webm");
    std::fs::write(&a, b"a fixture").unwrap();
    std::fs::write(&b, b"b fixture").unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Files,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    let ids = add(&mut api, vec![a.clone(), b.clone()]);
    select(&mut api, &ids);
    let actions = call(&mut api, Command::Actions);
    let actions = actions.as_array().unwrap();
    assert!(
        actions
            .iter()
            .any(|action| action["id"] == "create_image_collection")
    );
    assert!(
        !actions
            .iter()
            .any(|action| action["id"] == "create_bs_collection")
    );
    let result = unified(&mut api);
    let card = &result["event"];
    assert_eq!(card["source"], "file_bull_collection");
    let target = call(&mut api, Command::CollectionRenameInfo { id: id(card) });
    let renamed = finish(
        &mut api,
        Command::RenameCollection {
            id: id(card),
            revision: revision(&target),
            name: "Моя коллекция".into(),
        },
    );
    assert_eq!(renamed["event"]["name"], "Моя коллекция.bull");
    assert_eq!(renamed["event"]["revision"], card["revision"]);
    assert_eq!(
        reply(
            &mut api,
            Command::RenameCollection {
                id: id(card),
                revision: revision(&target),
                name: "Stale".into()
            }
        )["ok"],
        false
    );
    let page = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: id(card),
            revision: revision(card),
            index: 0,
        },
    );
    assert_eq!(page["file"]["name"], "a.mp4");
    assert_eq!(page["file"]["archive_kind"], "bull_file");
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![id(card)],
            confirmed: false,
        },
    );
    assert!(files(&mut api, ShelfTab::Files).is_empty());
    for id in ids {
        assert_eq!(reply(&mut api, Command::Get { id })["ok"], false);
    }
    assert_eq!(std::fs::read(a).unwrap(), b"a fixture");
    assert_eq!(std::fs::read(b).unwrap(), b"b fixture");
}

#[cfg(windows)]
type RecycledFile = (String, Vec<u8>);
#[cfg(windows)]
struct SyntheticFolderTrash {
    fail_name: Option<String>,
    seen: Arc<Mutex<Vec<RecycledFile>>>,
}
#[cfg(windows)]
impl katran::downloads::Trash for SyntheticFolderTrash {
    fn recycle(&self, path: &Path) -> katran::Result<()> {
        assert!(
            path.parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .is_some_and(|name| name.to_string_lossy().starts_with(".katran-recovery-"))
        );
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.seen
            .lock()
            .unwrap()
            .push((name.clone(), std::fs::read(path)?));
        if self.fail_name.as_deref() == Some(name.as_str()) {
            return Err(katran::ApiError {
                code: "fixture_recycle_failure".into(),
                message: "synthetic recycle failure".into(),
            });
        }
        std::fs::remove_file(path)?;
        Ok(())
    }
}

#[cfg(windows)]
#[test]
fn folder_packs_and_unpacks_in_place_with_verified_recycle() {
    let profile = tempfile::tempdir().unwrap();
    let disk = tempfile::tempdir().unwrap();
    let a = disk.path().join("first.txt");
    let b = disk.path().join("second.bin");
    std::fs::write(&a, b"text fixture").unwrap();
    std::fs::write(&b, [0, 4, 9]).unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    api.set_trash(Box::new(SyntheticFolderTrash {
        fail_name: None,
        seen: seen.clone(),
    }));
    call(
        &mut api,
        Command::ChooseFolder {
            path: disk.path().into(),
        },
    );
    let browse = |api: &mut Service| {
        call(
            api,
            Command::BrowseShelfSorted {
                query: String::new(),
                limit: 20,
                cursor: None,
            },
        )
    };
    let page = browse(&mut api);
    let ids: Vec<_> = page["events"].as_array().unwrap().iter().map(id).collect();
    select(&mut api, &ids);
    let result = unified(&mut api);
    assert_eq!(result["kind"], "bs");
    let archive = PathBuf::from(result["path"].as_str().unwrap());
    assert_eq!(result["archive_verified"], true);
    assert_eq!(result["sources_remaining"], 0);
    assert!(archive.exists() && !a.exists() && !b.exists());
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .any(|(name, bytes)| name == "first.txt" && bytes == b"text fixture")
    );
    assert!(
        seen.iter()
            .any(|(name, bytes)| name == "second.bin" && bytes == &[0, 4, 9])
    );
    drop(seen);
    let page = browse(&mut api);
    let selected = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["name"]
                .as_str()
                .is_some_and(|name| name.ends_with(".bs"))
        })
        .unwrap();
    let target = call(&mut api, Command::CollectionRenameInfo { id: id(selected) });
    let before_rename = std::fs::read(&archive).unwrap();
    let renamed = finish(
        &mut api,
        Command::RenameCollection {
            id: id(selected),
            revision: revision(&target),
            name: "Project files".into(),
        },
    );
    assert!(!archive.exists());
    let archive = PathBuf::from(renamed["path"].as_str().unwrap());
    assert_eq!(archive.file_name().unwrap(), "Project files.bs");
    assert_eq!(std::fs::read(&archive).unwrap(), before_rename);
    let page = browse(&mut api);
    let selected = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == "Project files.bs")
        .unwrap();
    let page = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: id(selected),
            revision: revision(selected),
            index: 0,
        },
    );
    assert_eq!(page["count"], 2);
    // A new unrelated file with the original name must never be overwritten.
    std::fs::write(&a, b"existing collision").unwrap();
    select(&mut api, &[id(selected)]);
    let extracted = unified(&mut api);
    assert_eq!(extracted["unpacked"], true);
    let destination = PathBuf::from(extracted["path"].as_str().unwrap());
    assert_eq!(
        destination.canonicalize().unwrap(),
        disk.path().canonicalize().unwrap()
    );
    assert_eq!(
        std::fs::read(destination.join("first (2).txt")).unwrap(),
        b"text fixture"
    );
    assert_eq!(
        std::fs::read(destination.join("second.bin")).unwrap(),
        [0, 4, 9]
    );
    assert_eq!(std::fs::read(&a).unwrap(), b"existing collision");
    assert!(!archive.exists());
    assert_eq!(extracted["publication_complete"], true);
    assert_eq!(extracted["published_paths"].as_array().unwrap().len(), 2);
    assert_eq!(extracted["sources_remaining"], 0);
}

#[cfg(windows)]
#[test]
fn folder_recycle_failure_keeps_verified_archive_and_reports_retained_original() {
    let profile = tempfile::tempdir().unwrap();
    let disk = tempfile::tempdir().unwrap();
    let a = disk.path().join("first.txt");
    let b = disk.path().join("second.bin");
    std::fs::write(&a, b"text fixture").unwrap();
    std::fs::write(&b, b"second fixture").unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    api.set_trash(Box::new(SyntheticFolderTrash {
        fail_name: Some("second.bin".into()),
        seen: Arc::new(Mutex::new(Vec::new())),
    }));
    call(
        &mut api,
        Command::ChooseFolder {
            path: disk.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 20,
            cursor: None,
        },
    );
    let ids: Vec<_> = page["events"].as_array().unwrap().iter().map(id).collect();
    select(&mut api, &ids);
    let result = unified(&mut api);
    assert_eq!(result["archive_verified"], true);
    assert_eq!(result["sources_remaining"], 1);
    assert!(!a.exists());
    assert_eq!(std::fs::read(&b).unwrap(), b"second fixture");
    let entries =
        baloh_collection::bs::read(std::fs::File::open(result["path"].as_str().unwrap()).unwrap())
            .unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.name == "first.txt" && e.bytes == b"text fixture")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.name == "second.bin" && e.bytes == b"second fixture")
    );
    let receipts = result["original_removal"].as_array().unwrap();
    assert_eq!(receipts.iter().filter(|r| r["ok"] == false).count(), 1);
    assert!(
        receipts
            .iter()
            .any(|r| r["status"] == "restored" && r["original_removed"] == false)
    );
}

#[cfg(windows)]
#[test]
fn folder_images_archive_preserves_names_and_exact_bytes() {
    let profile = tempfile::tempdir().unwrap();
    let disk = tempfile::tempdir().unwrap();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(7, 5)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(6, 4)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    std::fs::write(disk.path().join("alpha.png"), png.get_ref()).unwrap();
    std::fs::write(disk.path().join("beta.jpg"), jpeg.get_ref()).unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    api.set_trash(Box::new(SyntheticFolderTrash {
        fail_name: None,
        seen: Arc::new(Mutex::new(Vec::new())),
    }));
    call(
        &mut api,
        Command::ChooseFolder {
            path: disk.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 20,
            cursor: None,
        },
    );
    let ids: Vec<_> = page["events"].as_array().unwrap().iter().map(id).collect();
    select(&mut api, &ids);
    let result = unified(&mut api);
    assert_eq!(result["archive_verified"], true);
    assert_eq!(result["sources_remaining"], 0);
    let archive = PathBuf::from(result["path"].as_str().unwrap());
    assert_eq!(archive.extension().unwrap(), "bull");
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 20,
            cursor: None,
        },
    );
    let card = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"].as_str().is_some_and(|p| Path::new(p) == archive))
        .unwrap();
    let result = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: id(card),
            revision: revision(card),
            index: 0,
        },
    );
    assert!(result["png"].as_str().is_some_and(|png| !png.is_empty()));
    let cover = call(
        &mut api,
        Command::PreviewPath {
            path: archive.clone(),
            revision: revision(card),
            max_edge: 64,
            image_only: false,
        },
    );
    assert_eq!(cover["visual_kind"], "image");
    select(&mut api, &[id(card)]);
    let extracted = unified(&mut api);
    let folder = PathBuf::from(extracted["path"].as_str().unwrap());
    assert_eq!(
        folder.canonicalize().unwrap(),
        disk.path().canonicalize().unwrap()
    );
    assert_eq!(
        std::fs::read(folder.join("alpha.png")).unwrap(),
        *png.get_ref()
    );
    assert_eq!(
        std::fs::read(folder.join("beta.jpg")).unwrap(),
        *jpeg.get_ref()
    );
    assert!(!archive.exists());
    assert_eq!(extracted["sources_remaining"], 0);
}

#[cfg(windows)]
#[test]
fn folder_unpack_recycle_failure_keeps_archive_and_reports_published_files() {
    let profile = tempfile::tempdir().unwrap();
    let disk = tempfile::tempdir().unwrap();
    let archive = disk.path().join("fixture.bs");
    let entries = vec![
        baloh_collection::bs::FileEntry {
            name: "first.txt".into(),
            bytes: b"one".to_vec(),
        },
        baloh_collection::bs::FileEntry {
            name: "second.bin".into(),
            bytes: vec![0, 1, 2],
        },
    ];
    let mut output = std::fs::File::create(&archive).unwrap();
    baloh_collection::bs::write(&mut output, &entries).unwrap();
    drop(output);
    let original = std::fs::read(&archive).unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
    api.set_trash(Box::new(SyntheticFolderTrash {
        fail_name: Some("fixture.bs".into()),
        seen: Arc::new(Mutex::new(Vec::new())),
    }));
    call(
        &mut api,
        Command::ChooseFolder {
            path: disk.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 20,
            cursor: None,
        },
    );
    let id = id(&page["events"][0]);
    select(&mut api, &[id]);
    let result = unified(&mut api);
    assert_eq!(result["unpacked"], true);
    assert_eq!(result["publication_complete"], false);
    assert_eq!(result["files_published"], true);
    assert_eq!(result["archive_unchanged"], true);
    assert_eq!(result["sources_remaining"], 1);
    assert_eq!(std::fs::read(&archive).unwrap(), original);
    assert_eq!(
        std::fs::read(disk.path().join("first.txt")).unwrap(),
        b"one"
    );
    assert_eq!(
        std::fs::read(disk.path().join("second.bin")).unwrap(),
        [0, 1, 2]
    );
    assert_eq!(result["original_removal"][0]["status"], "restored");
}

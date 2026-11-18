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
#[test]
fn folder_uses_same_action_and_publishes_collections_alongside_originals() {
    let profile = tempfile::tempdir().unwrap();
    let disk = tempfile::tempdir().unwrap();
    let a = disk.path().join("first.txt");
    let b = disk.path().join("second.bin");
    std::fs::write(&a, b"text fixture").unwrap();
    std::fs::write(&b, [0, 4, 9]).unwrap();
    let mut api = Service::open(profile.path(), katran::DEFAULT_QUOTA).unwrap();
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
    assert!(archive.exists() && a.exists() && b.exists());
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
    let page = finish(
        &mut api,
        Command::TranslationCollectionPage {
            id: id(selected),
            revision: revision(selected),
            index: 0,
        },
    );
    assert_eq!(page["count"], 2);
    select(&mut api, &[id(selected)]);
    let extracted = unified(&mut api);
    assert_eq!(extracted["unpacked"], true);
    let destination = PathBuf::from(extracted["path"].as_str().unwrap());
    assert_eq!(
        std::fs::read(destination.join("first.txt")).unwrap(),
        b"text fixture"
    );
    assert_eq!(
        std::fs::read(destination.join("second.bin")).unwrap(),
        [0, 4, 9]
    );
    assert!(archive.exists());
}

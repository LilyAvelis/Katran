use katran::{Command, Request, SelectionRef, Service, settings::Settings};
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "owned-timestamps".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn selection(event: &Value) -> SelectionRef {
    SelectionRef {
        id: event["id"].as_i64().unwrap(),
        revision: event["revision"].as_str().unwrap().into(),
    }
}

fn modified_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[test]
fn owned_exports_use_arrival_time_and_external_file_copies_keep_source_time() {
    let temp = tempfile::tempdir().unwrap();
    let shelf = temp.path().join("shelf");
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();

    let mut api = Service::open(&shelf, 1_000_000).unwrap();
    let mut settings = Settings::defaults(1_000_000);
    settings.downloads_enabled = false;
    settings.text_enabled = true;
    settings.links_enabled = true;
    settings.tables_enabled = true;
    call(&mut api, Command::SetSettings { settings });

    let image = call(
        &mut api,
        Command::IngestBytes {
            name: "owned.png".into(),
            bytes: b"synthetic image bytes".to_vec(),
            source: "synthetic".into(),
        },
    );
    let text = call(
        &mut api,
        Command::IngestText {
            text: "synthetic owned text".into(),
            source: "synthetic".into(),
        },
    );
    let link = call(
        &mut api,
        Command::IngestLink {
            url: "https://example.test/synthetic".into(),
            source: "synthetic".into(),
        },
    );
    let table = call(
        &mut api,
        Command::IngestTable {
            rows: vec![vec!["synthetic".into(), "value".into()]],
            source: "synthetic".into(),
        },
    );
    let owned = [&image, &text, &link, &table];
    for event in owned {
        assert_eq!(event["modified_at_ms"], event["detected_at_ms"]);
    }

    call(
        &mut api,
        Command::StageTransfer {
            items: owned.iter().map(|event| selection(event)).collect(),
            cut: false,
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: target.clone(),
        },
    );
    let pasted = call(&mut api, Command::PasteToFolder);
    let items = pasted["items"].as_array().unwrap();
    assert_eq!(items.len(), owned.len(), "{pasted}");
    for (event, item) in owned.iter().zip(items) {
        assert_eq!(item["status"], "copied", "{item}");
        let path = Path::new(item["path"].as_str().unwrap());
        assert_eq!(modified_ms(path), event["detected_at_ms"].as_i64().unwrap());
    }

    let external = temp.path().join("external.txt");
    std::fs::write(&external, b"synthetic external file").unwrap();
    let old_time = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    std::fs::File::options()
        .write(true)
        .open(&external)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old_time))
        .unwrap();
    let original_time_ms = modified_ms(&external);
    assert!(original_time_ms < image["detected_at_ms"].as_i64().unwrap());

    let external_paste = call(
        &mut api,
        Command::PasteFilesToFolder {
            paths: vec![external.clone()],
        },
    );
    assert_eq!(external_paste["items"][0]["status"], "copied");
    let copied = Path::new(external_paste["items"][0]["path"].as_str().unwrap());
    assert_eq!(
        std::fs::read(&external).unwrap(),
        b"synthetic external file"
    );
    assert_eq!(modified_ms(&external), original_time_ms);
    assert_eq!(modified_ms(copied), original_time_ms);
}

use katran::{
    Command, DEFAULT_QUOTA, Request, Service,
    interaction::{SelectionMode, ShelfTab},
    sorting::SortMode,
};
use serde_json::{Value, json};
fn call(api: &mut Service, command: Command) -> Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "sort-test".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
fn sort(api: &mut Service, mode: SortMode, reverse: bool) {
    call(
        api,
        Command::SetSort {
            mode,
            reverse,
            seed: Some(42),
        },
    );
}
fn page(api: &mut Service, query: &str, limit: u32, cursor: Option<String>) -> Value {
    call(
        api,
        Command::BrowseShelfSorted {
            query: query.into(),
            limit,
            cursor,
        },
    )
}
fn names(value: &Value) -> Vec<String> {
    value["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().into())
        .collect()
}
fn fixture() -> (tempfile::TempDir, Service) {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    for (name, size) in [
        ("file10.webp", 10),
        ("file2.PNG", 2),
        ("file1.jpg", 1),
        ("other.gif", 5),
    ] {
        call(
            &mut api,
            Command::IngestBytes {
                name: name.into(),
                bytes: vec![b'x'; size],
                source: "manual".into(),
            },
        );
    }
    (root, api)
}
#[test]
fn orders_entire_filter_before_page_and_cursor_survives_deleted_anchor() {
    let (_root, mut api) = fixture();
    sort(&mut api, SortMode::Natural, false);
    let first = page(&mut api, "file", 1, None);
    assert_eq!(names(&first), ["file1.jpg"]);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let id = first["events"][0]["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![id],
            confirmed: true,
        },
    );
    let second = page(&mut api, "file", 1, Some(cursor));
    assert_eq!(names(&second), ["file2.PNG"]);
    let third = page(
        &mut api,
        "file",
        1,
        Some(second["next_cursor"].as_str().unwrap().into()),
    );
    assert_eq!(names(&third), ["file10.webp"]);
    assert!(third["next_cursor"].is_null());
}
#[test]
fn all_modes_reverse_and_shuffle_pages_are_consistent() {
    let (_root, mut api) = fixture();
    for mode in [
        SortMode::Arrival,
        SortMode::Extension,
        SortMode::Natural,
        SortMode::Name,
        SortMode::Size,
        SortMode::Origin,
    ] {
        sort(&mut api, mode, false);
        let forward = names(&page(&mut api, "", 100, None));
        sort(&mut api, mode, true);
        let mut reverse = names(&page(&mut api, "", 100, None));
        reverse.reverse();
        assert_eq!(forward, reverse, "{mode:?}");
    }
    sort(&mut api, SortMode::Shuffle, false);
    let all = names(&page(&mut api, "", 100, None));
    let mut paged = vec![];
    let mut cursor = None;
    loop {
        let next = page(&mut api, "", 1, cursor);
        paged.extend(names(&next));
        cursor = next["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(all, paged);
    sort(&mut api, SortMode::Shuffle, true);
    assert_eq!(all, names(&page(&mut api, "", 100, None)));
}
#[test]
fn cursor_binds_filter_sort_tab_and_source_generation() {
    let (root, mut api) = fixture();
    sort(&mut api, SortMode::Name, false);
    let cursor = page(&mut api, "", 1, None)["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "bad".into(),
        command: Command::BrowseShelfSorted {
            query: "file".into(),
            limit: 1,
            cursor: Some(cursor.clone()),
        },
    });
    assert_eq!(reply["error"]["code"], "stale_cursor");
    sort(&mut api, SortMode::Size, false);
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "bad".into(),
        command: Command::BrowseShelfSorted {
            query: "".into(),
            limit: 1,
            cursor: Some(cursor),
        },
    });
    assert_eq!(reply["error"]["code"], "stale_cursor");
    let saved = call(&mut api, Command::Interaction)["sort"].clone();
    drop(api);
    let mut api = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    assert_eq!(call(&mut api, Command::Interaction)["sort"], saved);
}
#[test]
fn recorded_mtime_unknown_last_without_stat_and_origin_provenance() {
    let (root, mut api) = fixture();
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute("UPDATE events SET modified_at_ms=CASE name WHEN 'file1.jpg' THEN 10 WHEN 'file2.PNG' THEN 20 END",[]).unwrap();
    db.execute(
        "UPDATE events SET origin_json=?1 WHERE name='file1.jpg'",
        [json!({"source_domain":"z.example"}).to_string()],
    )
    .unwrap();
    db.execute(
        "UPDATE events SET origin_json=?1 WHERE name='file2.PNG'",
        [json!({"source_domain":"a.example"}).to_string()],
    )
    .unwrap();
    sort(&mut api, SortMode::Modified, false);
    let page_result = page(&mut api, "", 100, None);
    assert_eq!(page_result["events"][0]["modified_at_ms"], 10);
    assert_eq!(page_result["events"][1]["modified_at_ms"], 20);
    assert!(page_result["events"][2]["modified_at_ms"].is_null());
    let first_id = page_result["events"][0]["id"].as_i64().unwrap();
    assert_eq!(
        call(&mut api, Command::Get { id: first_id })["modified_at_ms"],
        10
    );
    let result = names(&page_result);
    assert_eq!(&result[..2], ["file1.jpg", "file2.PNG"]);
    sort(&mut api, SortMode::Modified, true);
    let result = names(&page(&mut api, "", 100, None));
    assert_eq!(&result[..2], ["file2.PNG", "file1.jpg"]);
    sort(&mut api, SortMode::Origin, false);
    assert_eq!(names(&page(&mut api, "", 100, None))[0], "file2.PNG");
}
#[test]
fn selection_range_is_display_order_and_toggle_is_click_order() {
    let (_root, mut api) = fixture();
    sort(&mut api, SortMode::Natural, false);
    let events = page(&mut api, "file", 100, None);
    let ids: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    call(
        &mut api,
        Command::Select {
            id: ids[2],
            mode: SelectionMode::Replace,
        },
    );
    let selected = call(
        &mut api,
        Command::Select {
            id: ids[0],
            mode: SelectionMode::Range,
        },
    );
    assert_eq!(selected["selected"], json!(ids));
    call(
        &mut api,
        Command::Select {
            id: ids[2],
            mode: SelectionMode::Replace,
        },
    );
    let selected = call(
        &mut api,
        Command::Select {
            id: ids[0],
            mode: SelectionMode::Toggle,
        },
    );
    assert_eq!(selected["selected"], json!([ids[2], ids[0]]));
}

#[path = "common/source_api.rs"]
mod source_api;
#[test]
fn migration_preserves_v3_events_unknown_metadata_and_old_settings() {
    let (root, api) = fixture();
    drop(api);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute("INSERT INTO events(detected_at_ms,source,name,path,size,origin_json) VALUES(42,'manual','legacy-external.txt','absent-synthetic',1,'{}')", []).unwrap();
    db.execute_batch("DROP INDEX events_sort_modified;ALTER TABLE events DROP COLUMN modified_at_ms;PRAGMA user_version=3;").unwrap();
    drop(db);
    let mut api = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    assert_eq!(names(&page(&mut api, "", 100, None)).len(), 4);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        11
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM events WHERE blob_hash IS NOT NULL AND modified_at_ms=detected_at_ms",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        4
    );
    assert_eq!(
        db.query_row(
            "SELECT modified_at_ms FROM events WHERE name='legacy-external.txt'",
            [],
            |r| r.get::<_, Option<i64>>(0)
        )
        .unwrap(),
        None
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
    let settings: katran::settings::Settings =
        serde_json::from_value(json!({"quota_bytes":10000})).unwrap();
    assert_eq!(settings.sort, Default::default());
}
#[test]
fn original_mtime_is_recorded_on_direct_and_prepared_ingest_and_same_hash_source_refresh() {
    use std::{
        fs::{File, FileTimes},
        time::{Duration, UNIX_EPOCH},
    };
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("synthetic.zip");
    std::fs::write(&path, b"synthetic").unwrap();
    let stamp = UNIX_EPOCH + Duration::from_secs(1_000_000);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(stamp))
        .unwrap();
    let mut api = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    let event = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "manual".into(),
        },
    );
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    let recorded = |id: i64| {
        db.query_row("SELECT modified_at_ms FROM events WHERE id=?1", [id], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .unwrap()
    };
    assert_eq!(recorded(event["id"].as_i64().unwrap()), Some(1_000_000_000));
    let mut settings = katran::settings::Settings::defaults(DEFAULT_QUOTA);
    settings.downloads_folder = Some(folder.path().to_owned());
    call(&mut api, Command::SetSettings { settings });
    let original =
        source_api::wait_event(&mut api, katran::source_policy::SourceId::Downloads, &path);
    let id = original["id"].as_i64().unwrap();
    assert_eq!(recorded(id), Some(1_000_000_000));
    let stamp = stamp + Duration::from_secs(120);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(stamp))
        .unwrap();
    source_api::settle(&mut api, katran::source_policy::SourceId::Downloads);
    assert_eq!(recorded(id), Some(1_000_120_000));
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM events WHERE source='downloads'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::IngestPath {
                path: path.clone(),
                source: "manual".into(),
            }),
        },
    );
    let job_id = katran::domain_jobs::JobId(job["job_id"].as_u64().unwrap());
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let result = call(&mut api, Command::PollJob { job_id });
        if result["state"] == "succeeded" {
            assert_eq!(
                recorded(result["result"]["id"].as_i64().unwrap()),
                Some(1_000_120_000)
            );
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{result}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn cursor_rejects_changed_tab_generation_bad_payload_and_supports_large_filtered_pages() {
    let (root, mut api) = fixture();
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    let tx = db.unchecked_transaction().unwrap();
    for n in 0..1010 {
        tx.execute("INSERT INTO events(detected_at_ms,source,name,path,size,origin_json,delete_policy_json) VALUES(0,'manual',?1,'absent-synthetic',1,'{}','{\"kind\":\"shelf_only\"}')",[format!("bulk{n}.png")]).unwrap();
    }
    tx.commit().unwrap();
    sort(&mut api, SortMode::Natural, false);
    let first = page(&mut api, "bulk", 1000, None);
    assert_eq!(first["events"].as_array().unwrap().len(), 1000);
    assert_eq!(names(&first)[999], "bulk999.png");
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = page(&mut api, "bulk", 1000, Some(cursor.clone()));
    assert_eq!(names(&second)[0], "bulk1000.png");
    assert_eq!(names(&second).len(), 10);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Screenshots,
        },
    );
    let bad = |api: &mut Service, cursor: String| {
        api.execute(Request {
            api_version: 1,
            request_id: "invalid".into(),
            command: Command::BrowseShelfSorted {
                query: "bulk".into(),
                limit: 1,
                cursor: Some(cursor),
            },
        })
    };
    assert_eq!(
        bad(&mut api, cursor.clone())["error"]["code"],
        "stale_cursor"
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    db.execute(
        "UPDATE source_configurations SET generation=generation+1",
        [],
    )
    .unwrap();
    assert_eq!(bad(&mut api, cursor)["error"]["code"], "stale_cursor");
    assert_eq!(
        bad(&mut api, "broken".into())["error"]["code"],
        "invalid_cursor"
    );
    assert_eq!(
        bad(&mut api, "x".repeat(131073))["error"]["code"],
        "invalid_cursor"
    );
}
#[test]
fn actions_select_origin_forward_reverse_and_request_fresh_shuffle() {
    let (_root, mut api) = fixture();
    let actions = call(&mut api, Command::Actions);
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "sort_origin")
    );
    call(
        &mut api,
        Command::RunAction {
            id: "sort_origin_reverse".into(),
        },
    );
    let state = call(&mut api, Command::Interaction);
    assert_eq!(state["sort"]["mode"], "origin");
    assert_eq!(state["sort"]["reverse"], true);
    call(
        &mut api,
        Command::RunAction {
            id: "sort_shuffle".into(),
        },
    );
    let first = call(&mut api, Command::Interaction)["sort"]["seed"].clone();
    call(
        &mut api,
        Command::RunAction {
            id: "sort_shuffle".into(),
        },
    );
    assert_ne!(call(&mut api, Command::Interaction)["sort"]["seed"], first);
}

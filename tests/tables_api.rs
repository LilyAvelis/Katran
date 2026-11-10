use katran::{
    interaction::ShelfTab,
    table_clipboard::{Receipt, Sink},
    *,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
fn call(api: &mut Service, command: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "tables".into(),
        command,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
fn raw(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "tables".into(),
        command,
    })
}
fn fixture() -> (tempfile::TempDir, Service) {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    let mut settings = settings::Settings::defaults(DEFAULT_QUOTA);
    settings.downloads_enabled = false;
    settings.tables_enabled = true;
    settings.text_enabled = true;
    settings.links_enabled = true;
    call(&mut api, Command::SetSettings { settings });
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Tables,
        },
    );
    (root, api)
}
fn ingest(api: &mut Service, rows: Vec<Vec<String>>) -> Value {
    call(
        api,
        Command::IngestTable {
            rows,
            source: "synthetic".into(),
        },
    )
}
fn rows(input: &[&[&str]]) -> Vec<Vec<String>> {
    input
        .iter()
        .map(|row| row.iter().map(|s| s.to_string()).collect())
        .collect()
}
fn selection(e: &Value) -> SelectionRef {
    SelectionRef {
        id: e["id"].as_i64().unwrap(),
        revision: e["revision"].as_str().unwrap().into(),
    }
}
type ClipboardCalls = Arc<Mutex<Vec<(String, Vec<u8>)>>>;
#[derive(Clone)]
struct Fake {
    calls: ClipboardCalls,
    html: bool,
    text: bool,
}
impl Sink for Fake {
    fn write(&self, tsv: &str, html: &[u8]) -> Result<Receipt> {
        self.calls.lock().unwrap().push((tsv.into(), html.into()));
        Ok(Receipt {
            sequence: 456,
            owner_token: Some("000000000000000000000000000001c8".into()),
            html_written: self.html,
            text_written: self.text,
            warnings: if self.html && self.text {
                vec![]
            } else {
                vec!["synthetic partial".into()]
            },
        })
    }
}
fn sink(api: &mut Service, html: bool, text: bool) -> ClipboardCalls {
    let calls = Arc::new(Mutex::new(vec![]));
    api.set_table_clipboard_sink(Box::new(Fake {
        calls: calls.clone(),
        html,
        text,
    }));
    calls
}
fn poll(api: &mut Service, id: u64) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let r = call(
            api,
            Command::PollJob {
                job_id: domain_jobs::JobId(id),
            },
        );
        if !matches!(r["state"].as_str(), Some("queued" | "running" | "ready")) {
            return r;
        }
        assert!(std::time::Instant::now() < deadline, "{r}");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn values_persist_search_and_blob_dedup_and_visibility_is_functional() {
    let (root, mut api) = fixture();
    let values = rows(&[&["Имя", "🐄"], &["=1+1", ""], &["multiline\ncell", "a\tb"]]);
    let a = ingest(&mut api, values.clone());
    let b = ingest(&mut api, values.clone());
    assert_eq!(a["kind"], "table_fragment");
    assert_eq!(a["path"], b["path"]);
    assert_ne!(a["id"], b["id"]);
    assert_eq!(
        call(
            &mut api,
            Command::GetTable {
                id: a["id"].as_i64().unwrap()
            }
        )["table"]["rows"],
        json!(values)
    );
    let found = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "multiline".into(),
            limit: 100,
            cursor: None,
        },
    );
    assert_eq!(found["events"].as_array().unwrap().len(), 2);
    let bytes = std::fs::read(a["path"].as_str().unwrap()).unwrap();
    assert!(bytes.starts_with(&[0xef, 0xbb, 0xbf]));
    drop(api);
    let mut api = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::GetTable {
                id: a["id"].as_i64().unwrap()
            }
        )["table"]["rows"],
        json!(values)
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Tables,
            enabled: false,
        },
    );
    assert_eq!(
        raw(
            &mut api,
            Command::IngestTable {
                rows: values,
                source: "synthetic".into()
            }
        )["error"]["code"],
        "tab_disabled"
    );
    call(&mut api, Command::SetTab { tab: ShelfTab::All });
    assert!(
        call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "".into(),
                limit: 100,
                cursor: None
            }
        )["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        call(
            &mut api,
            Command::GetTable {
                id: a["id"].as_i64().unwrap()
            }
        )["id"],
        a["id"]
    );
}
#[test]
fn rich_copy_stacks_in_selection_order_and_reports_partial_publication() {
    let (_root, mut api) = fixture();
    let a = ingest(&mut api, rows(&[&["first", "two"]]));
    let b = ingest(&mut api, rows(&[&["last"]]));
    let calls = sink(&mut api, true, true);
    let result = call(
        &mut api,
        Command::CopySmart {
            items: vec![selection(&b), selection(&a)],
        },
    );
    assert_eq!(result["table_copied"], true);
    let output = calls.lock().unwrap();
    assert_eq!(output[0].0, "last\t\r\n\t\r\nfirst\ttwo\r\n");
    let html = table_fragments::extract_cf_html(&output[0].1).unwrap();
    assert_eq!(
        table_fragments::parse_html(&html).unwrap().rows,
        rows(&[&["last", ""], &["", ""], &["first", "two"]])
    );
    drop(output);
    sink(&mut api, true, false);
    let result = call(
        &mut api,
        Command::CopyTables {
            items: vec![selection(&a)],
        },
    );
    assert_eq!(result["table_copied"], false);
    assert_eq!(result["html_written"], true);
    assert_eq!(result["text_written"], false);
    assert!(!result["warnings"].as_array().unwrap().is_empty());
}
#[test]
fn invalid_and_conflicting_duplicate_selections_never_touch_clipboard() {
    let (_root, mut api) = fixture();
    let a = ingest(&mut api, rows(&[&["a"]]));
    let calls = sink(&mut api, true, true);
    let mut stale = selection(&a);
    stale.revision = "wrong".into();
    let result = call(
        &mut api,
        Command::CopyTables {
            items: vec![selection(&a), stale],
        },
    );
    assert_eq!(result["merged"], false);
    assert_eq!(result["items"][1]["error"]["code"], "stale_selection");
    assert!(calls.lock().unwrap().is_empty());
    let text = call(
        &mut api,
        Command::IngestText {
            text: "ordinary\ttabs".into(),
            source: "synthetic".into(),
        },
    );
    assert_eq!(text["kind"], "text_fragment");
    let result = call(
        &mut api,
        Command::CopyTables {
            items: vec![selection(&a), selection(&text)],
        },
    );
    assert_eq!(result["merged"], false);
    assert!(calls.lock().unwrap().is_empty());
}
#[test]
fn explicit_tsv_html_and_async_disable_cancel_stale_are_api_only() {
    let (_root, mut api) = fixture();
    let a = call(
        &mut api,
        Command::IngestTableTsv {
            text: "a\tb\r\n".into(),
            source: "synthetic".into(),
        },
    );
    let html = call(
        &mut api,
        Command::IngestTableHtml {
            html: "<table><tr><td rowspan='2'>A</td><td>B</td></tr><tr><td>C</td></tr></table>"
                .into(),
            source: "synthetic".into(),
        },
    );
    assert_eq!(
        call(
            &mut api,
            Command::GetTable {
                id: html["id"].as_i64().unwrap()
            }
        )["table"]["rows"],
        json!([["A", "B"], ["", "C"]])
    );
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::IngestTable {
                rows: rows(&[&["disabled"]]),
                source: "synthetic".into(),
            }),
        },
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Tables,
            enabled: false,
        },
    );
    assert_eq!(
        poll(&mut api, job["job_id"].as_u64().unwrap())["state"],
        "failed"
    );
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Tables,
            enabled: true,
        },
    );
    let calls = sink(&mut api, true, true);
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopyTables {
                items: vec![selection(&a)],
            }),
        },
    );
    call(
        &mut api,
        Command::CancelJob {
            job_id: domain_jobs::JobId(job["job_id"].as_u64().unwrap()),
        },
    );
    assert_eq!(
        poll(&mut api, job["job_id"].as_u64().unwrap())["state"],
        "cancelled"
    );
    assert!(calls.lock().unwrap().is_empty());
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopyTables {
                items: vec![selection(&a)],
            }),
        },
    );
    call(
        &mut api,
        Command::DeleteEvents {
            ids: vec![a["id"].as_i64().unwrap()],
            confirmed: true,
        },
    );
    assert_eq!(
        poll(&mut api, job["job_id"].as_u64().unwrap())["state"],
        "failed"
    );
    assert!(calls.lock().unwrap().is_empty());
}
#[test]
fn aggregate_limits_migration_and_plain_csv_preserve_type_contracts() {
    let (root, mut api) = fixture();
    let ordinary = call(
        &mut api,
        Command::IngestBytes {
            name: "ordinary.csv".into(),
            bytes: b"a,b\r\n".to_vec(),
            source: "synthetic".into(),
        },
    );
    assert_eq!(ordinary["kind"], "file_reference");
    let a = ingest(
        &mut api,
        vec![vec!["x".repeat(table_fragments::MAX_CONTENT_BYTES / 2 + 1)]],
    );
    let b = ingest(
        &mut api,
        vec![vec!["y".repeat(table_fragments::MAX_CONTENT_BYTES / 2 + 1)]],
    );
    let calls = sink(&mut api, true, true);
    let result = raw(
        &mut api,
        Command::CopyTables {
            items: vec![selection(&a), selection(&b)],
        },
    );
    assert_eq!(result["error"]["code"], "invalid_table");
    assert!(calls.lock().unwrap().is_empty());
    drop(api);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE table_fragments;PRAGMA user_version=5;")
        .unwrap();
    drop(db);
    let mut api = Service::open(root.path(), DEFAULT_QUOTA).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::Get {
                id: ordinary["id"].as_i64().unwrap()
            }
        )["kind"],
        "file_reference"
    );
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("before-v6"))
    );
}
#[test]
fn readonly_get_table_and_async_html_success_and_smart_copy_remain_rich() {
    let (root, mut api) = fixture();
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::IngestTableHtml {
                html: "<table><tr><td>🐄</td><td>=A1</td></tr></table>".into(),
                source: "synthetic".into(),
            }),
        },
    );
    let event = poll(&mut api, job["job_id"].as_u64().unwrap())["result"].clone();
    let mut reader = Service::open_read_only(root.path()).unwrap();
    assert_eq!(
        call(
            &mut reader,
            Command::GetTable {
                id: event["id"].as_i64().unwrap()
            }
        )["table"]["rows"],
        json!([["🐄", "=A1"]])
    );
    let calls = sink(&mut api, true, true);
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::CopySmart {
                items: vec![selection(&event)],
            }),
        },
    );
    let result = poll(&mut api, job["job_id"].as_u64().unwrap());
    assert_eq!(result["result"]["table_copied"], true);
    assert_eq!(calls.lock().unwrap().len(), 1);
}
#[test]
fn shared_csv_bytes_keep_text_and_table_search_independent_in_either_order() {
    for table_first in [true, false] {
        let (root, mut api) = fixture();
        let values = rows(&[&["left", "right"]]);
        let csv = String::from_utf8(
            table_fragments::to_csv_bom(&table_fragments::Table {
                rows: values.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        let (text, table) = if table_first {
            let table = ingest(&mut api, values.clone());
            let text = call(
                &mut api,
                Command::IngestText {
                    text: csv.clone(),
                    source: "synthetic".into(),
                },
            );
            (text, table)
        } else {
            let text = call(
                &mut api,
                Command::IngestText {
                    text: csv.clone(),
                    source: "synthetic".into(),
                },
            );
            let table = ingest(&mut api, values.clone());
            (text, table)
        };
        assert_eq!(text["path"], table["path"]);
        ingest(&mut api, values);
        call(
            &mut api,
            Command::SetTab {
                tab: ShelfTab::Tables,
            },
        );
        let found = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "left\tright".into(),
                limit: 100,
                cursor: None,
            },
        );
        assert_eq!(found["events"].as_array().unwrap().len(), 2);
        call(
            &mut api,
            Command::SetTab {
                tab: ShelfTab::Text,
            },
        );
        let found = call(
            &mut api,
            Command::BrowseShelfSorted {
                query: "left,right".into(),
                limit: 100,
                cursor: None,
            },
        );
        assert_eq!(found["events"].as_array().unwrap().len(), 1);
        assert_eq!(found["events"][0]["id"], text["id"]);
        let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM table_contents", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            call(
                &mut api,
                Command::GetText {
                    id: text["id"].as_i64().unwrap()
                }
            )["text"],
            csv
        );
    }
}

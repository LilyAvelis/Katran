use katran::{Command, Request, SelectionRef, Service, interaction::ShelfTab};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
fn call(api: &mut Service, command: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "cut-publication".into(),
        command,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
fn item(e: &Value) -> SelectionRef {
    SelectionRef {
        id: e["id"].as_i64().unwrap(),
        revision: e["revision"].as_str().unwrap().into(),
    }
}
struct Files(Arc<AtomicU32>);
impl katran::clipboard_files::Source for Files {
    fn sequence(&self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
    fn files(&self) -> katran::Result<Vec<PathBuf>> {
        Ok(vec![])
    }
}
struct Text(Arc<AtomicU32>);
impl katran::text_fragments::ClipboardSink for Text {
    fn write(&self, _: &str) -> katran::Result<u32> {
        Ok(self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
#[test]
fn unrelated_text_publication_disarms_old_cut_in_sync_and_worker_paths() {
    for asynchronous in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100000).unwrap();
        let seq = Arc::new(AtomicU32::new(1));
        api.set_clipboard_files_source(Box::new(Files(seq.clone())));
        api.set_text_clipboard_sink(Box::new(Text(seq)));
        call(
            &mut api,
            Command::SetTabEnabled {
                tab: ShelfTab::Text,
                enabled: true,
            },
        );
        let a = call(
            &mut api,
            Command::IngestBytes {
                name: "cut.png".into(),
                bytes: vec![1, 2, 3],
                source: "synthetic".into(),
            },
        );
        let b = call(
            &mut api,
            Command::IngestText {
                text: "new copied text".into(),
                source: "synthetic".into(),
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
            Command::StageTransfer {
                items: vec![item(&a)],
                cut: true,
            },
        );
        let command = Command::CopyText {
            items: vec![item(&b)],
            separator: "".into(),
        };
        if asynchronous {
            let job = call(
                &mut api,
                Command::BeginJob {
                    command: Box::new(command),
                },
            );
            let job_id = serde_json::from_value(job["job_id"].clone()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let status = call(&mut api, Command::PollJob { job_id });
                if status["state"] == "succeeded" {
                    break;
                }
                assert!(Instant::now() < deadline, "{status}");
                std::thread::yield_now();
            }
        } else {
            call(&mut api, command);
        }
        let r = api.execute(Request {
            api_version: 1,
            request_id: "paste".into(),
            command: Command::PasteToFolder,
        });
        assert_eq!(r["ok"], false, "no external files in fake Clipboard");
        assert!(std::path::Path::new(a["path"].as_str().unwrap()).exists());
        assert!(!folder.path().join("cut.png").exists());
        assert_eq!(
            call(
                &mut api,
                Command::Get {
                    id: a["id"].as_i64().unwrap()
                }
            )["id"],
            a["id"]
        );
    }
}
#[test]
fn exact_staged_copy_survives_text_publication_but_not_as_cut() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100000).unwrap();
    let seq = Arc::new(AtomicU32::new(1));
    api.set_clipboard_files_source(Box::new(Files(seq.clone())));
    api.set_text_clipboard_sink(Box::new(Text(seq)));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let e = call(
        &mut api,
        Command::IngestText {
            text: "same text".into(),
            source: "synthetic".into(),
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
        Command::StageTransfer {
            items: vec![item(&e)],
            cut: false,
        },
    );
    call(
        &mut api,
        Command::CopySmart {
            items: vec![item(&e)],
        },
    );
    let r = call(&mut api, Command::PasteToFolder);
    assert_eq!(r["cut"], false);
    assert_eq!(r["items"][0]["status"], "copied");
    assert!(std::path::Path::new(e["path"].as_str().unwrap()).exists());
}

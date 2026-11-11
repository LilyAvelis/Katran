use katran::{
    Command, Request, Service,
    app_handlers::{AppHandler, HandlerIcon, Provider},
    interaction::SelectionMode,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

type Calls = Arc<Mutex<Vec<(String, String, PathBuf)>>>;
struct Fake {
    calls: Calls,
}
impl Provider for Fake {
    fn discover(&self, ext: &str) -> katran::Result<Vec<AppHandler>> {
        assert!(ext.starts_with('.'));
        Ok(vec![
            AppHandler {
                id: "second".into(),
                name: "Other editor".into(),
                recommended: false,
                icon: None,
            },
            AppHandler {
                id: "first".into(),
                name: "Preferred editor".into(),
                recommended: true,
                icon: Some(HandlerIcon {
                    width: 999,
                    height: 999,
                    rgba_base64: "malformed".into(),
                }),
            },
            AppHandler {
                id: "first".into(),
                name: "duplicate".into(),
                recommended: false,
                icon: None,
            },
        ])
    }
    fn invoke(&self, ext: &str, id: &str, path: &Path) -> katran::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push((ext.into(), id.into(), path.into()));
        Ok(())
    }
}
fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "apps".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let r = reply(api, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
fn fixture() -> (tempfile::TempDir, Service, Calls, i64, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(&root.path().join("shelf"), 10000).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    api.set_app_provider(Box::new(Fake {
        calls: Arc::clone(&calls),
    }));
    let path = root.path().join("Original & проба.txt");
    std::fs::write(&path, b"original").unwrap();
    let item = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "test".into(),
        },
    );
    let id = item["id"].as_i64().unwrap();
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    (root, api, calls, id, path.canonicalize().unwrap())
}

#[test]
fn picker_is_revision_bound_and_rejects_unknown_apps_without_launching() {
    let (_root, mut api, calls, id, path) = fixture();
    let list = call(
        &mut api,
        Command::RunAction {
            id: "open_with".into(),
        },
    );
    assert_eq!(list["open_apps"], true);
    assert_eq!(list["apps"].as_array().unwrap().len(), 2);
    assert_eq!(list["apps"][0]["id"], "first");
    assert_eq!(list["apps"][0]["icon"], Value::Null);
    assert!(calls.lock().unwrap().is_empty());
    let token = list["token"].as_str().unwrap().to_owned();
    assert_eq!(
        reply(
            &mut api,
            Command::OpenWithApp {
                token: token.clone(),
                app_id: Some("C:\\forged.exe".into())
            }
        )["error"]["code"],
        "unknown_application"
    );
    // Changes to the cursor cannot retarget an already offered catalogue.
    let second = call(
        &mut api,
        Command::IngestBytes {
            name: "other.txt".into(),
            bytes: b"other".to_vec(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut api, Command::SetVisible { ids: vec![second] });
    call(
        &mut api,
        Command::Select {
            id: second,
            mode: SelectionMode::Replace,
        },
    );
    let result = call(
        &mut api,
        Command::OpenWithApp {
            token: token.clone(),
            app_id: Some("second".into()),
        },
    );
    assert_eq!(result["id"], id);
    assert_eq!(
        calls.lock().unwrap()[0],
        (".txt".into(), "second".into(), path.clone())
    );
    std::fs::write(&path, b"changed!").unwrap();
    assert_eq!(
        reply(
            &mut api,
            Command::OpenWithApp {
                token: token.clone(),
                app_id: Some("first".into())
            }
        )["error"]["code"],
        "source_changed"
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
    call(
        &mut api,
        Command::ReleaseOpenApps {
            token: token.clone(),
        },
    );
    assert_eq!(
        reply(
            &mut api,
            Command::OpenWithApp {
                token,
                app_id: Some("first".into())
            }
        )["error"]["code"],
        "open_apps_expired"
    );
}

#[test]
fn cached_image_uses_recorded_extension_and_independent_named_file() {
    let (_root, mut api, calls, _, _) = fixture();
    let item = call(
        &mut api,
        Command::IngestBytes {
            name: "clipboard.PNG".into(),
            bytes: b"image-bytes".to_vec(),
            source: "test".into(),
        },
    );
    let id = item["id"].as_i64().unwrap();
    let list = call(&mut api, Command::ListOpenApps { id });
    let token = list["token"].as_str().unwrap().to_owned();
    let started = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::OpenWithApp {
                token,
                app_id: Some("first".into()),
            }),
        },
    );
    let job_id = serde_json::from_value(started["job_id"].clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = call(&mut api, Command::PollJob { job_id });
        if status["state"] == "succeeded" {
            break;
        }
        assert!(
            matches!(
                status["state"].as_str(),
                Some("queued" | "running" | "ready")
            ),
            "{status}"
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].0, ".png");
    let path = &calls[0].2;
    assert!(path.extension().unwrap().eq_ignore_ascii_case("png"));
    std::fs::write(path, b"edited").unwrap();
    assert_eq!(
        std::fs::read(item["path"].as_str().unwrap()).unwrap(),
        b"image-bytes"
    );
}

#[test]
fn sessions_are_bounded_and_no_extension_has_an_explicit_empty_list() {
    let (_root, mut api, _calls, id, _) = fixture();
    let first = call(&mut api, Command::ListOpenApps { id })["token"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..8 {
        call(&mut api, Command::ListOpenApps { id });
    }
    assert_eq!(
        reply(
            &mut api,
            Command::OpenWithApp {
                token: first,
                app_id: Some("first".into())
            }
        )["error"]["code"],
        "open_apps_expired"
    );
    let item = call(
        &mut api,
        Command::IngestBytes {
            name: "noextension".into(),
            bytes: b"one".to_vec(),
            source: "test".into(),
        },
    );
    assert_eq!(
        call(
            &mut api,
            Command::ListOpenApps {
                id: item["id"].as_i64().unwrap()
            }
        )["apps"],
        json!([])
    );
}

#[test]
fn synchronous_link_open_with_passes_exact_url_to_offered_handler() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    api.set_app_provider(Box::new(Fake {
        calls: Arc::clone(&calls),
    }));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Links,
            enabled: true,
        },
    );
    let url = "https://example.test/путь?x=1&y=two#фрагмент";
    let id = call(
        &mut api,
        Command::IngestLink {
            url: url.into(),
            source: "test".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    let list = call(&mut api, Command::ListOpenApps { id });
    let token = list["token"].as_str().unwrap().to_owned();
    let receipt = call(
        &mut api,
        Command::OpenWithApp {
            token,
            app_id: Some("first".into()),
        },
    );
    assert_eq!(receipt["link"], true);
    assert_eq!(receipt["operation"], "open_with");
    let invocations = calls.lock().unwrap();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].0, ".html");
    assert_eq!(invocations[0].1, "first");
    assert_eq!(invocations[0].2.as_os_str(), std::ffi::OsStr::new(url));
}

#[test]fn async_link_open_with_passes_exact_url(){
 let root=tempfile::tempdir().unwrap();let mut api=Service::open(root.path(),10000).unwrap();let calls=Arc::new(Mutex::new(Vec::new()));api.set_app_provider(Box::new(Fake{calls:calls.clone()}));
 call(&mut api,Command::SetTabEnabled{tab:katran::interaction::ShelfTab::Links,enabled:true});let url="https://example.test/путь?a=1#exact";
 let id=call(&mut api,Command::IngestLink{url:url.into(),source:"synthetic".into()})["id"].as_i64().unwrap();let token=call(&mut api,Command::ListOpenApps{id})["token"].as_str().unwrap().to_owned();
 let job=call(&mut api,Command::BeginJob{command:Box::new(Command::OpenWithApp{token,app_id:Some("first".into())})});let job_id=serde_json::from_value(job["job_id"].clone()).unwrap();let deadline=Instant::now()+Duration::from_secs(3);loop{let status=call(&mut api,Command::PollJob{job_id});if status["state"]=="succeeded"{assert_eq!(status["result"]["link"],true);break;}assert!(Instant::now()<deadline,"{status}");std::thread::yield_now();}
 assert_eq!(calls.lock().unwrap()[0].2.as_os_str(),std::ffi::OsStr::new(url));
}

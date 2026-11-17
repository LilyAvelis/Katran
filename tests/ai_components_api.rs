use katran::{
    Command, Request, Service,
    ai_components::{Action, Component, Installation, Launcher},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
struct Handle(Arc<Mutex<bool>>);
impl Installation for Handle {
    fn finished(&mut self) -> katran::Result<bool> {
        Ok(*self.0.lock().unwrap())
    }
}
struct Fake {
    requests: Arc<Mutex<Vec<PathBuf>>>,
    done: Arc<Mutex<bool>>,
}
impl Launcher for Fake {
    fn launch(&mut self, request: &Path, _: &Path) -> katran::Result<Box<dyn Installation>> {
        self.requests.lock().unwrap().push(request.into());
        Ok(Box::new(Handle(self.done.clone())))
    }
}
fn invoke(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "ai-components".into(),
        command,
    })
}
#[test]
fn explicit_terminal_operations_are_scoped_and_receipts_are_not_presence() {
    let root = tempfile::tempdir().unwrap();
    let bundle = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(bundle.path().join("scripts/ai-components")).unwrap();
    std::fs::write(
        bundle.path().join("scripts/ai-components/terminal.ps1"),
        "# test only",
    )
    .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(Mutex::new(false));
    let mut api = Service::open(root.path(), 100_000).unwrap();
    api.set_ai_component_launcher(
        Box::new(Fake {
            requests: requests.clone(),
            done: done.clone(),
        }),
        bundle.path().into(),
    );
    let before = invoke(&mut api, Command::AiComponentsStatus);
    assert_eq!(before["ok"], true);
    assert!(
        requests.lock().unwrap().is_empty(),
        "status must not launch anything"
    );
    assert_eq!(before["result"]["components"][0]["files_found"], false);
    let launch = invoke(
        &mut api,
        Command::LaunchAiComponent {
            component: Component::Folder,
            action: Action::Install,
        },
    );
    assert_eq!(launch["ok"], true, "{launch}");
    assert_eq!(launch["result"]["visible_terminal"], true);
    assert_eq!(
        invoke(
            &mut api,
            Command::LaunchAiComponent {
                component: Component::Folder,
                action: Action::Verify
            }
        )["error"]["code"],
        "ai_install_busy"
    );
    let request = requests.lock().unwrap()[0].clone();
    let value: Value = serde_json::from_slice(&std::fs::read(&request).unwrap()).unwrap();
    assert_eq!(value["component"], "folder");
    assert_eq!(value["action"], "install");
    assert!(Path::new(value["profile"].as_str().unwrap()).is_absolute());
    assert_eq!(
        request.parent().unwrap().join("install.log").to_str(),
        value["log_path"].as_str()
    );
    let result = request.parent().unwrap().join("result.json");
    std::fs::write(
        &result,
        json!({"id":value["id"],"component":"folder","state":"succeeded","verified":true})
            .to_string(),
    )
    .unwrap();
    let status = invoke(&mut api, Command::AiComponentsStatus);
    assert_eq!(
        status["result"]["components"][1]["files_found"], false,
        "a receipt cannot manufacture runtime files"
    );
    assert_eq!(
        status["result"]["components"][1]["operation"]["receipt"]["verified"],
        true
    );
    assert_eq!(
        invoke(
            &mut api,
            Command::LaunchAiComponent {
                component: Component::Folder,
                action: Action::Verify
            }
        )["ok"],
        true,
        "finished receipt releases operation despite terminal staying open"
    );
    *done.lock().unwrap() = true;
    let status = invoke(&mut api, Command::AiComponentsStatus);
    assert_eq!(
        status["result"]["components"][1]["operation"]["receipt"]["code"],
        "terminal_closed"
    );
}

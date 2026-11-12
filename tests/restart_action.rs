use katran::{Command, Request, Service};
use serde_json::Value;
fn call(api: &mut Service, command: Command) -> Value {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "restart-action".into(),
        command,
    });
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
#[test]
fn restart_is_in_actions_and_returns_intent_without_changing_profile() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let before = call(&mut api, Command::GetSettings);
    let actions = call(&mut api, Command::Actions);
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "restart" && a["label"] == "Restart Katran")
    );
    assert_eq!(
        call(
            &mut api,
            Command::RunAction {
                id: "restart".into()
            }
        )["restart_requested"],
        true
    );
    assert_eq!(call(&mut api, Command::GetSettings), before);
}

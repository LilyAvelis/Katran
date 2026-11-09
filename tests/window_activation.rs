use katran::{Command, Request, Service};
#[test]
fn activation_is_repeatable_without_changing_the_shelf() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    for serial in 1..=3 {
        let reply = api.execute(Request {
            api_version: 1,
            request_id: "launch".into(),
            command: Command::ActivateWindow,
        });
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["result"]["activation_requested"], true);
        assert_eq!(reply["result"]["serial"], serial);
    }
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "state".into(),
        command: Command::Interaction,
    });
    assert!(reply["result"]["selected"].as_array().unwrap().is_empty());
    assert!(reply["result"]["preview"].is_null());
}

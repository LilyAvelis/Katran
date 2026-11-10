use katran::{Command, Request, Service, origin::Origin};

#[test]
fn known_sharex_clipboard_is_excluded_but_other_images_are_ingested() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1000).unwrap();
    for (app, ignored) in [("ShareX", true), ("Zen", false)] {
        let response = api.execute(Request {
            api_version: 1,
            request_id: app.into(),
            command: Command::IngestWithOrigin {
                origin: Origin {
                    source_app: Some(app.into()),
                    source_domain: None,
                },
                command: Box::new(Command::IngestBytes {
                    name: "synthetic.png".into(),
                    bytes: vec![1, 2, 3],
                    source: "clipboard_image".into(),
                }),
            },
        });
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["ignored"] == true, ignored);
    }
    let response = api.execute(Request {
        api_version: 1,
        request_id: "list".into(),
        command: Command::List { limit: 10 },
    });
    assert_eq!(response["result"]["events"].as_array().unwrap().len(), 1);
}

use katran::{Command, Request, SelectionRef, Service};
#[test]
fn mixed_regular_file_types_share_one_prepared_drag_snapshot() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let mut api = Service::open(data.path(), 10000).unwrap();
    let mut request = |command| {
        api.execute(Request {
            api_version: 1,
            request_id: "mixed".into(),
            command,
        })
    };
    let mut items = Vec::new();
    for extension in ["zip", "docx", "mp4", "pdf", "unknown"] {
        let path = files.path().join(format!("synthetic.{extension}"));
        std::fs::write(&path, extension.as_bytes()).unwrap();
        let response = request(Command::IngestPath {
            path,
            source: "test".into(),
        });
        assert_eq!(response["ok"], true);
        items.push(SelectionRef {
            id: response["result"]["id"].as_i64().unwrap(),
            revision: response["result"]["revision"].as_str().unwrap().into(),
        });
    }
    let response = request(Command::DragEventsChecked { items });
    assert_eq!(response["ok"], true, "{response}");
    let token = response["result"]["drag_token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(request(Command::CancelPreparedDrag { token })["ok"], true);
    for extension in ["zip", "docx", "mp4", "pdf", "unknown"] {
        assert_eq!(
            std::fs::read(files.path().join(format!("synthetic.{extension}"))).unwrap(),
            extension.as_bytes()
        );
    }
}

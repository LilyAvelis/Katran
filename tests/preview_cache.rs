use katran::{Command, Request, Service};
fn call(s: &mut Service, c: Command) -> serde_json::Value {
    s.execute(Request {
        api_version: 1,
        request_id: "preview-test".into(),
        command: c,
    })
}
#[test]
fn preview_persists_and_source_revision_blocks_stale_selection() {
    let root = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("image.png");
    image::DynamicImage::new_rgb8(10, 10).save(&path).unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let r = call(
        &mut api,
        Command::IngestPath {
            path: path.clone(),
            source: "test".into(),
        },
    );
    let id = r["result"]["id"].as_i64().unwrap();
    let revision = r["result"]["revision"].as_str().unwrap().to_string();
    assert_eq!(
        call(&mut api, Command::PreviewImage { id, max_edge: 64 })["result"]["cache_hit"],
        false
    );
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::PreviewImage { id, max_edge: 64 })["result"]["cache_hit"],
        true
    );
    image::DynamicImage::new_rgb8(30, 20).save(&path).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::CopyEventsChecked {
                items: vec![katran::SelectionRef { id, revision }]
            }
        )["error"]["code"],
        "stale_selection"
    );
    assert_eq!(
        call(&mut api, Command::PreviewImage { id, max_edge: 64 })["error"]["code"],
        "source_changed"
    );
    let mut read = Service::open_read_only(root.path()).unwrap();
    assert_eq!(
        call(
            &mut read,
            Command::DeleteEvents {
                ids: vec![id],
                confirmed: true
            }
        )["error"]["code"],
        "read_only"
    );
}

use katran::{Request, Service};
use serde_json::{Value, json};
use tempfile::tempdir;

#[test]
fn capture_is_explicit_and_pause_prevents_clipboard_reads() {
    let root = tempdir().unwrap();
    let mut service = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        call(&mut service, json!({"type":"status"}))["result"]["capture_enabled"],
        false
    );
    assert_eq!(
        call(&mut service, json!({"type":"poll_clipboard"}))["result"]["state"],
        "paused"
    );
    assert_eq!(
        call(
            &mut service,
            json!({"type":"set_capture_enabled","enabled":true})
        )["result"]["capture_enabled"],
        true
    );
    assert_eq!(
        call(
            &mut service,
            json!({"type":"set_capture_enabled","enabled":false})
        )["result"]["capture_enabled"],
        false
    );
    assert_eq!(
        call(&mut service, json!({"type":"poll_clipboard"}))["result"]["state"],
        "paused"
    );
}

fn call(service: &mut Service, command: Value) -> Value {
    let request: Request =
        serde_json::from_value(json!({"api_version":1,"request_id":"test","command":command}))
            .unwrap();
    service.execute(request)
}

#[test]
fn bytes_events_dedupe_quota_restart_and_safe_import() {
    let root = tempdir().unwrap();
    let dest = tempdir().unwrap();
    let mut service = Service::open(root.path(), 4).unwrap();
    for _ in 0..5 {
        let reply = call(
            &mut service,
            json!({"type":"ingest_bytes","name":"image.png","bytes":[1,2,3,4],"source":"test"}),
        );
        assert_eq!(reply["ok"], true, "{reply}");
    }
    assert_eq!(
        call(&mut service, json!({"type":"status"}))["result"]["cached_bytes"],
        4
    );
    let rejected = call(
        &mut service,
        json!({"type":"ingest_bytes","name":"other.png","bytes":[5],"source":"test"}),
    );
    assert_eq!(rejected["error"]["code"], "quota_exceeded");
    drop(service);
    let mut service = Service::open(root.path(), 4).unwrap();
    assert_eq!(
        call(&mut service, json!({"type":"list"}))["result"]["events"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    std::fs::write(dest.path().join("image.png"), b"original").unwrap();
    let reply = call(
        &mut service,
        json!({"type":"import","ids":[1,2,3],"destination":dest.path()}),
    );
    let items = reply["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    for item in items {
        assert_eq!(item["ok"], true, "{item}");
        assert_eq!(
            std::fs::read(item["path"].as_str().unwrap()).unwrap(),
            vec![1, 2, 3, 4]
        );
    }
    assert_eq!(
        std::fs::read(dest.path().join("image.png")).unwrap(),
        b"original"
    );
}

#[test]
fn references_report_missing_and_batch_partial_failures() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let dest = tempdir().unwrap();
    let mut service = Service::open(root.path(), 1).unwrap();
    let path = sources.path().join("source.png");
    std::fs::write(&path, b"source contents").unwrap();
    assert_eq!(
        call(
            &mut service,
            json!({"type":"ingest_path","path":path,"source":"test"})
        )["result"]["storage"],
        "reference"
    );
    assert_eq!(
        call(&mut service, json!({"type":"status"}))["result"]["cached_bytes"],
        0
    );
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        call(&mut service, json!({"type":"get","id":1}))["result"]["available"],
        false
    );
    let reply = call(
        &mut service,
        json!({"type":"import","ids":[1],"destination":dest.path()}),
    );
    assert_eq!(reply["result"]["items"][0]["ok"], false);
    assert_eq!(std::fs::read_dir(dest.path()).unwrap().count(), 0);
}

#[test]
fn rejects_wrong_version_and_path_traversal() {
    let root = tempdir().unwrap();
    let mut service = Service::open(root.path(), 100).unwrap();
    let request = serde_json::from_value(
        json!({"api_version":99,"request_id":"version","command":{"type":"status"}}),
    )
    .unwrap();
    assert_eq!(
        service.execute(request)["error"]["code"],
        "unsupported_version"
    );
    for name in ["../outside.png", "C:\\outside.png", "CON.png"] {
        assert_eq!(
            call(
                &mut service,
                json!({"type":"ingest_bytes","name":name,"bytes":[1],"source":"test"})
            )["error"]["code"],
            "invalid_argument"
        );
    }
}

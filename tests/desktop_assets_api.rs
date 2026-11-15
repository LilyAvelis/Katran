use katran::{Command, Request, Service, interaction::ShelfTab};
use sha2::Digest;

fn call(api: &mut Service, command: Command) -> serde_json::Value {
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "desktop-assets-test".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}

#[test]
fn hidden_desktop_tab_still_collects_and_can_delete_inactive_backgrounds() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let first = source.path().join("first.png");
    let second = source.path().join("second.png");
    image::RgbaImage::from_pixel(5, 4, image::Rgba([10, 20, 30, 255]))
        .save(&first)
        .unwrap();
    image::RgbaImage::from_pixel(5, 4, image::Rgba([40, 50, 60, 255]))
        .save(&second)
        .unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Desktop,
            enabled: false,
        },
    );
    let a = call(&mut api, Command::ImportThemeBackground { path: first });
    let b = call(&mut api, Command::ImportThemeBackground { path: second });
    let aid = format!("image:{}", a["id"].as_str().unwrap());
    let bid = format!("image:{}", b["id"].as_str().unwrap());
    let listed = call(&mut api, Command::ListDesktopAssets);
    assert_eq!(listed["items"].as_array().unwrap().len(), 2);
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::ListDesktopAssets),
        },
    );
    let job_id = serde_json::from_value(job["job_id"].clone()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let status = call(&mut api, Command::PollJob { job_id });
        if status["state"] == "succeeded" {
            assert_eq!(status["result"]["items"].as_array().unwrap().len(), 2);
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{status}");
        std::thread::yield_now();
    }
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == aid)
    );
    call(
        &mut api,
        Command::ApplyDesktopAsset {
            id: aid.clone(),
            target: "katran".into(),
        },
    );
    let failed = api.execute(Request {
        api_version: 1,
        request_id: "active-delete".into(),
        command: Command::DeleteDesktopAsset { id: aid },
    });
    assert_eq!(failed["error"]["code"], "background_in_use");
    call(&mut api, Command::DeleteDesktopAsset { id: bid });
    assert_eq!(
        call(&mut api, Command::ListDesktopAssets)["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn direct_apply_rejects_modified_managed_video() {
    let root = tempfile::tempdir().unwrap();
    let original = b"synthetic video bytes";
    let digest = format!("{:x}", sha2::Sha256::digest(original));
    let directory = root.path().join("video-backgrounds");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("{digest}.mp4")),
        b"changed video bytes",
    )
    .unwrap();
    std::fs::write(
        directory.join(format!("{digest}.json")),
        serde_json::json!({"id":digest,"name":"video.mp4","codec":"h264","width":2,"height":2,"duration":1.0}).to_string(),
    )
    .unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "changed-direct-apply".into(),
        command: Command::ApplyDesktopAsset {
            id: format!("video:{digest}"),
            target: "katran".into(),
        },
    });
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error"]["code"], "stale_selection");
}

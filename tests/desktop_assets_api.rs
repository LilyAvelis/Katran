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

#[test]
fn desktop_actions_use_metadata_and_reject_multi_selection() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let first = source.path().join("first.png");
    image::RgbaImage::from_pixel(5, 4, image::Rgba([10, 20, 30, 255]))
        .save(&first)
        .unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    call(&mut api, Command::ImportThemeBackground { path: first });
    let bytes = b"synthetic video";
    let digest = format!("{:x}", sha2::Sha256::digest(bytes));
    let dir = root.path().join("video-backgrounds");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{digest}.mp4")), bytes).unwrap();
    std::fs::write(dir.join(format!("{digest}.json")), serde_json::json!({
        "id": digest, "name": "video.mp4", "codec": "h264", "width": 2, "height": 2, "duration": 1.0
    }).to_string()).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Desktop,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Desktop,
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let events = page["events"].as_array().unwrap();
    let image = events
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with(".png"))
        .unwrap();
    let video = events
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with(".mp4"))
        .unwrap();
    let image_id = image["id"].as_i64().unwrap();
    let video_id = video["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::SetVisible {
            ids: vec![image_id, video_id],
        },
    );
    let select = |id, mode| Command::Select { id, mode };
    let has = |actions: &serde_json::Value, name: &str| {
        actions.as_array().unwrap().iter().any(|a| a["id"] == name)
    };
    call(
        &mut api,
        select(image_id, katran::interaction::SelectionMode::Replace),
    );
    // Change bytes after browsing: menu remains metadata-only; action rejects them.
    std::fs::write(image["path"].as_str().unwrap(), b"synthetic tampering").unwrap();
    let image_actions = call(&mut api, Command::Actions);
    assert!(has(&image_actions, "use_as_background"));
    assert!(has(&image_actions, "use_as_desktop_wallpaper"));
    assert!(!has(&image_actions, "use_as_video_background"));
    let result = api.execute(Request {
        api_version: 1,
        request_id: "stale-desktop".into(),
        command: Command::UseSelectedImageAsBackground,
    });
    assert_eq!(result["ok"], false, "{result}");
    call(
        &mut api,
        select(video_id, katran::interaction::SelectionMode::Replace),
    );
    let video_actions = call(&mut api, Command::Actions);
    let expected_sort: Vec<_> = video_actions
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["id"].as_str().unwrap().starts_with("sort_"))
        .cloned()
        .collect();
    assert!(!expected_sort.is_empty());
    assert_eq!(
        call(&mut api, Command::SortActions),
        serde_json::json!(expected_sort)
    );
    assert!(has(&video_actions, "use_as_video_background"));
    assert!(has(&video_actions, "use_as_video_desktop_wallpaper"));
    assert!(has(&video_actions, "use_as_video_desktop_wallpaper_silent"));
    assert!(!has(&video_actions, "use_as_background"));
    call(
        &mut api,
        select(image_id, katran::interaction::SelectionMode::Toggle),
    );
    let multi = call(&mut api, Command::Actions);
    // Sorting remains a pure catalogue query even if the selected file goes away.
    std::fs::remove_file(video["path"].as_str().unwrap()).unwrap();
    assert_eq!(
        call(&mut api, Command::SortActions),
        serde_json::json!(expected_sort)
    );
    for id in [
        "use_as_background",
        "use_as_desktop_wallpaper",
        "use_as_video_background",
        "use_as_video_desktop_wallpaper",
        "use_as_video_desktop_wallpaper_silent",
    ] {
        assert!(!has(&multi, id), "{id}");
    }
}

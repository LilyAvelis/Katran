#[path = "common/source_api.rs"]
mod source_api;
use katran::{
    Command, Request, Service,
    domain_jobs::JobId,
    interaction::{SelectionMode, ShelfTab},
    source_policy::SourceId,
};
use serde_json::Value;
fn call(api: &mut Service, command: Command) -> Value {
    let v = api.execute(Request {
        api_version: 1,
        request_id: "video-api".into(),
        command,
    });
    assert_eq!(v["ok"], true, "{v}");
    v["result"].clone()
}
#[test]
fn folder_video_action_uses_original_without_importing_into_downloads() {
    use std::process::Command as Process;
    if Process::new("ffmpeg").arg("-version").output().is_err() {
        return;
    }
    let profile = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("folder-background.mp4");
    assert!(
        Process::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x90:rate=24",
                "-t",
                "1",
                "-c:v",
                "libx264",
                "-y"
            ])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let original = std::fs::read(&source).unwrap();
    let mut api = Service::open(profile.path(), 10_000_000).unwrap();
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().to_owned(),
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
    let id = page["events"][0]["id"].as_i64().unwrap();
    assert!(id < 0);
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    assert!(
        call(&mut api, Command::Actions)
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "use_as_video_background")
    );
    assert_eq!(
        call(&mut api, Command::UseSelectedVideoAsBackground)["background_applied"],
        true
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    // A stale Folder revision must not authorize preparation after a source change.
    let mut event: katran::Event = serde_json::from_value(page["events"][0].clone()).unwrap();
    std::fs::write(&source, b"changed synthetic source").unwrap();
    assert!(katran::video_backgrounds::prepare_event(&event).is_err());
    event.id = 42;
    assert!(katran::video_backgrounds::prepare_event(&event).is_err());
}
#[test]
fn downloads_action_imports_muted_copy_and_persists_video_theme() {
    use std::process::Command as Process;
    if Process::new("ffmpeg").arg("-version").output().is_err() {
        return;
    }
    let profile = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let source = downloads.path().join("synthetic-video.mp4");
    let status = Process::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=24",
            "-t",
            "1",
            "-c:v",
            "libx264",
            "-y",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let original = std::fs::read(&source).unwrap();
    let mut api = Service::open(profile.path(), 10_000_000).unwrap();
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings.downloads_folder = Some(downloads.path().to_owned());
    settings.downloads_enabled = true;
    call(&mut api, Command::SetSettings { settings });
    let event = source_api::wait_event(&mut api, SourceId::Downloads, &source);
    let id = event["id"].as_i64().unwrap();
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Downloads,
        },
    );
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    assert!(
        call(&mut api, Command::Actions)
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "use_as_video_background")
    );
    let job = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::UseSelectedVideoAsBackground),
        },
    );
    let job_id = JobId(job["job_id"].as_u64().unwrap());
    let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let result = loop {
        let status = call(&mut api, Command::PollJob { job_id });
        if status["state"] == "succeeded" {
            break status["result"].clone();
        }
        assert_ne!(status["state"], "failed", "{status}");
        assert!(std::time::Instant::now() < end);
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(result["background_applied"], true);
    let video_id = result["id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["video_background"],
        video_id
    );
    assert_eq!(
        call(&mut api, Command::VideoBackgrounds)["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    drop(api);
    let mut api = Service::open(profile.path(), 10_000_000).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["theme"]["video_background"],
        video_id
    );
    let mut theme: katran::theme::ThemePreferences =
        serde_json::from_value(call(&mut api, Command::Themes)["current"].clone()).unwrap();
    theme.video_enabled = Some(false);
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    let disabled: katran::theme::ThemePreferences =
        serde_json::from_value(call(&mut api, Command::Themes)["current"].clone()).unwrap();
    assert_eq!(
        disabled.video_background.as_deref(),
        Some(video_id.as_str())
    );
    assert_eq!(disabled.active_video(), None);
    theme.video_enabled = Some(true);
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    assert!(theme.active_video().is_some());
    theme.lily_background = Some(1);
    assert_eq!(
        api.execute(Request {
            api_version: 1,
            request_id: "conflicting-backgrounds".into(),
            command: Command::SetTheme { theme }
        })["ok"],
        false
    );
}

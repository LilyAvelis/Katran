use katran::{Command, Request, Service};
use serde_json::Value;
fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "emoji-font-test".into(),
        command,
    })
}
#[test]
fn defaults_migrate_and_selection_round_trips_without_modifying_text() {
    let old: katran::settings::Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.emoji_font, "apple");
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let fonts = call(&mut api, Command::EmojiFonts);
    assert_eq!(fonts["ok"], true, "{fonts}");
    assert_eq!(fonts["result"]["selected"], "apple");
    assert_eq!(
        fonts["result"]["fonts"][0]["available"],
        cfg!(katran_bundled_apple)
    );
    let system = call(
        &mut api,
        Command::ResolveEmojiFont {
            id: "system".into(),
        },
    );
    assert_eq!(system["result"]["available"], true);
    assert!(system["result"]["path"].is_null());
    let mut settings = old;
    settings.emoji_font = "system".into();
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["ok"],
        true
    );
    drop(api);
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["emoji_font"],
        "system"
    );
    for id in ["../private", "", "f", "../../apple"] {
        assert_eq!(
            call(&mut api, Command::ResolveEmojiFont { id: id.into() })["ok"],
            false
        );
    }
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
    settings.emoji_font = "a".repeat(64);
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["ok"],
        false
    );
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["emoji_font"],
        "system"
    );
}
#[cfg(windows)]
#[test]
fn valid_font_import_is_profile_local_deduplicated_and_does_not_select_itself() {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    let source =
        std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("Fonts/segoeui.ttf");
    let before = std::fs::read(&source).unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let imported = call(
        &mut api,
        Command::ImportEmojiFont {
            path: source.clone(),
        },
    );
    assert_eq!(imported["ok"], true, "{imported}");
    let id = imported["result"]["id"].as_str().unwrap();
    assert_eq!(id, format!("{:x}", Sha256::digest(&before)));
    let path = std::path::PathBuf::from(imported["result"]["path"].as_str().unwrap());
    assert!(path.starts_with(root.path().canonicalize().unwrap()));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert_eq!(
        call(&mut api, Command::ImportEmojiFont { path: source })["result"]["id"],
        id
    );
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["emoji_font"],
        "apple"
    );
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
    settings.emoji_font = id.into();
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["ok"],
        true
    );
    let invalid = root.path().join("bad.ttf");
    std::fs::write(&invalid, b"synthetic-invalid-font").unwrap();
    let started = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::ImportEmojiFont { path: invalid }),
        },
    );
    assert_eq!(started["ok"], true, "{started}");
    let job_id = katran::domain_jobs::JobId(started["result"]["job_id"].as_u64().unwrap());
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let state = call(&mut api, Command::PollJob { job_id });
        if state["result"]["state"] == "failed" {
            assert_eq!(state["result"]["error"]["code"], "invalid_font");
            break;
        }
        assert!(std::time::Instant::now() < end, "{state}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(all(windows, katran_bundled_apple))]
#[test]
fn bundled_font_is_private_verified_and_original_takes_precedence() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let font = call(&mut api, Command::ResolveEmojiFont { id: "apple".into() });
    assert_eq!(font["ok"], true, "{font}");
    let path = std::path::PathBuf::from(font["result"]["path"].as_str().unwrap());
    assert_eq!(path.file_name().unwrap(), "apple-bundled96.ttf");
    assert!(path.starts_with(root.path().canonicalize().unwrap()));
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 66_360_392);
    assert!(katran::emoji_render::probe_font(&path).is_ok());
    let original = path.parent().unwrap().join("apple.ttf");
    std::fs::write(&original, b"user file remains untouched").unwrap();
    let picked = call(&mut api, Command::ResolveEmojiFont { id: "apple".into() });
    assert_eq!(
        std::path::PathBuf::from(picked["result"]["path"].as_str().unwrap()),
        original
    );
    assert_eq!(
        std::fs::read(original).unwrap(),
        b"user file remains untouched"
    );
}

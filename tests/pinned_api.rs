#[path = "common/source_api.rs"]
mod source_api;

use katran::{Command, Request, Service, source_policy::SourceId};
use source_api::call;

fn ingest_text(service: &mut Service, text: &str) -> i64 {
    call(
        service,
        Command::IngestText {
            text: text.into(),
            source: "synthetic".into(),
        },
    )["id"]
        .as_i64()
        .unwrap()
}

fn exists(service: &mut Service, id: i64) -> bool {
    service.execute(Request {
        api_version: 1,
        request_id: "exists".into(),
        command: Command::Get { id },
    })["ok"]
        == true
}

#[test]
fn pinned_shelf_deletion_requires_unpin_and_reports_skipped_items() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    call(
        &mut service,
        Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: true,
        },
    );
    let first = ingest_text(&mut service, "protected synthetic text");
    let second = ingest_text(&mut service, "unprotected synthetic text");
    call(&mut service, Command::TogglePins { ids: vec![first] });
    let pending = call(
        &mut service,
        Command::DeleteSelection {
            ids: vec![first, second, first],
            confirmed: false,
        },
    );
    assert_eq!(pending["confirmation_required"], true);
    assert_eq!(pending["ids"], serde_json::json!([first, second]));
    let deleted = call(
        &mut service,
        Command::DeleteSelection {
            ids: vec![first, second],
            confirmed: true,
        },
    );
    assert_eq!(deleted["deleted"], 1);
    assert_eq!(deleted["items"][0]["skipped_pinned"], true);
    assert_eq!(deleted["items"][0]["ok"], false);
    assert!(exists(&mut service, first));
    assert!(!exists(&mut service, second));
    let direct = call(
        &mut service,
        Command::DeleteEvents {
            ids: vec![first],
            confirmed: true,
        },
    );
    assert_eq!(direct["deleted"], 0);
    assert_eq!(direct["items"][0]["skipped_pinned"], true);
    call(&mut service, Command::TogglePins { ids: vec![first] });
    let after = call(
        &mut service,
        Command::DeleteEvents {
            ids: vec![first],
            confirmed: true,
        },
    );
    assert_eq!(after["deleted"], 1);
    assert!(!exists(&mut service, first));
}

#[test]
fn pinned_external_alias_prevents_original_deletion() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("synthetic.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([20, 30, 40, 255]))
        .save(&path)
        .unwrap();
    let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut service, Command::GetSettings)).unwrap();
    settings.downloads_enabled = false;
    settings.sharex_folder = Some(source.path().into());
    call(&mut service, Command::SetSettings { settings });
    let original = source_api::wait_event(&mut service, SourceId::Sharex, &path)["id"]
        .as_i64()
        .unwrap();
    let alias = call(
        &mut service,
        Command::IngestPath {
            path: path.clone(),
            source: "synthetic".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    assert_ne!(original, alias);
    call(&mut service, Command::TogglePins { ids: vec![alias] });
    let result = call(
        &mut service,
        Command::DeleteSelection {
            ids: vec![original],
            confirmed: false,
        },
    );
    assert_eq!(result["deleted"], 0);
    assert_eq!(result["items"][0]["skipped_pinned"], true);
    assert_eq!(result["items"][0]["ok"], false);
    assert!(path.is_file());
    assert!(exists(&mut service, original));
    assert!(exists(&mut service, alias));
}

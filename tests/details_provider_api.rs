//! Cross-provider details routes: synthetic snapshots only, no native shell or clipboard.
use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;

fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "details-provider".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let r = reply(api, command);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn desktop_id_and_folder_snapshot_use_distinct_read_only_preview_routes() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("image.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 255]))
        .save(&source)
        .unwrap();
    let desktop_source = folder.path().join("portrait");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([7, 8, 9, 255]))
        .save_with_format(&desktop_source, image::ImageFormat::Png)
        .unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    call(
        &mut api,
        Command::ImportThemeBackground {
            path: desktop_source,
        },
    );
    call(
        &mut api,
        Command::ChooseFolder {
            path: folder.path().into(),
        },
    );
    let page = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: "image.png".into(),
            limit: 10,
            cursor: None,
        },
    );
    let snapshot = &page["events"][0];
    assert_eq!(snapshot["source"], "work_folder");
    let folder_id = snapshot["id"].as_i64().unwrap();
    assert!(folder_id < 0);
    let revision = snapshot["revision"].as_str().unwrap().to_owned();
    let path = std::path::PathBuf::from(snapshot["path"].as_str().unwrap());
    let mut worker = Service::open_read_only(root.path()).unwrap();
    assert_eq!(
        reply(
            &mut worker,
            Command::PreviewImage {
                id: folder_id,
                max_edge: 64
            }
        )["ok"],
        false
    );
    let result = call(
        &mut worker,
        Command::PreviewPath {
            path: path.clone(),
            revision: revision.clone(),
            max_edge: 64,
            image_only: true,
        },
    );
    assert_eq!(result["revision"], revision);
    std::fs::write(&path, b"changed synthetic source").unwrap();
    assert_eq!(
        reply(
            &mut worker,
            Command::PreviewPath {
                path,
                revision,
                max_edge: 64,
                image_only: true
            }
        )["error"]["code"],
        "source_changed"
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
            limit: 10,
            cursor: None,
        },
    );
    let desktop = &page["events"][0];
    assert_eq!(desktop["source"], "desktop");
    assert_eq!(desktop["name"], "portrait");
    let id = desktop["id"].as_i64().unwrap();
    assert!(id < 0 && id != folder_id);
    let result = call(&mut worker, Command::PreviewImage { id, max_edge: 64 });
    assert_eq!(result["revision"], desktop["revision"]);
    assert!(
        result["width"]
            .as_u64()
            .is_some_and(|width| width > 0 && width <= 64)
    );
}

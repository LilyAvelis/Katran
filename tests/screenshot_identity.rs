use katran::{Event, Result, preview, storage};
#[path = "../src/screenshot_identity.rs"]
mod screenshot_identity;
use screenshot_identity::matched_clipboard_ids;

fn event(id: i64, source: &str, path: &std::path::Path) -> Event {
    Event {
        id,
        source: source.into(),
        revision: format!("ref:{}", preview::content_hash(path).unwrap()),
        origin: Default::default(),
        detected_at_ms: 0,
        name: "image".into(),
        storage: "reference".into(),
        path: path.into(),
        size: std::fs::metadata(path).unwrap().len(),
        available: true,
    }
}

#[test]
fn pixel_matches_are_durable_scoped_and_validate_arrival_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("source.png");
    let copy = dir.path().join("copy.png");
    let unrelated = dir.path().join("other.png");
    let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
    image.save(&original).unwrap();
    image::RgbImage::from_pixel(3, 2, image::Rgb([10, 20, 30]))
        .save(&copy)
        .unwrap();
    assert_ne!(
        std::fs::read(&original).unwrap(),
        std::fs::read(&copy).unwrap()
    );
    image::RgbaImage::from_pixel(3, 2, image::Rgba([99, 20, 30, 255]))
        .save(&unrelated)
        .unwrap();
    let source = event(1, "sharex", &original);
    let clipboard = event(2, "clipboard_image", &copy);
    let other = event(3, "clipboard", &unrelated);
    let db = rusqlite::Connection::open_in_memory().unwrap();
    let events = vec![source.clone(), clipboard.clone(), other];
    assert_eq!(matched_clipboard_ids(&db, &events).unwrap(), [2].into());
    std::fs::remove_file(&original).unwrap();
    let mut missing = source.clone();
    missing.available = false;
    assert_eq!(
        matched_clipboard_ids(&db, &[missing.clone(), clipboard.clone()]).unwrap(),
        [2].into()
    );
    assert!(
        matched_clipboard_ids(&db, std::slice::from_ref(&clipboard))
            .unwrap()
            .is_empty()
    );
    let fresh = rusqlite::Connection::open_in_memory().unwrap();
    assert!(
        matched_clipboard_ids(&fresh, &[missing, clipboard.clone()])
            .unwrap()
            .is_empty()
    );
    // Changed originals may not identify the previous arrival, even when pixels now match.
    image.save(&original).unwrap();
    let stale = event(4, "sharex", &unrelated);
    image.save(&unrelated).unwrap();
    assert!(
        matched_clipboard_ids(&fresh, &[stale, clipboard])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn worker_preparation_is_pure_and_commit_rejects_stale_arrivals() {
    use katran::{Command, Request, Service};
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("source.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    let mut api = Service::open(root.path(), 1_000_000).unwrap();
    let mut call = |command| {
        let response = api.execute(Request {
            api_version: 1,
            request_id: "identity".into(),
            command,
        });
        assert_eq!(response["ok"], true, "{response}");
        serde_json::from_value::<Event>(response["result"].clone()).unwrap()
    };
    let source = call(Command::IngestPath {
        path: path.clone(),
        source: "sharex".into(),
    });
    let clipboard = call(Command::IngestBytes {
        name: "clipboard.png".into(),
        bytes: std::fs::read(&path).unwrap(),
        source: "clipboard".into(),
    });
    let pairs = screenshot_identity::prepare_matches(&[source.clone(), clipboard.clone()]);
    assert_eq!(pairs.len(), 1);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    assert_eq!(screenshot_identity::commit_matches(&db, &pairs).unwrap(), 1);
    assert_eq!(screenshot_identity::commit_matches(&db, &pairs).unwrap(), 0);
    let mut stale = pairs[0].clone();
    stale.clipboard_revision = "stale".into();
    assert_eq!(
        screenshot_identity::commit_matches(&db, &[stale]).unwrap(),
        0
    );
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        screenshot_identity::known_clipboard_ids(&db, &[source, clipboard]).unwrap(),
        [pairs[0].clipboard_id].into()
    );
}

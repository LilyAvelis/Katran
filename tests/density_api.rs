use katran::{Command, Request, Service, interaction::View};
fn call(s: &mut Service, c: Command) -> serde_json::Value {
    s.execute(Request {
        api_version: 1,
        request_id: "density".into(),
        command: c,
    })
}
#[test]
fn density_is_discrete_per_view_validated_and_persistent() {
    let root = tempfile::tempdir().unwrap();
    let mut s = Service::open(root.path(), 1000).unwrap();
    let r = call(&mut s, Command::AdjustDensity { delta: 1 });
    assert_eq!(r["result"]["gallery_columns"], 5);
    assert_eq!(r["result"]["stream_rows"], 3);
    call(&mut s, Command::SetView { view: View::List });
    let r = call(&mut s, Command::AdjustDensity { delta: 1 });
    assert_eq!(r["result"]["stream_rows"], 4);
    assert_eq!(
        call(&mut s, Command::AdjustDensity { delta: 127 })["result"]["stream_rows"],
        12
    );
    assert_eq!(
        call(&mut s, Command::AdjustDensity { delta: -128 })["result"]["stream_rows"],
        1
    );
    drop(s);
    let mut s = Service::open(root.path(), 1000).unwrap();
    let r = call(&mut s, Command::GetSettings);
    assert_eq!(r["result"]["gallery_columns"], 5);
    assert_eq!(r["result"]["stream_rows"], 1);
    let old: katran::settings::Settings = serde_json::from_str("{}").unwrap();
    assert_eq!((old.gallery_columns, old.stream_rows), (4, 3));
}

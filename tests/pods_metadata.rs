use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;
fn call(api: &mut Service, c: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "pods-meta".into(),
        command: c,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn names_codepoints_and_density_are_domain_owned() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
    );
    for (text, name, cp) in [
        ("🐄", "cow", "U+1F404"),
        ("🤡", "clown face", "U+1F921"),
        ("⚰️", "coffin", "U+26B0"),
        ("💀", "skull", "U+1F480"),
        ("🦐", "shrimp", "U+1F990"),
    ] {
        let id = call(
            &mut api,
            Command::IngestText {
                text: text.into(),
                source: "test".into(),
            },
        )["id"]
            .as_i64()
            .unwrap();
        let info = call(&mut api, Command::DescribePod { id });
        assert_eq!(info["name"], name);
        assert_eq!(info["text"], text);
        assert_eq!(info["codepoints"][0]["codepoint"], cp);
    }
    let before = call(&mut api, Command::GetSettings);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
    );
    call(&mut api, Command::AdjustDensity { delta: 1 });
    let after = call(&mut api, Command::GetSettings);
    assert_eq!(after["pods_tile_size"], 84);
    assert_eq!(after["gallery_columns"], before["gallery_columns"]);
    call(&mut api, Command::AdjustDensity { delta: -1 });
    assert_eq!(call(&mut api, Command::GetSettings)["pods_tile_size"], 96);
}

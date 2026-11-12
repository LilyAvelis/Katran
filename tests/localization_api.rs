use katran::{Command, Request, Service, settings::Language};

fn call(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "language-test".into(),
        command,
    })
}

#[test]
fn language_defaults_and_persists_through_dispatcher() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    let initial = call(&mut api, Command::GetSettings);
    assert_eq!(initial["result"]["language"], "en");
    assert_eq!(initial["result"]["theme"]["preset"], "blue_experimental");
    assert_eq!(initial["result"]["theme"]["lily_background"], 1);
    let mut settings: katran::settings::Settings =
        serde_json::from_value(initial["result"].clone()).unwrap();
    settings.language = Language::Russian;
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["ok"],
        true
    );
    drop(api);

    let mut api = Service::open(root.path(), 10_000).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["language"],
        "ru"
    );
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
    settings.language = Language::English;
    assert_eq!(
        call(&mut api, Command::SetSettings { settings })["ok"],
        true
    );
    drop(api);
    let mut api = Service::open(root.path(), 10_000).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["language"],
        "en"
    );
}

#[test]
fn old_documents_load_russian_and_unknown_languages_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    let mut document = call(&mut api, Command::GetSettings)["result"].clone();
    document.as_object_mut().unwrap().remove("language");
    drop(api);
    let db = rusqlite::Connection::open(root.path().join("index.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO preferences(key,document) VALUES('general',?1)",
        [document.to_string()],
    )
    .unwrap();
    drop(db);
    let mut api = Service::open(root.path(), 10_000).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["result"]["language"],
        "ru"
    );
    document["language"] = "fr".into();
    assert!(serde_json::from_value::<katran::settings::Settings>(document).is_err());
}

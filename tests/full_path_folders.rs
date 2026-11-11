use katran::{Command, Request, Service, everything::direct_folder_query};
use serde_json::Value;
use std::path::PathBuf;
fn request(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "full-path".into(),
        command,
    })
}
#[test]
fn lexical_windows_paths_and_quotes_need_no_filesystem() {
    for value in [
        r"C:\папка\name",
        "C:/folder",
        r"\\server\share\folder",
        r"\\?\C:\folder",
        r"\\?\UNC\server\share\folder",
    ] {
        assert_eq!(
            direct_folder_query(&format!("  \"{value}\"  ")).unwrap(),
            Some(PathBuf::from(value))
        );
    }
    for value in ["report", r"C:relative", r"folder\child", r"\single-root"] {
        assert_eq!(direct_folder_query(value).unwrap(), None, "{value}");
    }
    assert!(direct_folder_query("C:\\bad\0name").is_err());
    assert!(direct_folder_query(&format!("C:\\{}", "a".repeat(16001))).is_err());
}
#[test]
fn dispatcher_finds_and_selects_literal_directory_without_index() {
    let root = tempfile::tempdir().unwrap();
    let tree = tempfile::tempdir().unwrap();
    let folder = tree.path().join("Катран синтетическая папка");
    std::fs::create_dir(&folder).unwrap();
    let file = tree.path().join("file.txt");
    std::fs::write(&file, b"synthetic").unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let query = format!("  \"{}\"  ", folder.display());
    let found = request(
        &mut api,
        Command::SearchFolders {
            query: query.clone(),
            limit: 100,
        },
    );
    assert_eq!(found["ok"], true, "{found}");
    assert_eq!(found["result"]["items"].as_array().unwrap().len(), 1);
    let parsed = direct_folder_query(&query).unwrap().unwrap();
    let chosen = request(&mut api, Command::ChooseFolder { path: parsed });
    assert_eq!(chosen["ok"], true, "{chosen}");
    let before = request(&mut api, Command::CurrentFolder)["result"].clone();
    for path in [file, tree.path().join("missing")] {
        let result = request(
            &mut api,
            Command::SearchFolders {
                query: path.to_string_lossy().into_owned(),
                limit: 100,
            },
        );
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(result["error"]["code"], "invalid_folder");
        let choose = request(&mut api, Command::ChooseFolder { path });
        assert_eq!(choose["ok"], false, "{choose}");
        assert_eq!(request(&mut api, Command::CurrentFolder)["result"], before);
    }
}

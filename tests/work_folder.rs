use katran::{
    sorting::{SortMode, SortSpec},
    work_folder::State,
};
use std::fs;

fn browse(state: &State, query: &str, limit: u32, cursor: Option<&str>) -> serde_json::Value {
    state
        .browse(
            query,
            limit,
            cursor,
            SortSpec {
                mode: SortMode::Name,
                ..Default::default()
            },
        )
        .unwrap()
}
#[test]
fn live_pages_and_session_ids_are_invalidated() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"a").unwrap();
    fs::write(dir.path().join("b.txt"), b"bb").unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested").join("hidden.txt"), b"x").unwrap();
    let mut state = State::default();
    state.choose(dir.path()).unwrap();
    let first = browse(&state, "", 1, None);
    let first_id = first["events"][0]["id"].as_i64().unwrap();
    assert!(first_id < 0);
    assert_eq!(first["events"][0]["name"], "a.txt");
    let cursor = first["next_cursor"].as_str().unwrap();
    assert_eq!(
        browse(&state, "", 1, Some(cursor))["events"][0]["name"],
        "b.txt"
    );
    assert_eq!(
        browse(&state, "A.TXT", 10, None)["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::remove_file(dir.path().join("a.txt")).unwrap();
    assert!(state.get(first_id).is_err());
    assert!(
        state
            .browse(
                "",
                1,
                Some(cursor),
                SortSpec {
                    mode: SortMode::Name,
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(
        browse(&state, "", 10, None)["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::write(dir.path().join("a.txt"), b"new content").unwrap();
    let later = browse(&state, "", 10, None);
    let new_id = later["events"][0]["id"].as_i64().unwrap();
    assert!(new_id < first_id);
    state.choose(dir.path()).unwrap();
    assert!(state.get(new_id).is_err());
    assert!(
        browse(&state, "", 10, None)["events"][0]["id"]
            .as_i64()
            .unwrap()
            < new_id
    );
}
#[test]
fn changed_file_and_cursor_context_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("one"), b"x").unwrap();
    fs::write(dir.path().join("two"), b"y").unwrap();
    let mut state = State::default();
    state.choose(dir.path()).unwrap();
    let first = browse(&state, "", 1, None);
    let id = first["events"][0]["id"].as_i64().unwrap();
    let cursor = first["next_cursor"].as_str().unwrap();
    assert!(
        state
            .browse(
                "different",
                1,
                Some(cursor),
                SortSpec {
                    mode: SortMode::Name,
                    ..Default::default()
                }
            )
            .is_err()
    );
    fs::write(dir.path().join("one"), b"different length").unwrap();
    assert!(state.get(id).is_err());
    assert!(
        state
            .browse(
                "",
                1,
                Some(cursor),
                SortSpec {
                    mode: SortMode::Name,
                    ..Default::default()
                }
            )
            .is_err()
    );
}
#[cfg(unix)]
#[test]
fn symlink_files_are_excluded() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside"), b"x").unwrap();
    std::os::unix::fs::symlink(outside.path().join("outside"), dir.path().join("link")).unwrap();
    let mut state = State::default();
    state.choose(dir.path()).unwrap();
    assert!(
        browse(&state, "", 10, None)["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

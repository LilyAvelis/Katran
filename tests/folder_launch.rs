#[path = "../src/folder_launch.rs"]
mod folder_launch;
use std::{fs, path::Path};

#[test]
fn queue_preserves_unicode_and_multiple_folders() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    fs::create_dir(&profile).unwrap();
    let one = temp.path().join("папка & test");
    let two = temp.path().join("folder two");
    fs::create_dir(&one).unwrap();
    fs::create_dir(&two).unwrap();
    folder_launch::enqueue(&profile, &one).unwrap();
    folder_launch::enqueue(&profile, &two).unwrap();
    let got: Vec<_> = folder_launch::take(&profile)
        .unwrap()
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        got,
        vec![
            fs::canonicalize(one).unwrap(),
            fs::canonicalize(two).unwrap()
        ]
    );
    assert!(folder_launch::take(&profile).unwrap().is_empty());
}

#[test]
fn invalid_request_does_not_block_other_items() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let folder = root.join("valid");
    fs::create_dir(&folder).unwrap();
    assert!(folder_launch::enqueue(root, Path::new("relative")).is_err());
    let file = root.join("file");
    fs::write(&file, b"data").unwrap();
    assert!(folder_launch::enqueue(root, &file).is_err());
    folder_launch::enqueue(root, &folder).unwrap();
    let queue = root.join("launch-requests");
    let first = fs::read_dir(&queue)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(first, b"not json").unwrap();
    folder_launch::enqueue(root, &folder).unwrap();
    let items = folder_launch::take(root).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(items.iter().filter(|r| r.is_err()).count(), 1);
    assert!(folder_launch::take(root).unwrap().is_empty());
    assert_eq!(fs::read(&file).unwrap(), b"data");
}

#[test]
fn queue_is_bounded_and_rejects_invalid_directory() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("folder");
    fs::create_dir(&folder).unwrap();
    for _ in 0..32 {
        folder_launch::enqueue(temp.path(), &folder).unwrap();
    }
    assert!(folder_launch::enqueue(temp.path(), &folder).is_err());
    assert_eq!(folder_launch::take(temp.path()).unwrap().len(), 32);
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("launch-requests"), b"untouched").unwrap();
    assert!(folder_launch::enqueue(other.path(), &folder).is_err());
    assert_eq!(
        fs::read(other.path().join("launch-requests")).unwrap(),
        b"untouched"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_queue() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("launch-requests")).unwrap();
    assert!(folder_launch::enqueue(temp.path(), outside.path()).is_err());
}

#[cfg(unix)]
#[test]
fn accepts_target_directory_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    folder_launch::enqueue(temp.path(), &link).unwrap();
    let result = folder_launch::take(temp.path())
        .unwrap()
        .pop()
        .unwrap()
        .unwrap();
    assert_eq!(result, fs::canonicalize(target).unwrap());
}

#[path = "../src/files_pointer.rs"]
mod files_pointer;

use files_pointer::prepare_pointer;
use std::fs;

#[test]
fn prepares_reference_without_copy_or_source_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("original.bin");
    let folder = temp.path().join("folder");
    fs::write(&source, b"synthetic").unwrap();
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("original.bin"), b"existing").unwrap();
    let pointer = prepare_pointer(&source).unwrap();
    assert_eq!(pointer.path, fs::canonicalize(&source).unwrap());
    assert_eq!(pointer.size, 9);
    assert_eq!(pointer.content_hash.len(), 64);
    assert_eq!(fs::read(source).unwrap(), b"synthetic");
    assert_eq!(fs::read(folder.join("original.bin")).unwrap(), b"existing");
    assert_eq!(fs::read_dir(folder).unwrap().count(), 1);
}

#[test]
fn rejects_changed_missing_and_nonfile_inputs_individually() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("original.bin");
    fs::write(&source, b"one").unwrap();
    let pointer = prepare_pointer(&source).unwrap();
    assert!(prepare_pointer(&source).is_ok());
    assert!(prepare_pointer(&temp.path().join("missing")).is_err());
    assert!(prepare_pointer(temp.path()).is_err());
    assert!(prepare_pointer(std::path::Path::new("relative.bin")).is_err());
    fs::write(&source, b"different").unwrap();
    assert_ne!(prepare_pointer(&source).unwrap(), pointer);
    fs::remove_file(&source).unwrap();
    assert!(prepare_pointer(&source).is_err());
}

#[cfg(unix)]
#[test]
fn rejects_final_symlink() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let link = temp.path().join("link");
    fs::write(&source, b"synthetic").unwrap();
    symlink(&source, &link).unwrap();
    assert!(prepare_pointer(&link).is_err());
}

use katran::{downloads::DownloadsScanner, sharex::SharexScanner};
use std::fs;

#[test]
fn scanners_ignore_reserved_recovery_trees_but_keep_other_hidden_content() {
    let root = tempfile::tempdir().unwrap();
    let recovery = root.path().join(".katran-recovery-synthetic/data");
    let nested = root.path().join("normal/.katran-recovery-another/data");
    let other = root.path().join(".other-hidden");
    fs::create_dir_all(&recovery).unwrap();
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::write(recovery.join("original.png"), b"staged").unwrap();
    fs::write(nested.join("other.png"), b"staged").unwrap();
    let expected = other.join("visible.png");
    fs::write(&expected, b"normal").unwrap();
    let similarly_named_file = root.path().join(".katran-recovery-not-a-dir.png");
    fs::write(&similarly_named_file, b"normal too").unwrap();
    let mut images = SharexScanner::default();
    let mut downloads = DownloadsScanner::default();
    assert!(images.scan(root.path()).unwrap().is_empty());
    assert!(downloads.scan(root.path()).unwrap().is_empty());
    let mut expected = vec![
        expected.canonicalize().unwrap(),
        similarly_named_file.canonicalize().unwrap(),
    ];
    expected.sort();
    assert_eq!(images.scan(root.path()).unwrap(), expected);
    assert_eq!(downloads.scan(root.path()).unwrap(), expected);
}

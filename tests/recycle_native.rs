#![cfg(windows)]
use katran::{downloads::RecycleOnlyTrash, preview, safe_recycle::recycle_verified};

#[test]
#[ignore = "Native Shell smoke: run explicitly outside sandbox on disposable data"]
fn synthetic_file_is_recycled_through_the_native_adapter() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("katran-recycle-smoke.txt");
    std::fs::write(&original, b"Katran disposable recycle smoke fixture").unwrap();
    let hash = preview::content_hash(&original).unwrap();
    let receipt = recycle_verified(&original, root.path(), &hash, &RecycleOnlyTrash).unwrap();
    assert_eq!(receipt.status, "recycled", "{receipt:?}");
    assert!(receipt.recycled);
    assert!(!original.exists());
}

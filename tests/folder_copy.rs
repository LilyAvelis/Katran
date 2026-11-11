use katran::{
    Event,
    copy_out::{self, ClipboardSink},
    preview,
};
use std::path::{Path, PathBuf};

fn event(id: i64, path: &Path) -> Event {
    Event {
        kind: Default::default(),
        text_preview: None,
        delete_policy: Default::default(),
        revision: preview::revision(path, None),
        origin: Default::default(),
        id,
        detected_at_ms: 0,
        modified_at_ms: None,
        source: "work_folder".into(),
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        storage: "reference".into(),
        path: path.into(),
        size: std::fs::metadata(path).unwrap().len(),
        available: true,
    }
}
struct FakeSink(Vec<PathBuf>);
impl ClipboardSink for FakeSink {
    fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
        self.0 = paths.to_vec();
        Ok((17, vec![]))
    }
}

#[test]
fn folder_files_become_independent_named_file_payloads_without_reclassification() {
    let root = tempfile::tempdir().unwrap();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = a.path().join("same.csv");
    let second = b.path().join("same.csv");
    std::fs::write(&first, "a,b\n🐄,x\n").unwrap();
    std::fs::write(&second, "a,b\n🐄,x\n").unwrap();
    let plan = copy_out::prepare(root.path(), &[event(-1, &first), event(-2, &second)]).unwrap();
    assert!(plan.failures.is_empty());
    assert_eq!(plan.paths.len(), 2);
    assert_eq!(plan.paths[0].file_name().unwrap(), "same.csv");
    assert_eq!(plan.paths[1].file_name().unwrap(), "same (1).csv");
    let mut sink = FakeSink(vec![]);
    let receipt = copy_out::commit(&plan, &mut sink).unwrap();
    assert_eq!(receipt.count, 2);
    assert_eq!(sink.0, plan.paths);
    std::fs::write(&first, b"changed").unwrap();
    assert_eq!(std::fs::read(&sink.0[0]).unwrap(), "a,b\n🐄,x\n".as_bytes());
    assert_eq!(std::fs::read(&second).unwrap(), "a,b\n🐄,x\n".as_bytes());
}

#[test]
fn stale_folder_event_is_rejected_and_normal_reference_contract_is_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("note.txt");
    std::fs::write(&source, b"first").unwrap();
    let original = event(-1, &source);
    std::fs::write(&source, b"changed length").unwrap();
    let stale = copy_out::prepare(root.path(), &[original]).unwrap();
    assert!(stale.paths.is_empty());
    assert!(stale.failures[0].error.starts_with("source_changed:"));
    let mut unrelated = event(5, &source);
    unrelated.source = "manual".into();
    let ordinary = copy_out::prepare(root.path(), &[unrelated]).unwrap();
    assert!(ordinary.paths.is_empty());
    assert!(ordinary.failures[0].error.starts_with("source_changed:"));
}

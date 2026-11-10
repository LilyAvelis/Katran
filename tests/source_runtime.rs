use katran::{
    source_policy::{SourceConfiguration, SourceId},
    source_runtime::{SourceRuntime, SourceState},
};
use std::time::{Duration, Instant};
fn config(root: &std::path::Path, generation: u64, enabled: bool) -> SourceConfiguration {
    SourceConfiguration {
        source_id: SourceId::Downloads,
        root: root.into(),
        generation,
        enabled,
    }
}
#[test]
fn setup_failures_are_asynchronous_and_explicit_retry_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let mut runtime = SourceRuntime::new();
    runtime.configure(vec![config(&missing, 1, true)]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let batch = runtime.poll(8);
        if batch.statuses[0].state == SourceState::Failed {
            assert!(batch.statuses[0].error.is_some());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::create_dir(&missing).unwrap();
    // Merely polling does not hide the failure or silently enable polling fallback.
    assert_eq!(runtime.poll(8).statuses[0].state, SourceState::Failed);
    runtime.rescan(SourceId::Downloads).unwrap();
    #[cfg(windows)]
    loop {
        if runtime.poll(8).statuses[0].state == SourceState::Watching {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn rapid_reconfiguration_keeps_workers_bounded_and_disabled_sources_stopped() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = SourceRuntime::new();
    for generation in 1..300 {
        runtime
            .configure(vec![config(temp.path(), generation, true)])
            .unwrap();
        assert!(runtime.statuses()[0].worker_count <= 2);
    }
    runtime
        .configure(vec![config(temp.path(), 300, false)])
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = runtime.poll(8).statuses.remove(0);
        assert!(!status.enabled);
        assert_eq!(status.state, SourceState::Disabled);
        if status.worker_count == 0 {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn duplicate_configuration_is_rejected_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = SourceRuntime::new();
    let c = config(temp.path(), 1, false);
    assert!(runtime.configure(vec![c.clone(), c]).is_err());
    assert!(runtime.statuses().is_empty());
}

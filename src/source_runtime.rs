//! Domain-owned bounded native source lifecycle. Prepared values never cross the wire.
use crate::{
    ApiError, Result,
    folder_watch::{FolderConfig, FolderEvent, FolderWatcher, PreparedCandidate},
    source_policy::{SourceConfiguration, SourceId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Disabled,
    Starting,
    Watching,
    Failed,
    Stopping,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceStatus {
    pub source_id: SourceId,
    pub generation: u64,
    pub root: std::path::PathBuf,
    pub enabled: bool,
    pub state: SourceState,
    pub error: Option<String>,
    pub backlog: usize,
    pub worker_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRuntimeError {
    pub source_id: SourceId,
    pub message: String,
}
#[derive(Debug)]
pub(crate) struct PreparedSourceFile {
    pub(crate) candidate: PreparedCandidate,
    pub(crate) configuration: SourceConfiguration,
}
pub(crate) struct SourceSnapshot {
    pub(crate) configuration: SourceConfiguration,
    pub(crate) paths: Vec<std::path::PathBuf>,
}
pub struct RuntimeBatch {
    pub(crate) snapshots: Vec<SourceSnapshot>,
    pub(crate) prepared: Vec<PreparedSourceFile>,
    pub refresh: Vec<SourceId>,
    pub statuses: Vec<SourceStatus>,
    pub errors: Vec<SourceRuntimeError>,
}
struct Slot {
    configuration: SourceConfiguration,
    active: Option<FolderWatcher>,
    retired: Vec<FolderWatcher>,
    state: SourceState,
    error: Option<String>,
}
#[derive(Default)]
pub struct SourceRuntime {
    slots: Vec<Slot>,
    cursor: usize,
}
impl SourceRuntime {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn configure(&mut self, configurations: Vec<SourceConfiguration>) -> Result<()> {
        if configurations.len() > 2
            || configurations.iter().enumerate().any(|(i, c)| {
                configurations[..i]
                    .iter()
                    .any(|other| other.source_id == c.source_id)
            })
        {
            return Err(ApiError::new(
                "invalid_argument",
                "Sources must be unique and at most two",
            ));
        }
        for slot in &mut self.slots {
            if !configurations
                .iter()
                .any(|c| c.source_id == slot.configuration.source_id)
            {
                slot.configuration.enabled = false;
                Self::retire(slot);
            }
        }
        for configuration in configurations {
            if let Some(slot) = self
                .slots
                .iter_mut()
                .find(|s| s.configuration.source_id == configuration.source_id)
            {
                if slot.configuration != configuration {
                    Self::retire(slot);
                    slot.configuration = configuration;
                    slot.error = None;
                }
            } else {
                self.slots.push(Slot {
                    configuration,
                    active: None,
                    retired: Vec::new(),
                    state: SourceState::Disabled,
                    error: None,
                });
            }
        }
        self.maintain();
        Ok(())
    }
    fn retire(slot: &mut Slot) {
        if let Some(worker) = slot.active.take() {
            worker.stop();
            slot.retired.push(worker);
        }
        slot.state = if slot.configuration.enabled {
            SourceState::Stopping
        } else {
            SourceState::Disabled
        };
    }
    fn maintain(&mut self) {
        for slot in &mut self.slots {
            slot.retired.retain(|worker| !worker.is_finished());
            // Never replace a second worker while an older root still owns I/O.
            if slot.active.is_none()
                && slot.retired.len() < 2
                && slot.configuration.enabled
                && slot.error.is_none()
            {
                match FolderWatcher::start(vec![FolderConfig {
                    source: slot.configuration.source_id.into(),
                    root: slot.configuration.root.clone(),
                }]) {
                    Ok(worker) => {
                        slot.active = Some(worker);
                        slot.state = SourceState::Starting;
                    }
                    Err(error) => {
                        slot.error = Some(error.message);
                        slot.state = SourceState::Failed;
                    }
                }
            }
            if !slot.configuration.enabled {
                slot.state = SourceState::Disabled;
            }
        }
    }
    pub fn stop(&mut self, source_id: SourceId) -> Result<()> {
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.configuration.source_id == source_id)
            .ok_or_else(|| ApiError::new("unknown_source", "Source is not configured"))?;
        slot.configuration.enabled = false;
        Self::retire(slot);
        Ok(())
    }
    pub fn rescan(&mut self, source_id: SourceId) -> Result<()> {
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.configuration.source_id == source_id)
            .ok_or_else(|| ApiError::new("unknown_source", "Source is not configured"))?;
        if let Some(worker) = &slot.active
            && slot.error.is_none()
        {
            worker.rescan();
            return Ok(());
        }
        Self::retire(slot);
        slot.error = None;
        self.maintain();
        Ok(())
    }
    pub fn statuses(&self) -> Vec<SourceStatus> {
        self.slots
            .iter()
            .map(|s| SourceStatus {
                source_id: s.configuration.source_id,
                generation: s.configuration.generation,
                root: s.configuration.root.clone(),
                enabled: s.configuration.enabled,
                state: s.state,
                error: s.error.clone(),
                backlog: s.active.as_ref().map_or(0, FolderWatcher::backlog),
                worker_count: s.retired.len() + usize::from(s.active.is_some()),
            })
            .collect()
    }
    pub fn poll(&mut self, limit: u8) -> RuntimeBatch {
        self.maintain();
        let mut batch = RuntimeBatch {
            prepared: Vec::new(),
            snapshots: Vec::new(),
            refresh: Vec::new(),
            statuses: Vec::new(),
            errors: Vec::new(),
        };
        let count = self.slots.len();
        for offset in 0..count {
            let index = (self.cursor + offset) % count;
            let slot = &mut self.slots[index];
            for _ in 0..8 {
                if batch.prepared.len() >= usize::from(limit.min(8)) {
                    break;
                }
                let Some(event) = slot.active.as_ref().and_then(FolderWatcher::try_recv) else {
                    break;
                };
                match event {
                    FolderEvent::Ready { .. } => {
                        slot.state = SourceState::Watching;
                        slot.error = None;
                    }
                    FolderEvent::Snapshot { paths, .. } => {
                        slot.state = SourceState::Watching;
                        slot.error = None;
                        batch.snapshots.retain(|snapshot| {
                            snapshot.configuration.source_id != slot.configuration.source_id
                        });
                        batch.snapshots.push(SourceSnapshot {
                            configuration: slot.configuration.clone(),
                            paths,
                        });
                    }
                    FolderEvent::Candidate(candidate) => batch.prepared.push(PreparedSourceFile {
                        candidate,
                        configuration: slot.configuration.clone(),
                    }),
                    FolderEvent::Refresh { source } => {
                        let id = source.into();
                        if !batch.refresh.contains(&id) {
                            batch.refresh.push(id);
                        }
                    }
                    FolderEvent::Error { message, .. } => {
                        slot.state = SourceState::Failed;
                        slot.error = Some(message.clone());
                        batch.errors.push(SourceRuntimeError {
                            source_id: slot.configuration.source_id,
                            message,
                        });
                    }
                }
            }
            if slot.active.as_ref().is_some_and(FolderWatcher::is_finished) && slot.error.is_none()
            {
                let message =
                    "Source worker stopped unexpectedly; explicit rescan required".to_string();
                slot.state = SourceState::Failed;
                slot.error = Some(message.clone());
                batch.errors.push(SourceRuntimeError {
                    source_id: slot.configuration.source_id,
                    message,
                });
            }
        }
        if count > 0 {
            self.cursor = (self.cursor + 1) % count;
        }
        batch.statuses = self.statuses();
        batch
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn config(root: &std::path::Path, generation: u64) -> SourceConfiguration {
        SourceConfiguration {
            source_id: SourceId::Downloads,
            root: root.into(),
            generation,
            enabled: true,
        }
    }
    fn wait(runtime: &mut SourceRuntime) -> PreparedSourceFile {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let batch = runtime.poll(8);
            assert!(batch.errors.is_empty(), "{:?}", batch.errors);
            if let Some(prepared) = batch.prepared.into_iter().next() {
                return prepared;
            }
            assert!(Instant::now() < deadline, "source timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn complete_worker_snapshots_report_fixture_removal_without_dispatcher_io() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("present.bin");
        std::fs::write(&path, b"synthetic availability").unwrap();
        let canonical = std::fs::canonicalize(&path).unwrap();
        let mut runtime = SourceRuntime::new();
        runtime.configure(vec![config(temp.path(), 7)]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let batch = runtime.poll(8);
            if batch.snapshots.iter().any(|snapshot| {
                snapshot.configuration.generation == 7 && snapshot.paths.contains(&canonical)
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::remove_file(&path).unwrap();
        loop {
            let batch = runtime.poll(8);
            if batch
                .snapshots
                .iter()
                .any(|snapshot| snapshot.configuration.generation == 7 && snapshot.paths.is_empty())
            {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn bounded_batches_drain_more_than_one_hundred_twenty_eight_files() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..129 {
            std::fs::write(
                temp.path().join(format!("{index:03}.bin")),
                format!("fixture {index}"),
            )
            .unwrap();
        }
        let mut runtime = SourceRuntime::new();
        runtime.configure(vec![config(temp.path(), 1)]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut paths = std::collections::HashSet::new();
        while paths.len() < 129 {
            let batch = runtime.poll(255);
            assert!(batch.prepared.len() <= 8);
            assert!(batch.errors.is_empty(), "{:?}", batch.errors);
            for item in batch.prepared {
                assert!(paths.insert(item.candidate.path));
            }
            assert!(Instant::now() < deadline, "source backlog did not drain");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 129);
    }
    #[test]
    fn native_startup_notification_rescan_and_late_generation_use_only_current_root() {
        let old = tempfile::tempdir().unwrap();
        let next = tempfile::tempdir().unwrap();
        let old_file = old.path().join("old.bin");
        std::fs::write(&old_file, b"old fixture").unwrap();
        let mut runtime = SourceRuntime::new();
        runtime.configure(vec![config(old.path(), 1)]).unwrap();
        let first = wait(&mut runtime);
        assert_eq!(first.configuration.generation, 1);
        let created = old.path().join("created.bin");
        std::fs::write(&created, b"created fixture").unwrap();
        assert_eq!(
            wait(&mut runtime).candidate.path,
            std::fs::canonicalize(&created).unwrap()
        );
        // Leave old worker deliveries queued, then replace its configuration.
        std::fs::write(old.path().join("late.bin"), b"late fixture").unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let fresh = next.path().join("fresh.bin");
        std::fs::write(&fresh, b"fresh fixture").unwrap();
        runtime.configure(vec![config(next.path(), 2)]).unwrap();
        let result = wait(&mut runtime);
        assert_eq!(result.configuration.generation, 2);
        assert_eq!(
            result.candidate.path,
            std::fs::canonicalize(&fresh).unwrap()
        );
        runtime.rescan(SourceId::Downloads).unwrap();
        assert_eq!(wait(&mut runtime).configuration.generation, 2);
        assert_eq!(std::fs::read(old_file).unwrap(), b"old fixture");
    }
}

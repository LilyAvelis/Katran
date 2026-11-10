//! Read-only ShareX folder discovery, independent of UI and storage.
//!
//! A nonempty supported image is offered after two consecutive successful scans
//! observe the same size and modification time and can read it. Each revision is
//! offered once while continuously present and readable. Restarting the scanner
//! or removing and restoring a file offers it again; ingestion clients must apply
//! their own durable deduplication. This is a
//! polling adapter: intermediate revisions and same-size edits preserving mtime
//! cannot be detected. It does not decode images or guarantee a writer has closed
//! them. Symlinks are skipped. Traversal is capped at 20,000 directory entries;
//! exceeding this limit or failing to enumerate a directory returns an error and
//! leaves scanner state unchanged. Missing/unreadable individual files are skipped.
use crate::api::{ApiError, Result};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

const MAX_ENTRIES: usize = 20_000;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Revision {
    size: u64,
    modified: SystemTime,
}

#[derive(Clone, Copy)]
struct Observation {
    revision: Revision,
    offered: bool,
}

#[derive(Default)]
pub struct SharexScanner {
    observations: HashMap<PathBuf, Observation>,
}

impl SharexScanner {
    pub fn scan(&mut self, root: &Path) -> Result<Vec<PathBuf>> {
        let root = fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(ApiError::new(
                "invalid_directory",
                "ShareX root must be a directory",
            ));
        }
        let mut pending = vec![root];
        let mut entries = 0;
        let mut current = HashMap::new();
        let mut ready = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                entries += 1;
                if entries > MAX_ENTRIES {
                    return Err(ApiError::new(
                        "scan_limit",
                        "ShareX scan exceeds 20000 entries",
                    ));
                }
                let kind = match entry.file_type() {
                    Ok(kind) => kind,
                    Err(_) => continue,
                };
                let path = entry.path();
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() && supported(&path) {
                    let Some(revision) = readable_revision(&path) else {
                        continue;
                    };
                    let previous = self.observations.get(&path);
                    let unchanged = previous.is_some_and(|old| old.revision == revision);
                    let offered = unchanged && previous.is_some_and(|old| old.offered);
                    if unchanged && !offered {
                        ready.push(path.clone());
                    }
                    current.insert(
                        path,
                        Observation {
                            revision,
                            offered: unchanged,
                        },
                    );
                }
            }
        }
        self.observations = current;
        ready.sort();
        Ok(ready)
    }
}

fn supported(path: &Path) -> bool {
    crate::file_types::is_native_image(path)
}

fn readable_revision(path: &Path) -> Option<Revision> {
    let mut file = File::open(path).ok()?;
    let before = file.metadata().ok()?;
    if before.len() == 0 {
        return None;
    }
    let revision = Revision {
        size: before.len(),
        modified: before.modified().ok()?,
    };
    file.read_exact(&mut [0]).ok()?;
    let after = file.metadata().ok()?;
    (after.len() == revision.size && after.modified().ok()? == revision.modified)
        .then_some(revision)
}

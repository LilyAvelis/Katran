//! Session-only, live, non-recursive working-folder projection.
use crate::{
    ApiError, Event, EventKind, Result,
    origin::Origin,
    preview,
    sorting::{self, SortMode, SortSpec},
    source_policy::DeletePolicy,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct State {
    root: Option<PathBuf>,
    cache: RefCell<Cache>,
}
#[derive(Default)]
struct Cache {
    next: i64,
    by_path: HashMap<PathBuf, Event>,
    by_id: HashMap<i64, Event>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    fingerprint: String,
    offset: usize,
}

impl State {
    pub fn choose(&mut self, path: &Path) -> Result<()> {
        let root = path.canonicalize()?;
        if !root.is_dir() {
            return Err(ApiError::new("invalid_folder", "Expected a directory"));
        }
        self.root = Some(root);
        let cache = self.cache.get_mut();
        cache.by_path.clear();
        cache.by_id.clear();
        Ok(())
    }
    pub fn current(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// A full fresh directory enumeration precedes filtering and pagination.
    pub fn browse(
        &self,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
        sort: SortSpec,
    ) -> Result<Value> {
        if !(1..=1000).contains(&limit) {
            return Err(ApiError::new("invalid_argument", "Expected limit 1..1000"));
        }
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| ApiError::new("folder_not_selected", "Choose a working folder first"))?;
        if root.canonicalize().ok().as_deref() != Some(root.as_path()) || !root.is_dir() {
            return Err(ApiError::new(
                "stale_folder",
                "Working folder is no longer available",
            ));
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if !safe_regular(&metadata) {
                continue;
            }
            let canonical = match path.canonicalize() {
                Ok(p) => p,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if canonical.parent() != Some(root.as_path()) {
                continue;
            }
            let revision = preview::revision(&canonical, None);
            files.push((canonical, metadata, revision));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut cache = self.cache.borrow_mut();
        let present: HashSet<_> = files.iter().map(|v| v.0.clone()).collect();
        cache.by_path.retain(|path, _| present.contains(path));
        for (path, metadata, revision) in files {
            let needs_new = cache
                .by_path
                .get(&path)
                .is_none_or(|event| event.revision != revision);
            if needs_new {
                let id = cache
                    .next
                    .checked_sub(1)
                    .ok_or_else(|| ApiError::new("id_exhausted", "Working folder IDs exhausted"))?;
                cache.next = id;
                let modified = metadata.modified().ok().and_then(sorting::modified_ms);
                let event = Event {
                    kind: EventKind::FileReference,
                    text_preview: None,
                    delete_policy: DeletePolicy::ShelfOnly,
                    revision,
                    origin: Origin::default(),
                    id,
                    detected_at_ms: modified.unwrap_or(0),
                    modified_at_ms: modified,
                    source: "work_folder".into(),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    storage: "reference".into(),
                    path: path.clone(),
                    size: metadata.len(),
                    available: true,
                };
                cache.by_path.insert(path, event);
            }
        }
        cache.by_id = cache.by_path.values().map(|e| (e.id, e.clone())).collect();
        let mut all: Vec<Event> = cache.by_path.values().cloned().collect();
        all.sort_by(|a, b| a.path.cmp(&b.path));
        let sort = sort.normalized();
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}\0{}\0{:?}\0", root, query, sort));
        for e in &all {
            hasher.update(format!("{:?}\0{}\0{}\0", e.path, e.revision, e.id));
        }
        let fingerprint = format!("{:x}", hasher.finalize());
        let offset = if let Some(value) = cursor {
            if value.len() > 131072 {
                return Err(ApiError::new("invalid_cursor", "Cursor too large"));
            }
            let c: Cursor =
                serde_json::from_str(value).map_err(|e| ApiError::new("invalid_cursor", e))?;
            if c.version != 1 || c.fingerprint != fingerprint {
                return Err(ApiError::new(
                    "stale_cursor",
                    "Folder or browsing context changed",
                ));
            }
            c.offset
        } else {
            0
        };
        let needle = query.to_lowercase();
        all.retain(|event| event.name.to_lowercase().contains(&needle));
        all.sort_by(|a, b| {
            let order = match sort.mode {
                SortMode::Arrival | SortMode::Modified => a.modified_at_ms.cmp(&b.modified_at_ms),
                SortMode::Extension => {
                    sorting::extension_key(&a.name).cmp(&sorting::extension_key(&b.name))
                }
                SortMode::Shuffle => sorting::shuffle_key(a.id, sort.seed)
                    .cmp(&sorting::shuffle_key(b.id, sort.seed)),
                SortMode::Natural => {
                    sorting::natural_key(&a.name).cmp(&sorting::natural_key(&b.name))
                }
                SortMode::Size => a.size.cmp(&b.size),
                SortMode::Name | SortMode::Origin => {
                    sorting::name_key(&a.name).cmp(&sorting::name_key(&b.name))
                }
            }
            .then_with(|| sorting::name_key(&a.name).cmp(&sorting::name_key(&b.name)))
            .then_with(|| a.path.cmp(&b.path));
            if sort.reverse { order.reverse() } else { order }
        });
        if offset > all.len() {
            return Err(ApiError::new(
                "invalid_cursor",
                "Cursor offset out of range",
            ));
        }
        let end = offset.saturating_add(limit as usize).min(all.len());
        let next = if end < all.len() {
            Some(
                serde_json::to_string(&Cursor {
                    version: 1,
                    fingerprint,
                    offset: end,
                })
                .map_err(|e| ApiError::new("invalid_cursor", e))?,
            )
        } else {
            None
        };
        Ok(json!({"events": &all[offset..end], "next_cursor": next, "sort": sort}))
    }
    pub fn get(&self, id: i64) -> Result<Event> {
        let stale = || {
            ApiError::new(
                "stale_selection",
                "Folder file is no longer current; refresh the folder",
            )
        };
        let root = self.root.as_ref().ok_or_else(stale)?;
        if root.canonicalize().ok().as_deref() != Some(root.as_path()) || !root.is_dir() {
            return Err(stale());
        }
        let event = self
            .cache
            .borrow()
            .by_id
            .get(&id)
            .cloned()
            .ok_or_else(stale)?;
        let metadata = fs::symlink_metadata(&event.path).map_err(|_| stale())?;
        if !safe_regular(&metadata)
            || event.path.canonicalize().ok().as_deref() != Some(event.path.as_path())
            || event.path.parent() != Some(root.as_path())
            || preview::revision(&event.path, None) != event.revision
        {
            return Err(stale());
        }
        Ok(event)
    }
}
fn safe_regular(metadata: &fs::Metadata) -> bool {
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}

//! Explicit original-file deletion for ShareX references, through the dispatcher.
use crate::*;
use serde_json::{Value, json};
use std::collections::HashSet;

impl Service {
    pub(crate) fn delete_selection(&mut self, ids: Vec<i64>, confirmed: bool) -> Result<Value> {
        let mut seen = HashSet::new();
        let ids: Vec<_> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
        if ids.is_empty() || ids.len() > 1000 || ids.iter().any(|id| *id <= 0) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 positive record IDs",
            ));
        }
        let originals = ids.iter().any(|id| {
            self.get(*id)
                .is_ok_and(|e| matches!(e.source.as_str(), "sharex" | "downloads"))
        });
        if !originals {
            return self.dispatch(Command::DeleteEvents { ids, confirmed });
        }
        let permanent_files = ids
            .iter()
            .any(|id| self.get(*id).is_ok_and(|e| e.source == "sharex"));
        let recycle_bin = ids
            .iter()
            .any(|id| self.get(*id).is_ok_and(|e| e.source == "downloads"));
        if !confirmed && ids.len() > 1 {
            return Ok(
                json!({"confirmation_required":true,"ids":ids,"original_files":true,"permanent_files":permanent_files,"recycle_bin":recycle_bin,"deleted":0}),
            );
        }
        let mut items = Vec::new();
        let mut deleted = 0;
        for &id in &ids {
            let result = (|| -> Result<Value> {
                let event = self.get(id)?;
                let mut missing = false;
                if event.source == "downloads" {
                    let folder =
                        downloads::configured_folder(self.settings.downloads_folder.as_deref())?;
                    missing =
                        downloads::recycle_original(&event, &folder, &downloads::NativeTrash)?;
                }
                if event.source == "sharex" {
                    missing = self.remove_sharex_original(&event)?;
                }
                let shelf = self.dispatch(Command::DeleteEvents {
                    ids: vec![id],
                    confirmed: true,
                })?;
                Ok(json!({"ok":true,"id":id,"original_missing":missing,"shelf":shelf}))
            })();
            match result {
                Ok(item) => {
                    deleted += 1;
                    items.push(item);
                }
                Err(error) => items.push(json!({"ok":false,"id":id,"error":error})),
            }
        }
        Ok(
            json!({"ids":ids,"confirmation_required":false,"deleted":deleted,"items":items,"original_files":true,"permanent_files":permanent_files,"recycle_bin":recycle_bin}),
        )
    }

    fn remove_sharex_original(&self, event: &Event) -> Result<bool> {
        let folder = self
            .settings
            .sharex_folder
            .as_ref()
            .ok_or_else(|| ApiError::new("unsafe_source", "ShareX folder is not configured"))?
            .canonicalize()?;
        // Resolve the parent even for a missing file. A renamed or replaced folder
        // cannot turn an old reference into permission to delete another source.
        if event
            .path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(ApiError::new(
                "unsafe_source",
                "Source contains parent traversal",
            ));
        }
        let mut ancestor = event
            .path
            .parent()
            .ok_or_else(|| ApiError::new("unsafe_source", "Source has no parent"))?;
        // A removed nested directory is also a missing original. Resolve the
        // closest surviving ancestor; never skip a dangling link or I/O error.
        loop {
            match fs::symlink_metadata(ancestor) {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    ancestor = ancestor.parent().ok_or_else(|| {
                        ApiError::new("unsafe_source", "Source has no existing parent")
                    })?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        let parent = ancestor.canonicalize()?;
        if !parent.starts_with(&folder) {
            return Err(ApiError::new(
                "unsafe_source",
                "Source is outside the configured ShareX folder",
            ));
        }
        match fs::symlink_metadata(&event.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(e) => return Err(e.into()),
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(ApiError::new(
                    "unsafe_source",
                    "Source is not a regular file",
                ));
            }
            Ok(_) => {}
        }
        let canonical = event.path.canonicalize()?;
        if canonical == folder || !canonical.starts_with(&folder) {
            return Err(ApiError::new(
                "unsafe_source",
                "Source escapes the configured ShareX folder",
            ));
        }
        let expected = event.revision.strip_prefix("ref:").ok_or_else(|| {
            ApiError::new("stale_selection", "Source has no recorded content hash")
        })?;
        if preview::content_hash(&canonical)? != expected {
            return Err(ApiError::new(
                "stale_selection",
                "Source contents changed; refresh before deleting",
            ));
        }
        fs::remove_file(&canonical)?;
        Ok(false)
    }
}

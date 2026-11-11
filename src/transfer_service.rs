//! Synchronous application boundary for an explicit staged folder transfer.
use crate::{ApiError, Command, Result, SelectionRef, Service};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

impl Service {
    pub(crate) fn stage_transfer(&mut self, items: Vec<SelectionRef>, cut: bool) -> Result<Value> {
        // A rejected replacement must not leave an older, surprising cut armed.
        self.transfer_intent = None;
        self.transfer_sequence = None;
        if items.is_empty() || items.len() > 1000 {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 selections",
            ));
        }
        let mut seen = HashSet::new();
        for item in &items {
            if !seen.insert(item.id) {
                return Err(ApiError::new("invalid_argument", "Duplicate item ID"));
            }
            let event = self.get(item.id)?;
            if event.revision != item.revision {
                return Err(ApiError::new(
                    "stale_selection",
                    "Selection changed before staging",
                ));
            }
            crate::open_file::validate(&event)?;
        }
        let count = items.len();
        self.transfer_intent = Some((items, cut));
        if cut { self.transfer_sequence=Some(self.clipboard_files.sequence()); }
        Ok(json!({"staged":count,"cut":cut}))
    }

    pub(crate) fn paste_to_folder(&mut self) -> Result<Value> {
        if self.transfer_intent.is_none() || self.transfer_sequence.is_some_and(|sequence|sequence!=self.clipboard_files.sequence()) {
            return self.paste_clipboard_to_folder();
        }
        let destination = self
            .work_folder
            .current()
            .ok_or_else(|| ApiError::new("folder_not_selected", "Choose a working folder first"))?
            .to_owned();
        // Resolve the destination before consuming the intent; after effects begin,
        // always clear it so a retry cannot silently duplicate completed outputs.
        let root = destination.canonicalize()?;
        if !root.is_dir() {
            return Err(ApiError::new(
                "invalid_folder",
                "Working folder is unavailable",
            ));
        }
        let (items, cut) = self
            .transfer_intent
            .take()
            .ok_or_else(|| ApiError::new("no_transfer", "No staged selection"))?;
        let mut results: Vec<Option<Value>> = (0..items.len()).map(|_| None).collect();
        let mut ready = Vec::new();
        let mut indices = Vec::new();
        let mut source_locks = HashMap::new();
        for (index, item) in items.iter().enumerate() {
            let event = match self.get(item.id) {
                Ok(event) if event.revision == item.revision => event,
                Ok(_) => {
                    results[index] = Some(failed(
                        item.id,
                        ApiError::new("stale_selection", "Selection changed"),
                    ));
                    continue;
                }
                Err(e) => {
                    results[index] = Some(failed(item.id, e));
                    continue;
                }
            };
            if cut
                && event
                    .path
                    .canonicalize()
                    .ok()
                    .and_then(|path| path.parent().map(|p| p == root))
                    .unwrap_or(false)
            {
                results[index] = Some(
                    json!({"id":item.id,"path":event.path,"status":"same_folder","error":null}),
                );
                continue;
            }
            #[cfg(windows)]
            if cut
                && event.id > 0
                && matches!(
                    event.delete_policy,
                    crate::source_policy::DeletePolicy::LiveOriginal { .. }
                )
            {
                match crate::delete_shelf::protected_by_pin(&self.db, event.id) {
                    Ok(true) => {}
                    Ok(false) => match self.lock_transfer_source(&event) {
                        Ok(lock) => {
                            source_locks.insert(event.id, lock);
                        }
                        Err(e) => {
                            results[index] = Some(failed(event.id, e));
                            continue;
                        }
                    },
                    Err(e) => {
                        results[index] = Some(failed(event.id, e));
                        continue;
                    }
                }
            }
            ready.push(event);
            indices.push(index);
        }
        if !ready.is_empty() {
            match crate::folder_transfer::materialize(&ready, &root) {
                Ok(receipt) => {
                    for ((index, event), result) in
                        indices.into_iter().zip(ready.iter()).zip(receipt.items)
                    {
                        if result.status != "copied" || !cut {
                            results[index] = Some(
                                serde_json::to_value(result)
                                    .map_err(|e| ApiError::new("serialization_error", e))?,
                            );
                            continue;
                        }
                        results[index] = Some(self.finish_cut(
                            event,
                            result.path,
                            result.content_hash,
                            source_locks.remove(&event.id),
                        ));
                    }
                }
                Err(e) => {
                    for index in indices {
                        results[index] =
                            Some(failed(items[index].id, ApiError::new(&e.code, &e.message)));
                    }
                }
            }
        }
        Ok(
            json!({"cut":cut,"items":results.into_iter().map(|item| item.expect("each selection has a result")).collect::<Vec<_>>() }),
        )
    }

    pub(crate) fn import_from_folder(&mut self, ids: Vec<i64>) -> Result<Value> {
        if ids.is_empty() || ids.len() > 1000 || ids.iter().any(|id| *id >= 0) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 folder IDs",
            ));
        }
        let mut seen = HashSet::new();
        if ids.iter().any(|id| !seen.insert(*id)) {
            return Err(ApiError::new("invalid_argument", "Duplicate item ID"));
        }
        let mut results = Vec::new();
        for id in ids {
            let result = (|| {
                let event = self.get(id)?;
                crate::open_file::validate(&event)?;
                self.import_folder_event(event)
            })();
            match result {
                Ok(event) => results.push(json!({"id":id,"ok":event["failed"].as_u64().unwrap_or(0)==0,"event":event})),
                Err(e) => results.push(json!({"id":id,"ok":false,"error":e})),
            }
        }
        Ok(json!({"items":results}))
    }

    fn finish_cut(
        &mut self,
        event: &crate::Event,
        path: Option<std::path::PathBuf>,
        hash: Option<String>,
        source_lock: Option<crate::delete_selection::LockedOriginal>,
    ) -> Value {
        let result = self.finish_cut_inner(event, path.as_deref(), hash.as_deref(), source_lock);
        match result {
            Ok(status) => {
                json!({"id":event.id,"path":path,"status":status,"destination_created":true,"original_removed":status == "moved" && event.storage != "cached","error":null})
            }
            Err(e) => {
                let removed = e.code == "source_removed_shelf_retained";
                json!({"id":event.id,"path":path,"status":if removed { "source_removed_shelf_retained" } else { "source_not_moved" },"destination_created":true,"original_removed":removed,"error":e})
            }
        }
    }

    fn finish_cut_inner(
        &mut self,
        event: &crate::Event,
        path: Option<&std::path::Path>,
        hash: Option<&str>,
        source_lock: Option<crate::delete_selection::LockedOriginal>,
    ) -> Result<&'static str> {
        if event.id < 0 {
            return Err(ApiError::new(
                "source_not_movable",
                "Folder source retained; move is not available yet",
            ));
        }
        if crate::delete_shelf::protected_by_pin(&self.db, event.id)? {
            return Ok("pinned_copy");
        }
        let current = self.get(event.id)?;
        if current.revision != event.revision || current.delete_policy != event.delete_policy {
            return Err(ApiError::new(
                "stale_selection",
                "Selection changed before move",
            ));
        }
        #[cfg(not(windows))]
        {
            let _ = (path, hash, source_lock);
            return Err(ApiError::new(
                "unsupported_platform",
                "Safe move requires Windows file handles",
            ));
        }
        #[cfg(windows)]
        {
            use crate::source_policy::DeletePolicy;
            let root = self
                .work_folder
                .current()
                .ok_or_else(|| ApiError::new("folder_not_selected", "Working folder unavailable"))?
                .canonicalize()?;
            let output =
                path.ok_or_else(|| ApiError::new("verification_failed", "Missing output path"))?;
            let digest =
                hash.ok_or_else(|| ApiError::new("verification_failed", "Missing output hash"))?;
            // Keep the verified output handle locked against writes and replacement
            // through source disposition and shelf deletion.
            let _destination =
                crate::delete_selection::prepare_locked_original(output, &root, digest)?;
            if crate::delete_shelf::protected_by_pin(&self.db, event.id)? {
                return Ok("pinned_copy");
            }
            let current = self.get(event.id)?;
            if current.revision != event.revision || current.delete_policy != event.delete_policy {
                return Err(ApiError::new(
                    "stale_selection",
                    "Selection changed before move",
                ));
            }
            crate::open_file::validate(&current)?;
            let mut original_removed = false;
            if current.storage != "cached" {
                let DeletePolicy::LiveOriginal {
                    source_id,
                    generation,
                    ..
                } = current.delete_policy
                else {
                    return Err(ApiError::new(
                        "source_not_movable",
                        "External reference is not a managed original",
                    ));
                };
                let configured: String = self.db.query_row(
                    "SELECT c.root FROM source_configurations c JOIN source_projection p ON p.source_id=c.source_id AND p.generation=c.generation WHERE c.source_id=?1 AND c.generation=?2 AND c.enabled=1 AND p.event_id=?3",
                    rusqlite::params![source_id.as_str(), generation, current.id], |row| row.get(0)
                ).map_err(|_| ApiError::new("stale_source_generation", "Original no longer belongs to the configured source"))?;
                let _ = configured;
                let source = source_lock.ok_or_else(|| {
                    ApiError::new(
                        "source_not_locked",
                        "Original was not held throughout copying",
                    )
                })?;
                if crate::delete_shelf::protected_by_pin(&self.db, event.id)? {
                    return Ok("pinned_copy");
                }
                let now = self.get(event.id)?;
                if now.revision != event.revision || now.delete_policy != event.delete_policy {
                    return Err(ApiError::new(
                        "stale_selection",
                        "Selection changed before source removal",
                    ));
                }
                crate::delete_selection::dispose_locked_original(source)?;
                original_removed = true;
            }
            let deleted = self.dispatch(Command::DeleteEvents {
                ids: vec![event.id],
                confirmed: true,
            });
            match deleted {
                Ok(value) if value["deleted"].as_u64() == Some(1) => Ok("moved"),
                Ok(_) if original_removed => Err(ApiError::new(
                    "source_removed_shelf_retained",
                    "Source removed, but shelf record was retained",
                )),
                Ok(_) => Err(ApiError::new(
                    "shelf_retained",
                    "Copy created; shelf record retained",
                )),
                Err(e) if original_removed => Err(ApiError::new(
                    "source_removed_shelf_retained",
                    format!("Source removed; shelf cleanup failed: {}", e.message),
                )),
                Err(e) => Err(e),
            }
        }
    }

    #[cfg(windows)]
    fn lock_transfer_source(
        &self,
        event: &crate::Event,
    ) -> Result<crate::delete_selection::LockedOriginal> {
        let crate::source_policy::DeletePolicy::LiveOriginal {
            source_id,
            generation,
            ..
        } = event.delete_policy
        else {
            return Err(ApiError::new(
                "source_not_movable",
                "Not a managed original",
            ));
        };
        let configured: String = self.db.query_row(
            "SELECT c.root FROM source_configurations c JOIN source_projection p ON p.source_id=c.source_id AND p.generation=c.generation WHERE c.source_id=?1 AND c.generation=?2 AND c.enabled=1 AND p.event_id=?3",
            rusqlite::params![source_id.as_str(), generation, event.id], |row| row.get(0)
        ).map_err(|_| ApiError::new("stale_source_generation", "Original no longer belongs to the configured source"))?;
        let folder = std::path::Path::new(&configured).canonicalize()?;
        let expected = event
            .revision
            .strip_prefix("ref:")
            .ok_or_else(|| ApiError::new("stale_selection", "Original has no content identity"))?;
        crate::delete_selection::prepare_locked_original(&event.path, &folder, expected)
    }
}
fn failed(id: i64, error: ApiError) -> Value {
    json!({"id":id,"path":null,"status":"failed","error":error})
}

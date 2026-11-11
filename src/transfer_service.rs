//! Synchronous application boundary for an explicit staged folder transfer.
use crate::{ApiError, Command, Result, SelectionRef, Service};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

impl Service {
    pub(crate) fn stage_transfer(&mut self, items: Vec<SelectionRef>, cut: bool) -> Result<Value> {
        self.stage_transfer_inner(items, cut, true)
    }

    fn stage_transfer_inner(
        &mut self,
        items: Vec<SelectionRef>,
        cut: bool,
        bind_clipboard: bool,
    ) -> Result<Value> {
        // A rejected replacement must not leave an older, surprising cut armed.
        self.transfer_intent = None;
        self.transfer_sequence = None;
        if cut && items.iter().any(|item| item.id < 0) {
            return Err(ApiError::new(
                "folder_cut_unavailable",
                "Folder cut is unavailable; use ordinary file copy",
            ));
        }
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
        if cut && bind_clipboard {
            self.transfer_sequence = Some(self.clipboard_files.sequence());
        }
        Ok(json!({"staged":count,"cut":cut}))
    }

    pub(crate) fn transfer_selection(&mut self) -> Result<Value> {
        self.direct_selection(true)
    }

    pub(crate) fn copy_selection(&mut self) -> Result<Value> {
        self.direct_selection(false)
    }

    fn direct_selection(&mut self, moving: bool) -> Result<Value> {
        use crate::interaction::ShelfTab;
        let ids = if self.interaction.selected.is_empty() {
            self.interaction.cursor.into_iter().collect::<Vec<_>>()
        } else {
            self.interaction.selected.clone()
        };
        if ids.is_empty() || ids.len() > 1000 {
            return Err(ApiError::new(
                "invalid_argument",
                "Выберите от 1 до 1000 объектов",
            ));
        }
        if self.interaction.tab == ShelfTab::All {
            let result = self.import_from_folder_mode(ids, moving)?;
            let target = result["items"].as_array().and_then(|items| {
                items.iter().find_map(|item| {
                    if item["event"]["imported"].as_u64().unwrap_or(0) == 0 {
                        return None;
                    }
                    match item["event"]["kind"].as_str()? {
                        "image" => Some(ShelfTab::Images),
                        "text" => Some(ShelfTab::Text),
                        "table" => Some(ShelfTab::Tables),
                        "links" => Some(ShelfTab::Links),
                        _ => None,
                    }
                })
            });
            if moving && let Some(tab) = target {
                self.select_tab(tab)?;
            }
            return Ok(
                json!({"direction":"from_folder","items":result["items"],"interaction":self.interaction}),
            );
        }
        let folder = self
            .work_folder
            .current()
            .ok_or_else(|| ApiError::new("folder_not_selected", "Папка ещё не выбрана"))?;
        if !folder.is_dir() || folder.canonicalize().ok().as_deref() != Some(folder) {
            return Err(ApiError::new("folder_not_selected", "Папка ещё не выбрана"));
        }
        let mut valid = Vec::new();
        let mut errors = HashMap::new();
        for &id in &ids {
            match self.get(id).and_then(|event| {
                crate::open_file::validate(&event)?;
                Ok(SelectionRef {
                    id,
                    revision: event.revision,
                })
            }) {
                Ok(item) => valid.push(item),
                Err(error) => {
                    errors.insert(id, failed(id, error));
                }
            }
        }
        if valid.is_empty() {
            let items = ids
                .into_iter()
                .map(|id| errors.remove(&id).expect("failed selection"))
                .collect::<Vec<_>>();
            return Ok(
                json!({"direction":"to_folder","items":items,"interaction":self.interaction}),
            );
        }
        self.stage_transfer_inner(valid, moving, false)?;
        let mut receipt = self.paste_to_folder_inner(moving)?;
        let mut by_id: HashMap<_, _> = receipt["items"]
            .as_array_mut()
            .expect("transfer receipts")
            .drain(..)
            .filter_map(|item| item["id"].as_i64().map(|id| (id, item)))
            .collect();
        let items = ids
            .into_iter()
            .map(|id| {
                errors
                    .remove(&id)
                    .or_else(|| by_id.remove(&id))
                    .expect("selection receipt")
            })
            .collect::<Vec<_>>();
        if moving {
            self.select_tab(ShelfTab::All)?;
        }
        Ok(json!({"direction":"to_folder","items":items,"interaction":self.interaction}))
    }

    pub(crate) fn paste_to_folder(&mut self) -> Result<Value> {
        self.paste_to_folder_inner(false)
    }

    fn paste_to_folder_inner(&mut self, allow_reference_move: bool) -> Result<Value> {
        if self.transfer_intent.is_none()
            || self
                .transfer_sequence
                .is_some_and(|sequence| sequence != self.clipboard_files.sequence())
        {
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
                if allow_reference_move
                    && event.id > 0
                    && event.storage != "cached"
                    && matches!(
                        event.delete_policy,
                        crate::source_policy::DeletePolicy::ShelfOnly
                    )
                {
                    results[index] = Some(match self.finish_same_folder_reference(&event, &root) {
                        Ok(status) => {
                            json!({"id":item.id,"path":event.path,"status":status,"destination_created":false,"original_removed":false,"error":null})
                        }
                        Err(error) => failed(item.id, error),
                    });
                } else {
                    results[index] = Some(
                        json!({"id":item.id,"path":event.path,"status":"same_folder","error":null}),
                    );
                }
                continue;
            }
            #[cfg(windows)]
            if cut
                && event.id > 0
                && (matches!(
                    event.delete_policy,
                    crate::source_policy::DeletePolicy::LiveOriginal { .. }
                ) || (allow_reference_move
                    && event.storage != "cached"
                    && matches!(
                        event.delete_policy,
                        crate::source_policy::DeletePolicy::ShelfOnly
                    )))
            {
                match crate::delete_shelf::protected_by_pin(&self.db, event.id) {
                    Ok(true) => {}
                    Ok(false) => match if matches!(
                        event.delete_policy,
                        crate::source_policy::DeletePolicy::ShelfOnly
                    ) {
                        let parent = event
                            .path
                            .parent()
                            .ok_or_else(|| ApiError::new("unsafe_source", "Missing source parent"));
                        parent.and_then(|parent| {
                            event
                                .revision
                                .strip_prefix("ref:")
                                .ok_or_else(|| {
                                    ApiError::new("stale_selection", "Missing source hash")
                                })
                                .and_then(|hash| {
                                    crate::delete_selection::prepare_locked_original(
                                        &event.path,
                                        parent,
                                        hash,
                                    )
                                })
                        })
                    } else {
                        self.lock_transfer_source(&event)
                    } {
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
                            allow_reference_move,
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
        self.import_from_folder_mode(ids, false)
    }

    fn import_from_folder_mode(&mut self, ids: Vec<i64>, moving: bool) -> Result<Value> {
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
            let result: Result<Value> = (|| {
                let event = self.get(id)?;
                crate::open_file::validate(&event)?;
                let imported = self.import_folder_event(event.clone())?;
                if !moving
                    || imported["failed"].as_u64().unwrap_or(0) != 0
                    || imported["imported"].as_u64().unwrap_or(0) == 0
                {
                    return Ok(json!({"import": imported, "status": "copied"}));
                }
                let effect = (|| -> Result<Value> {
                    if self.folder_path_pinned(&event.path)? {
                        return Ok(json!({"status":"pinned_copy"}));
                    }
                    let hash = imported["source_hash"].as_str().ok_or_else(|| {
                        ApiError::new("verification_failed", "Missing import hash")
                    })?;
                    let root = self.work_folder.current().ok_or_else(|| {
                        ApiError::new("folder_not_selected", "Folder unavailable")
                    })?;
                    if event.path.starts_with(&self.root) {
                        return Err(ApiError::new(
                            "managed_source",
                            "Katran managed files cannot be moved from Folder",
                        ));
                    }
                    if event.path.parent() != Some(root) || self.get(id)?.revision != event.revision
                    {
                        return Err(ApiError::new(
                            "stale_selection",
                            "Folder item changed after import",
                        ));
                    }
                    let trash = self.trash.lock().map_err(|_| {
                        ApiError::new("recycle_failed", "Trash adapter lock poisoned")
                    })?;
                    let recycle = crate::safe_recycle::recycle_verified(
                        &event.path,
                        root,
                        hash,
                        trash.as_ref(),
                    )?;
                    Ok(
                        json!({"status": if recycle.recycled {"moved"} else {"recycle_failed"}, "recycle": recycle}),
                    )
                })();
                match effect {
                    Ok(value) => Ok(
                        json!({"import":imported,"status":value["status"],"recycle":value["recycle"]}),
                    ),
                    Err(error) => {
                        Ok(json!({"import":imported,"status":"source_not_moved","error":error}))
                    }
                }
            })();
            match result {
                Ok(value) => results.push(json!({"id":id,"ok":value["import"]["failed"].as_u64().unwrap_or(0)==0 && value["status"] != "recycle_failed" && value["status"] != "source_not_moved", "status":value["status"], "recycle":value["recycle"], "error":value["error"], "event":value["import"]})),
                Err(e) => results.push(json!({"id":id,"ok":false,"error":e})),
            }
        }
        Ok(json!({"items":results}))
    }

    fn finish_same_folder_reference(
        &mut self,
        event: &crate::Event,
        root: &std::path::Path,
    ) -> Result<&'static str> {
        crate::open_file::validate(event)?;
        if crate::delete_shelf::protected_by_pin(&self.db, event.id)? {
            return Ok("pinned_copy");
        }
        #[cfg(not(windows))]
        {
            let _ = root;
            Err(ApiError::new(
                "unsupported_platform",
                "Safe move requires Windows file handles",
            ))
        }
        #[cfg(windows)]
        {
            let hash = event
                .revision
                .strip_prefix("ref:")
                .ok_or_else(|| ApiError::new("stale_selection", "Missing source hash"))?;
            let _locked =
                crate::delete_selection::prepare_locked_original(&event.path, root, hash)?;
            let current = self.get(event.id)?;
            if current.revision != event.revision
                || current.path != event.path
                || current.delete_policy != event.delete_policy
            {
                return Err(ApiError::new("stale_selection", "Selection changed"));
            }
            if crate::delete_shelf::protected_by_pin(&self.db, event.id)? {
                return Ok("pinned_copy");
            }
            let result = self.dispatch(Command::DeleteEvents {
                ids: vec![event.id],
                confirmed: true,
            })?;
            if result["deleted"].as_u64() == Some(1) {
                Ok("moved")
            } else {
                Err(ApiError::new("shelf_retained", "Shelf record retained"))
            }
        }
    }

    fn finish_cut(
        &mut self,
        event: &crate::Event,
        path: Option<std::path::PathBuf>,
        hash: Option<String>,
        source_lock: Option<crate::delete_selection::LockedOriginal>,
        allow_reference_move: bool,
    ) -> Value {
        let result = self.finish_cut_inner(
            event,
            path.as_deref(),
            hash.as_deref(),
            source_lock,
            allow_reference_move,
        );
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
        allow_reference_move: bool,
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
            let _ = (path, hash, source_lock, allow_reference_move);
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
                match &current.delete_policy {
                    DeletePolicy::LiveOriginal {
                        source_id,
                        generation,
                        ..
                    } => {
                        let _: String = self.db.query_row(
                            "SELECT c.root FROM source_configurations c JOIN source_projection p ON p.source_id=c.source_id AND p.generation=c.generation WHERE c.source_id=?1 AND c.generation=?2 AND c.enabled=1 AND p.event_id=?3",
                            rusqlite::params![source_id.as_str(), generation, current.id], |row| row.get(0)
                        ).map_err(|_| ApiError::new("stale_source_generation", "Original no longer belongs to the configured source"))?;
                    }
                    DeletePolicy::ShelfOnly if allow_reference_move => {}
                    _ => {
                        return Err(ApiError::new(
                            "source_not_movable",
                            "External reference is not a managed original",
                        ));
                    }
                }
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

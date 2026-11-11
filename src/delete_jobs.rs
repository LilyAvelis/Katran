//! Private three-phase original deletion: prepare, authorize, effect and shelf receipt.
//! Windows permanent deletion targets the pinned verified handle. Shell recycling
//! accepts a pathname, so it cannot promise an atomic file-identity transaction;
//! the worker rechecks containment/content immediately before that native call.
use crate::{
    source_policy::{DeletePolicy, OriginalDeleteMode, SourceId},
    *,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
pub(crate) enum DeleteStart {
    Immediate(Value),
    Prepare(DeleteRequest),
}
struct Meta {
    ids: Vec<i64>,
    permanent: bool,
    recycle: bool,
}
pub(crate) struct DeleteRequest {
    meta: Meta,
    items: Vec<(i64, Result<Input>)>,
}
struct Input {
    event: Event,
    root: Option<PathBuf>,
}
pub(crate) struct PreparedDelete {
    meta: Meta,
    items: Vec<(i64, Result<PreparedItem>)>,
}
pub(crate) struct AuthorizedDelete {
    meta: Meta,
    items: Vec<(i64, Result<PreparedItem>)>,
}
struct PreparedItem {
    event: Event,
    original: Original,
}
enum Original {
    Shelf,
    Missing,
    Permanent(crate::delete_selection::LockedOriginal),
    Recycle {
        locked: crate::delete_selection::LockedOriginal,
        root: PathBuf,
    },
}
pub(crate) struct DeletedBatch {
    meta: Meta,
    items: Vec<(i64, Result<DeletedItem>)>,
}
struct DeletedItem {
    missing: bool,
    removed: bool,
}
impl Service {
    pub(crate) fn begin_delete_selection(
        &mut self,
        ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<DeleteStart> {
        let mut unique = std::collections::HashSet::new();
        let ids: Vec<_> = ids.into_iter().filter(|id| unique.insert(*id)).collect();
        if ids.is_empty() || ids.len() > 1000 || ids.iter().any(|id| *id <= 0) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 positive record IDs",
            ));
        }
        let events: Vec<_> = ids.iter().map(|id| (*id, self.get(*id))).collect();
        let permanent = events.iter().any(|(_, event)| {
            event.as_ref().is_ok_and(|event| {
                matches!(
                    event.delete_policy,
                    DeletePolicy::LiveOriginal {
                        mode: OriginalDeleteMode::Permanent,
                        ..
                    }
                )
            })
        });
        let recycle = events.iter().any(|(_, event)| {
            event.as_ref().is_ok_and(|event| {
                matches!(
                    event.delete_policy,
                    DeletePolicy::LiveOriginal {
                        mode: OriginalDeleteMode::Recycle,
                        ..
                    }
                )
            })
        });
        if !permanent && !recycle {
            return Ok(DeleteStart::Immediate(
                self.dispatch(Command::DeleteEvents { ids, confirmed })?,
            ));
        }
        if !confirmed && ids.len() > 1 {
            return Ok(DeleteStart::Immediate(
                json!({"confirmation_required":true,"ids":ids,"original_files":true,"permanent_files":permanent,"recycle_bin":recycle,"deleted":0}),
            ));
        }
        let items=events.into_iter().map(|(id,result)| {
            let input=result.and_then(|event| {
                if crate::delete_shelf::protected_by_pin(&self.db,id)? {
                    return Err(ApiError::new("skipped_pinned","Pinned record or pinned file alias is protected"));
                }
                let root=if let DeletePolicy::LiveOriginal{source_id,generation,..}=event.delete_policy {
                    let root:String=self.db.query_row("SELECT root FROM source_configurations WHERE source_id=?1 AND generation=?2 AND enabled=1",params![source_id.as_str(),generation],|row|row.get(0)).map_err(|_|ApiError::new("stale_source_generation","Original source configuration changed"))?;Some(PathBuf::from(root))
                } else {None};
                Ok(Input{event,root})
            });(id,input)
        }).collect();
        Ok(DeleteStart::Prepare(DeleteRequest {
            meta: Meta {
                ids,
                permanent,
                recycle,
            },
            items,
        }))
    }
    pub(crate) fn authorize_delete(&self, prepared: PreparedDelete) -> AuthorizedDelete {
        let items=prepared.items.into_iter().map(|(id,result)| {
            let result=result.and_then(|item| {
                let current=self.get(id)?;
                if crate::delete_shelf::protected_by_pin(&self.db,id)? {return Err(ApiError::new("skipped_pinned","Pinned record or pinned file alias is protected"));}
                if current.revision!=item.event.revision || current.delete_policy!=item.event.delete_policy {return Err(ApiError::new("stale_selection","Delete selection or ownership changed during preparation"));}
                if let DeletePolicy::LiveOriginal{source_id,generation,..}=current.delete_policy {
                    let current:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM source_projection p JOIN source_configurations c ON c.source_id=p.source_id AND c.generation=p.generation WHERE p.source_id=?1 AND p.generation=?2 AND p.event_id=?3 AND c.enabled=1)",params![source_id.as_str(),generation,id],|row|row.get(0))?;
                    if !current {return Err(ApiError::new("stale_source_generation","Original is no longer in the current source"));}
                }
                Ok(item)
            });(id,result)
        }).collect();
        AuthorizedDelete {
            meta: prepared.meta,
            items,
        }
    }
    pub(crate) fn finish_delete(&mut self, deleted: DeletedBatch) -> Result<Value> {
        let mut count = 0;
        let mut items = Vec::new();
        for (id, result) in deleted.items {
            match result {
                Ok(receipt) => {
                    let exists: bool = self.db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1)",
                        [id],
                        |row| row.get(0),
                    )?;
                    let shelf = if exists {
                        self.dispatch(Command::DeleteEvents {
                            ids: vec![id],
                            confirmed: true,
                        })
                    } else {
                        Ok(json!({"deleted":0}))
                    };
                    match shelf {
                        Ok(shelf) if exists && shelf["deleted"].as_u64() == Some(0) => items.push(json!({"ok":false,"id":id,"skipped_pinned":true,"original_removed":receipt.removed,"shelf":shelf})),
                        Ok(shelf)=> {count+=1;items.push(json!({"ok":true,"id":id,"skipped_pinned":false,"original_missing":receipt.missing,"original_removed":receipt.removed,"shelf_missing":!exists,"shelf":shelf}));}
                        Err(error)=>items.push(json!({"ok":false,"id":id,"original_removed":receipt.removed,"error":error})),
                    }
                }
                Err(error) => {
                    let skipped_pinned = error.code == "skipped_pinned";
                    items.push(
                        json!({"ok":false,"id":id,"skipped_pinned":skipped_pinned,"error":error}),
                    );
                }
            }
        }
        Ok(
            json!({"ids":deleted.meta.ids,"confirmation_required":false,"deleted":count,"items":items,"original_files":true,"permanent_files":deleted.meta.permanent,"recycle_bin":deleted.meta.recycle}),
        )
    }
}
fn prepare_input(input: Input) -> Result<PreparedItem> {
    let Some(root) = input.root else {
        return Ok(PreparedItem {
            event: input.event,
            original: Original::Shelf,
        });
    };
    let root = root.canonicalize()?;
    let event = input.event;
    if event
        .path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ApiError::new(
            "unsafe_source",
            "Original contains parent traversal",
        ));
    }
    let mut ancestor = event
        .path
        .parent()
        .ok_or_else(|| ApiError::new("unsafe_source", "Original has no parent"))?;
    loop {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ancestor = ancestor.parent().ok_or_else(|| {
                    ApiError::new("unsafe_source", "Original has no existing parent")
                })?
            }
            Err(error) => return Err(error.into()),
        }
    }
    if !ancestor.canonicalize()?.starts_with(&root) {
        return Err(ApiError::new(
            "unsafe_source",
            "Original is outside configured source",
        ));
    }
    match std::fs::symlink_metadata(&event.path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PreparedItem {
                event,
                original: Original::Missing,
            });
        }
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(ApiError::new(
                "unsafe_source",
                "Original is not a regular file",
            ));
        }
        Ok(_) => {}
    }
    let path = event.path.canonicalize()?;
    if path == root || !path.starts_with(&root) {
        return Err(ApiError::new(
            "unsafe_source",
            "Original escaped configured source",
        ));
    }
    let expected = event.revision.strip_prefix("ref:").ok_or_else(|| {
        ApiError::new(
            "stale_selection",
            "Original has no recorded content identity",
        )
    })?;
    let locked = crate::delete_selection::prepare_locked_original(&path, &root, expected)?;
    let original = match event.delete_policy {
        DeletePolicy::LiveOriginal {
            source_id: SourceId::Sharex,
            mode: OriginalDeleteMode::Permanent,
            ..
        } => Original::Permanent(locked),
        DeletePolicy::LiveOriginal {
            source_id: SourceId::Downloads,
            mode: OriginalDeleteMode::Recycle,
            ..
        } => Original::Recycle { locked, root },
        _ => {
            return Err(ApiError::new(
                "unsafe_source",
                "Unsupported original deletion policy",
            ));
        }
    };
    Ok(PreparedItem { event, original })
}
pub(crate) fn prepare(request: DeleteRequest) -> PreparedDelete {
    let items = request
        .items
        .into_iter()
        .map(|(id, input)| (id, input.and_then(prepare_input)))
        .collect();
    PreparedDelete {
        meta: request.meta,
        items,
    }
}
pub(crate) fn apply(
    authorized: AuthorizedDelete,
    trash: Arc<Mutex<Box<dyn downloads::Trash>>>,
) -> DeletedBatch {
    let items = authorized
        .items
        .into_iter()
        .map(|(id, result)| {
            let result = result.and_then(|item| {
                let (missing, removed) = match item.original {
                    Original::Shelf => (false, false),
                    Original::Missing => (true, false),
                    Original::Permanent(locked) => {
                        crate::delete_selection::dispose_locked_original(locked)?;
                        (false, true)
                    }
                    Original::Recycle { locked, root } => {
                        drop(locked);
                        let trash = trash.lock().map_err(|_| {
                            ApiError::new("recycle_failed", "Trash adapter lock poisoned")
                        })?;
                        let missing =
                            downloads::recycle_original(&item.event, &root, trash.as_ref())?;
                        (missing, !missing)
                    }
                };
                Ok(DeletedItem { missing, removed })
            });
            (id, result)
        })
        .collect();
    DeletedBatch {
        meta: authorized.meta,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reauthorization_rechecks_pin_before_effect() {
        let root = tempfile::tempdir().unwrap();
        let mut service = Service::open(root.path(), 1024 * 1024).unwrap();
        let event = service
            .dispatch(Command::IngestBytes {
                name: "synthetic.bin".into(),
                bytes: b"synthetic".to_vec(),
                source: "test".into(),
            })
            .unwrap();
        let id = event["id"].as_i64().unwrap();
        let input = Input {
            event: service.get(id).unwrap(),
            root: None,
        };
        let request = DeleteRequest {
            meta: Meta {
                ids: vec![id],
                permanent: false,
                recycle: false,
            },
            items: vec![(id, Ok(input))],
        };
        let prepared = prepare(request);
        crate::pinned::toggle_pins(&service.db, &[id]).unwrap();
        let authorized = service.authorize_delete(prepared);
        let deleted = apply(authorized, Arc::clone(&service.trash));
        let result = service.finish_delete(deleted).unwrap();
        assert_eq!(result["deleted"], 0);
        assert_eq!(result["items"][0]["skipped_pinned"], true);
        assert!(service.get(id).is_ok());
    }
}

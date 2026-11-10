//! Trusted domain preparation recipes. Worker values are private capabilities.
use crate::{
    domain_jobs::{JobId, Lane},
    origin::Origin,
    *,
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::SystemTime};

pub(crate) struct PreparedReference {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) size: u64,
    pub(crate) modified: SystemTime,
    pub(crate) content_hash: String,
    pub(crate) origin: Origin,
}
pub(crate) enum PreparedWork {
    Value(Value),
    Blob(storage::PreparedBlob),
    DeletePrepared(delete_jobs::PreparedDelete),
    DeleteDeleted(delete_jobs::DeletedBatch),
    Capture {
        ticket: u64,
        snapshot: Result<capture::CaptureSnapshot>,
    },
    LegacyCapture(Result<capture::CaptureSnapshot>),
    Reference(PreparedReference),
    Copy {
        events: Vec<Event>,
        plan: copy_out::Plan,
        sink: copy_out::PreparedNativeClipboard,
    },
    Drag {
        events: Vec<Event>,
        prepared: drag_session::DragPreparation,
    },
}
pub(crate) fn prepare_reference(
    path: PathBuf,
    source: String,
    origin: Origin,
) -> Result<PreparedReference> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let path = std::fs::canonicalize(path)?;
    let before = std::fs::metadata(&path)?;
    if !before.is_file() {
        return Err(ApiError::new("invalid_argument", "Expected a file"));
    }
    let mut file = std::fs::File::open(&path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let after = std::fs::metadata(&path)?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        return Err(ApiError::new(
            "source_changed",
            "File changed while preparing ingestion",
        ));
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ApiError::new("invalid_argument", "Invalid filename"))?
        .to_owned();
    Ok(PreparedReference {
        path,
        name,
        source,
        size: after.len(),
        modified: after.modified()?,
        content_hash: format!("{:x}", hash.finalize()),
        origin,
    })
}
impl Service {
    pub(crate) fn reference_paths(&self) -> Result<Vec<PathBuf>> {
        let mut statement = self
            .db
            .prepare("SELECT path FROM events WHERE blob_hash IS NULL")?;
        Ok(statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(PathBuf::from)
            .collect())
    }
    fn checked_job_events(&self, items: Vec<SelectionRef>) -> Result<Vec<Event>> {
        if items.is_empty() || items.len() > 1000 {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 selected objects",
            ));
        }
        let mut events = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for item in items {
            let event = self.get(item.id)?;
            if event.revision != item.revision {
                return Err(ApiError::new("stale_selection", "Selected content changed"));
            }
            if seen.insert(item.id) {
                events.push(event);
            }
        }
        Ok(events)
    }
    fn validate_job_events(&self, events: &[Event]) -> Result<()> {
        for event in events {
            let current = self.get(event.id).map_err(|_| {
                ApiError::new("stale_selection", "Selected object no longer exists")
            })?;
            if current.revision != event.revision {
                return Err(ApiError::new(
                    "stale_selection",
                    "Selected object changed during preparation",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn commit_prepared_reference(
        &mut self,
        prepared: PreparedReference,
    ) -> Result<Value> {
        validate_source(&prepared.source)?;
        validate_name(&prepared.name)?;
        if prepared.path.starts_with(&self.root) {
            return Err(ApiError::new(
                "managed_source",
                "Use cached ingestion for files inside Katran storage",
            ));
        }
        let metadata = std::fs::metadata(&prepared.path)?;
        if !metadata.is_file()
            || metadata.len() != prepared.size
            || metadata.modified()? != prepared.modified
        {
            return Err(ApiError::new(
                "source_changed",
                "Prepared reference changed before ingestion",
            ));
        }
        let path = prepared
            .path
            .to_str()
            .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode path"))?;
        self.db.execute("INSERT INTO events(detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash) VALUES(?1,?2,?3,?4,?5,NULL,?6,?7)",params![now_ms(),prepared.source,prepared.name,path,prepared.size,serde_json::to_string(&prepared.origin).map_err(|e|ApiError::new("invalid_argument",e))?,prepared.content_hash])?;
        let id = self.db.last_insert_rowid();
        classification_runtime::enqueue(&self.db, id)?;
        Ok(json!(self.get(id)?))
    }
    pub(crate) fn begin_job(&mut self, mut command: Command) -> Result<Value> {
        if let Command::RunAction { id } = &command {
            match actions::resolve(id, &self.interaction, self.capture_enabled) {
                Some(actions::Dispatch::Command(resolved)) => command = resolved,
                Some(actions::Dispatch::Intent(value)) => return self.ready_value_job(value),
                None => return Err(ApiError::new("unknown_action", "Unknown action")),
            }
        }
        let id = match command {
            Command::CaptureClipboard { trigger } => {
                self.jobs.check_admission(Lane::Capture)?;
                match self.begin_capture_event(trigger)? {
                    capture_events::CaptureStart::Immediate(value) => {
                        return self.ready_value_job_in(Lane::Capture, value);
                    }
                    capture_events::CaptureStart::Prepare {
                        ticket,
                        last_sequence,
                    } => {
                        let root = self.root.clone();
                        let quota = self.quota;
                        let references = self.reference_paths()?;
                        self.jobs.submit(Lane::Capture, move |cancel| {
                            cancel.check()?;
                            Ok(PreparedWork::Capture {
                                ticket,
                                snapshot: capture::prepare_snapshot_for_ingest(
                                    last_sequence,
                                    &root,
                                    quota,
                                    &references,
                                ),
                            })
                        })?
                    }
                }
            }
            Command::PollClipboard => {
                if !self.capture_enabled {
                    return self.ready_value_job(json!({"captured":0,"state":"paused"}));
                }
                let sequence = self.clipboard_sequence;
                let root = self.root.clone();
                let quota = self.quota;
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Capture, move |cancel| {
                    cancel.check()?;
                    Ok(PreparedWork::LegacyCapture(
                        capture::prepare_snapshot_for_ingest(sequence, &root, quota, &references),
                    ))
                })?
            }
            Command::EnsurePreviewBackend => self.jobs.submit(Lane::Discovery, |cancel| {
                cancel.begin_effect()?;
                Ok(PreparedWork::Value(quicklook_backend::warmup()?))
            })?,
            Command::PreviewExternal { id } => return self.begin_preview_job(id),
            Command::OpenCurrent | Command::TogglePreview => {
                if let Some(id) = self.interaction.cursor
                    && !quicklook::is_native_image(&self.get(id)?)
                {
                    return self.begin_preview_job(id);
                }
                let value = self.dispatch(command)?;
                return self.ready_value_job(value);
            }
            Command::DeleteSelection { ids, confirmed } => {
                match self.begin_delete_selection(ids, confirmed)? {
                    delete_jobs::DeleteStart::Immediate(value) => {
                        return self.ready_value_job(value);
                    }
                    delete_jobs::DeleteStart::Prepare(request) => {
                        self.jobs.submit(Lane::Foreground, move |cancel| {
                            cancel.check()?;
                            Ok(PreparedWork::DeletePrepared(delete_jobs::prepare(request)))
                        })?
                    }
                }
            }
            Command::IngestPath { path, source } => {
                validate_source(&source)?;
                let origin = self.ingest_origin.clone();
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    Ok(PreparedWork::Reference(prepare_reference(
                        path, source, origin,
                    )?))
                })?
            }
            Command::CopyEventsChecked { items } => {
                let events = self.checked_job_events(items)?;
                return self.begin_transfer_job(events, false);
            }
            Command::CopyEvents { ids } => {
                if ids.is_empty() || ids.len() > 1000 {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Expected 1..1000 selected objects",
                    ));
                }
                let mut seen = std::collections::HashSet::new();
                let events = ids
                    .into_iter()
                    .filter(|id| seen.insert(*id))
                    .map(|id| self.get(id))
                    .collect::<Result<Vec<_>>>()?;
                return self.begin_transfer_job(events, false);
            }
            Command::DragEventsChecked { items } => {
                let events = self.checked_job_events(items)?;
                self.pending_drag = None;
                return self.begin_transfer_job(events, true);
            }
            Command::Import { ids, destination } => {
                if ids.is_empty() || ids.len() > 1000 {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Expected 1..1000 event IDs",
                    ));
                }
                let events = ids
                    .into_iter()
                    .map(|id| self.get(id))
                    .collect::<Result<Vec<_>>>()?;
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    let destination = std::fs::canonicalize(destination)?;
                    if !destination.is_dir() {
                        return Err(ApiError::new(
                            "invalid_argument",
                            "Destination must be an existing directory",
                        ));
                    }
                    cancel.begin_effect()?;
                    let items: Vec<_> = events
                        .into_iter()
                        .map(|event| match export(&event, &destination) {
                            Ok(path) => json!({"id":event.id,"ok":true,"path":path}),
                            Err(error) => json!({"id":event.id,"ok":false,"error":error}),
                        })
                        .collect();
                    Ok(PreparedWork::Value(json!({"items":items,"atomic":false})))
                })?
            }
            Command::BeginJob { .. } | Command::PollJob { .. } | Command::CancelJob { .. } => {
                return Err(ApiError::new(
                    "invalid_argument",
                    "Jobs cannot wrap job-control commands",
                ));
            }
            Command::IngestBytes {
                name,
                bytes,
                source,
            } => {
                validate_name(&name)?;
                validate_source(&source)?;
                let root = self.root.clone();
                let quota = self.quota;
                let origin = self.ingest_origin.clone();
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    Ok(PreparedWork::Blob(storage::prepare_bytes(
                        &root,
                        quota,
                        storage::ByteIngest {
                            name: &name,
                            source: &source,
                            bytes: &bytes,
                            origin: &origin,
                        },
                        &references,
                    )?))
                })?
            }
            command => {
                let value = self.dispatch(command)?;
                return self.ready_value_job(value);
            }
        };
        Ok(json!({"job_id":id,"state":"queued"}))
    }
    fn ready_value_job(&mut self, value: Value) -> Result<Value> {
        Ok(json!({"state":"succeeded","result":value}))
    }
    fn ready_value_job_in(&mut self, _lane: Lane, value: Value) -> Result<Value> {
        self.ready_value_job(value)
    }
    fn begin_preview_job(&mut self, id: i64) -> Result<Value> {
        let event = self.get(id)?;
        let launcher = std::sync::Arc::clone(&self.external_preview);
        let id = self.jobs.submit(Lane::Foreground, move |cancel| {
            cancel.check()?;
            let launcher = launcher
                .lock()
                .map_err(|_| ApiError::new("quicklook_busy", "Preview launcher lock poisoned"))?;
            cancel.begin_effect()?;
            Ok(PreparedWork::Value(quicklook::preview(
                &event,
                launcher.as_ref(),
            )?))
        })?;
        Ok(json!({"job_id":id,"state":"queued"}))
    }
    fn begin_transfer_job(&mut self, events: Vec<Event>, drag: bool) -> Result<Value> {
        let root = self.root.clone();
        let quota = self.quota;
        let references = self.reference_paths()?;
        let id = self.jobs.submit(Lane::Foreground, move |cancel| {
            cancel.check()?;
            if drag {
                let prepared = drag_session::prepare_owned(&root, &events, quota, &references)?;
                return Ok(PreparedWork::Drag { events, prepared });
            }
            let mut plan = copy_out::prepare_with_budget(&root, &events, quota)
                .map_err(|e| ApiError::new("copy_error", e))?;
            plan.protect_references(&references)
                .map_err(|e| ApiError::new("copy_error", e))?;
            let sink = copy_out::PreparedNativeClipboard::prepare(&plan.paths);
            Ok(PreparedWork::Copy { events, plan, sink })
        })?;
        Ok(json!({"job_id":id,"state":"queued"}))
    }
    pub(crate) fn poll_job(&mut self, id: JobId) -> Result<Value> {
        for (ready, result) in self.jobs.take_ready(8) {
            let result = match result {
                Ok(PreparedWork::DeletePrepared(prepared)) => {
                    let authorized = self.authorize_delete(prepared);
                    let trash = std::sync::Arc::clone(&self.trash);
                    let continued = self.jobs.authorize_claimed_effect(ready).and_then(|()| {
                        self.jobs.continue_claimed(ready, move |_cancel| {
                            Ok(PreparedWork::DeleteDeleted(delete_jobs::apply(
                                authorized, trash,
                            )))
                        })
                    });
                    if let Err(error) = continued {
                        self.jobs.finish(ready, Err(error))?;
                    }
                    continue;
                }
                result => result,
            };
            let result=result.and_then(|prepared|match prepared {
                PreparedWork::Value(value)=>Ok(value),
                PreparedWork::DeleteDeleted(deleted)=>self.finish_delete(deleted),
                PreparedWork::DeletePrepared(_)=>Err(ApiError::new("invalid_job_phase","Delete preparation must be authorized before effect")),
                PreparedWork::Blob(prepared)=> { let id=storage::commit_prepared_bytes(&mut self.db,&self.root,self.quota,prepared)?; Ok(json!(self.get(id)?)) }
                PreparedWork::Capture{ticket,snapshot}=>self.complete_capture(ticket,snapshot),
                PreparedWork::LegacyCapture(snapshot)=>snapshot.and_then(|snapshot|self.commit_capture_snapshot(snapshot,capture::sequence_number())),
                PreparedWork::Reference(prepared)=>self.commit_prepared_reference(prepared),
                PreparedWork::Copy{events,plan,mut sink}=> {
                    self.validate_job_events(&events)?;
                    let receipt=copy_out::commit(&plan,&mut sink).map_err(|e|ApiError::new("clipboard_error",e))?;self.clipboard_sequence=receipt.sequence;
                    Ok(json!({"copied":receipt.count,"failures":receipt.failures,"warnings":receipt.warnings}))
                }
                PreparedWork::Drag{events,prepared}=> {self.validate_job_events(&events)?;self.commit_drag_preparation(prepared)}
            });
            self.jobs.finish(ready, result)?;
        }
        Ok(json!(self.jobs.status(id)?))
    }
}

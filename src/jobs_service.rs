//! Trusted domain preparation recipes. Worker values are private capabilities.
use crate::{
    domain_jobs::{JobId, Lane},
    origin::Origin,
    *,
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::SystemTime};

struct FencedFileLauncher {
    launcher: std::sync::Arc<std::sync::Mutex<Box<dyn open_file::Launcher>>>,
    cancellation: domain_jobs::Cancellation,
}
impl open_file::Launcher for FencedFileLauncher {
    fn authorize(&self) -> Result<()> {
        self.cancellation.begin_effect()
    }
    fn launch(&self, path: &std::path::Path, operation: open_file::Operation) -> Result<()> {
        self.launcher
            .lock()
            .map_err(|_| ApiError::new("open_busy", "File launcher lock poisoned"))?
            .launch(path, operation)
    }
}

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
    EmojiFont(emoji_fonts::PreparedFont),
    ThemeBackground {
        prepared: theme_backgrounds::PreparedBackground,
        event: Option<Event>,
        activate: bool,
    },
    UpdateInstalled(updater::InstalledUpdate),
    UpdateChecked(updater::UpdateStatus),
    UpdateReady {
        updater: updater::Updater,
        prepared: updater::PreparedUpdate,
    },
    TableCopy {
        prepared: table_service::PreparedCopy,
        events: Vec<Event>,
    },
    TextCopy {
        result: Value,
        events: Vec<Event>,
    },
    AppList {
        event: Event,
        apps: Vec<app_handlers::AppHandler>,
    },
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
        self.check_ingest_enabled(&prepared.name)?;
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
        self.db.execute(
            "UPDATE events SET modified_at_ms=?1 WHERE id=?2",
            params![sorting::modified_ms(prepared.modified), id],
        )?;
        classification_runtime::enqueue(&self.db, id)?;
        Ok(json!(self.get(id)?))
    }
    pub(crate) fn begin_job(&mut self, mut command: Command) -> Result<Value> {
        if let Command::RunAction { id } = &command {
            match actions::resolve(id, &self.interaction, self.capture_enabled) {
                Some(actions::Dispatch::Command(resolved)) => command = *resolved,
                Some(actions::Dispatch::Intent(value)) => return self.ready_value_job(value),
                None => return Err(ApiError::new("unknown_action", "Unknown action")),
            }
        }
        if let Command::DeleteSelection { ids, .. } = &command
            && ids.iter().any(|id| *id < 0)
        {
            let value = self.dispatch(command)?;
            return self.ready_value_job(value);
        }
        if matches!(command, Command::CopySelectedText) {
            command = Command::CopyText {
                items: self.selected_refs()?,
                separator: "\n\n".into(),
            };
        }
        if matches!(command, Command::CopySelectedObjects) {
            command = Command::CopySmart {
                items: self.selected_refs()?,
            };
        }
        if let Command::CopySmart { items } = command {
            command = if self.selection_is_tables(&items)? {
                Command::CopyTables { items }
            } else if self.selection_is_links(&items)? {
                Command::CopyLinks { items }
            } else if self.selection_is_text(&items)? {
                let separator = if self.selection_is_pods(&items)? {
                    ""
                } else {
                    "\n\n"
                }
                .into();
                Command::CopyText { items, separator }
            } else {
                Command::CopyEventsChecked { items }
            };
        }
        let id = match command {
            Command::ImportEmojiFont { path } => {
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let prepared = emoji_fonts::prepare(&path)?;
                    cancel.check()?;
                    Ok(PreparedWork::EmojiFont(prepared))
                })?
            }
            Command::ImportThemeBackground { path } => {
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let prepared = theme_backgrounds::prepare(&path)?;
                    cancel.check()?;
                    Ok(PreparedWork::ThemeBackground {
                        prepared,
                        event: None,
                        activate: false,
                    })
                })?
            }
            Command::UseSelectedImageAsBackground => {
                let event = self.selected_background_image()?;
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let prepared = theme_backgrounds::prepare_event(&event)?;
                    cancel.check()?;
                    Ok(PreparedWork::ThemeBackground {
                        prepared,
                        event: Some(event),
                        activate: true,
                    })
                })?
            }
            Command::SearchFolders { query, limit } => {
                self.jobs.submit(Lane::Discovery, move |cancel| {
                    cancel.check()?;
                    let value = everything::search_folders(&query, limit as usize)
                        .map_err(|e| ApiError::new(&e.code, e.message))?;
                    cancel.check()?;
                    Ok(PreparedWork::Value(json!(value)))
                })?
            }
            Command::CheckUpdates => {
                let updater = self.updater.clone();
                self.jobs.submit(Lane::Update, move |cancel| {
                    cancel.check()?;
                    let status = updater.check()?;
                    cancel.check()?;
                    Ok(PreparedWork::UpdateChecked(status))
                })?
            }
            Command::InstallUpdate => {
                let release = self.checked_update.clone().ok_or_else(|| {
                    ApiError::new("update_not_checked", "Check for an available update first")
                })?;
                let updater = self.updater.clone();
                self.jobs.submit(Lane::Update, move |cancel| {
                    cancel.check()?;
                    let prepared = updater.prepare(&release)?;
                    cancel.check()?;
                    Ok(PreparedWork::UpdateReady { updater, prepared })
                })?
            }
            Command::IngestTableTsv { text, source } => {
                self.check_tables_enabled()?;
                validate_source(&source)?;
                let root = self.root.clone();
                let quota = self.quota;
                let origin = self.ingest_origin.clone();
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Capture, move |cancel| {
                    cancel.check()?;
                    let table =
                        table_fragments::parse_tsv(&text, true).map_err(table_service::invalid)?;
                    cancel.check()?;
                    let prepared = table_service::prepare(
                        &root,
                        quota,
                        &table,
                        &source,
                        &origin,
                        &references,
                    )?;
                    cancel.check()?;
                    Ok(PreparedWork::Blob(prepared))
                })?
            }
            Command::IngestTableHtml { html, source } => {
                self.check_tables_enabled()?;
                validate_source(&source)?;
                let root = self.root.clone();
                let quota = self.quota;
                let origin = self.ingest_origin.clone();
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Capture, move |cancel| {
                    cancel.check()?;
                    let table =
                        table_fragments::parse_html(&html).map_err(table_service::invalid)?;
                    cancel.check()?;
                    let prepared = table_service::prepare(
                        &root,
                        quota,
                        &table,
                        &source,
                        &origin,
                        &references,
                    )?;
                    cancel.check()?;
                    Ok(PreparedWork::Blob(prepared))
                })?
            }
            Command::IngestTable { rows, source } => {
                self.check_tables_enabled()?;
                validate_source(&source)?;
                let root = self.root.clone();
                let quota = self.quota;
                let origin = self.ingest_origin.clone();
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Capture, move |cancel| {
                    cancel.check()?;
                    let prepared = table_service::prepare(
                        &root,
                        quota,
                        &table_fragments::Table { rows },
                        &source,
                        &origin,
                        &references,
                    )?;
                    cancel.check()?;
                    Ok(PreparedWork::Blob(prepared))
                })?
            }
            Command::CopyTables { items } => {
                let snapshot: Vec<_> = items
                    .into_iter()
                    .map(|item| {
                        let event = self.get(item.id);
                        (item, event)
                    })
                    .collect();
                let events = snapshot
                    .iter()
                    .filter_map(|(_, event)| event.as_ref().ok().cloned())
                    .collect();
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let prepared = table_service::prepare_copy(snapshot)?;
                    cancel.check()?;
                    Ok(PreparedWork::TableCopy { prepared, events })
                })?
            }
            Command::IngestLink { url, source } => {
                self.check_links_enabled()?;
                validate_source(&source)?;
                let root = self.root.clone();
                let quota = self.quota;
                let origin = self.ingest_origin.clone();
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Capture, move |cancel| {
                    cancel.check()?;
                    Ok(PreparedWork::Blob(link_fragments::prepare(
                        &root,
                        quota,
                        &url,
                        &source,
                        &origin,
                        &references,
                    )?))
                })?
            }
            Command::CopyLinks { items } => {
                let snapshot: Vec<_> = items
                    .into_iter()
                    .map(|item| {
                        let event = self.get(item.id);
                        (item, event)
                    })
                    .collect();
                let events = snapshot
                    .iter()
                    .filter_map(|(_, event)| event.as_ref().ok().cloned())
                    .collect();
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let mut result = text_fragments::merge_snapshot_kind(snapshot, "\n", true)?;
                    result["kind"] = json!("link_fragment");
                    cancel.check()?;
                    Ok(PreparedWork::TextCopy { result, events })
                })?
            }
            Command::IngestText { text, source } => {
                if crate::pods::is_pod(&text) {
                    self.check_pods_enabled()?;
                } else {
                    self.check_text_enabled()?;
                }
                validate_source(&source)?;
                let root = self.root.clone();
                let quota = self.quota;
                let origin = self.ingest_origin.clone();
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Capture, move |cancel| {
                    cancel.check()?;
                    Ok(PreparedWork::Blob(text_fragments::prepare(
                        &root,
                        quota,
                        &text,
                        &source,
                        &origin,
                        &references,
                    )?))
                })?
            }
            Command::CopyText { items, separator } => {
                let snapshot: Vec<_> = items
                    .into_iter()
                    .map(|item| {
                        let event = self.get(item.id);
                        (item, event)
                    })
                    .collect();
                let events = snapshot
                    .iter()
                    .filter_map(|(_, event)| event.as_ref().ok().cloned())
                    .collect();
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let result = text_fragments::merge_snapshot(snapshot, &separator)?;
                    cancel.check()?;
                    Ok(PreparedWork::TextCopy { result, events })
                })?
            }
            Command::ListOpenApps { id } => {
                let event = self.get(id)?;
                let provider = std::sync::Arc::clone(&self.app_provider);
                self.jobs.submit(Lane::Discovery, move |cancel| {
                    cancel.check()?;
                    let apps = {
                        let provider = provider.lock().map_err(|_| {
                            ApiError::new(
                                "handlers_unavailable",
                                "Application provider lock poisoned",
                            )
                        })?;
                        app_picker::discover(&event, provider.as_ref())?
                    };
                    cancel.check()?;
                    Ok(PreparedWork::AppList { event, apps })
                })?
            }
            Command::OpenWithApp { token, app_id } => {
                let session = self.app_session(&token, app_id.as_deref())?;
                let mut launcher = self.app_launcher(&session.event, app_id, None);
                let root = self.root.clone();
                let quota = self.quota;
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    launcher.cancellation = Some(cancel);
                    let mut result = crate::app_picker::execute_selected(
                        &session.event,
                        &launcher,
                        &root,
                        quota,
                    )?;
                    result["application_id"] = json!(launcher.app_id);
                    Ok(PreparedWork::Value(result))
                })?
            }
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
                        let images_enabled =
                            self.settings.tab_enabled(interaction::ShelfTab::Images);
                        let text_enabled = self.settings.tab_enabled(interaction::ShelfTab::Text);
                        let links_enabled = self.settings.tab_enabled(interaction::ShelfTab::Links);
                        let tables_enabled =
                            self.settings.tab_enabled(interaction::ShelfTab::Tables);
                        let pods_enabled = self.settings.tab_enabled(interaction::ShelfTab::Pods);
                        let references = self.reference_paths()?;
                        let owned_table_tokens: Vec<_> =
                            self.owned_table_tokens.iter().cloned().collect();
                        self.jobs.submit(Lane::Capture, move |cancel| {
                            cancel.check()?;
                            Ok(PreparedWork::Capture {
                                ticket,
                                snapshot: capture::prepare_snapshot_for_ingest(
                                    last_sequence,
                                    &root,
                                    quota,
                                    &references,
                                    (
                                        images_enabled,
                                        text_enabled,
                                        links_enabled,
                                        tables_enabled,
                                        pods_enabled,
                                    ),
                                    &owned_table_tokens,
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
                let images_enabled = self.settings.tab_enabled(interaction::ShelfTab::Images);
                let text_enabled = self.settings.tab_enabled(interaction::ShelfTab::Text);
                let links_enabled = self.settings.tab_enabled(interaction::ShelfTab::Links);
                let tables_enabled = self.settings.tab_enabled(interaction::ShelfTab::Tables);
                let pods_enabled = self.settings.tab_enabled(interaction::ShelfTab::Pods);
                let sequence = self.clipboard_sequence;
                let root = self.root.clone();
                let quota = self.quota;
                let references = self.reference_paths()?;
                let owned_table_tokens: Vec<_> = self.owned_table_tokens.iter().cloned().collect();
                self.jobs.submit(Lane::Capture, move |cancel| {
                    cancel.check()?;
                    Ok(PreparedWork::LegacyCapture(
                        capture::prepare_snapshot_for_ingest(
                            sequence,
                            &root,
                            quota,
                            &references,
                            (
                                images_enabled,
                                text_enabled,
                                links_enabled,
                                tables_enabled,
                                pods_enabled,
                            ),
                            &owned_table_tokens,
                        ),
                    ))
                })?
            }
            Command::EnsurePreviewBackend => self.jobs.submit(Lane::Discovery, |cancel| {
                cancel.begin_effect()?;
                Ok(PreparedWork::Value(quicklook_backend::warmup()?))
            })?,
            Command::OpenFile { id, operation } => {
                let event = self.get(id)?;
                let pinned_launcher = if id > 0 && operation == open_file::Operation::Open {
                    self.pinned_app_launcher(&event)?
                } else {
                    None
                };
                let launcher = std::sync::Arc::clone(&self.file_launcher);
                let root = self.root.clone();
                let quota = self.quota;
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    if let Some(mut pinned_launcher) = pinned_launcher {
                        pinned_launcher.cancellation = Some(cancel);
                        return Ok(PreparedWork::Value(pinned_open::execute_pinned(
                            &event,
                            &pinned_launcher,
                            &root,
                            quota,
                        )?));
                    }
                    let launcher = FencedFileLauncher {
                        launcher,
                        cancellation: cancel,
                    };
                    let result = if event.kind == EventKind::LinkFragment
                        && operation == open_file::Operation::Open
                    {
                        open_file::execute_link(&event, &launcher)?
                    } else {
                        open_file::execute_with_cache(&event, operation, &launcher, &root, quota)?
                    };
                    Ok(PreparedWork::Value(result))
                })?
            }
            Command::PreviewExternal { id } => return self.begin_preview_job(id),
            Command::OpenCurrent | Command::TogglePreview => {
                if let Some(id) = self.interaction.cursor
                    && !file_types::has_internal_preview(&self.get(id)?)
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
                self.check_ingest_enabled(&path.to_string_lossy())?;
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
                self.check_ingest_enabled(&name)?;
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
                Ok(PreparedWork::UpdateReady { updater, prepared }) => {
                    let continued = self.jobs.authorize_claimed_effect(ready).and_then(|()| {
                        self.jobs.continue_claimed(ready, move |_cancel| {
                            Ok(PreparedWork::UpdateInstalled(updater.commit(prepared)?))
                        })
                    });
                    if let Err(error) = continued {
                        self.jobs.finish(ready, Err(error))?;
                    }
                    continue;
                }
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
                PreparedWork::ThemeBackground{prepared,event,activate}=>{
                    if let Some(event)=event {self.validate_job_events(&[event])?;}
                    self.jobs.authorize_claimed_effect(ready)?;
                    let result=theme_backgrounds::commit(&self.root,prepared)?;
                    if activate {self.activate_background(result)}else{Ok(result)}
                },
                PreparedWork::EmojiFont(font)=> {self.jobs.authorize_claimed_effect(ready)?; emoji_fonts::commit(&self.root,font)},
                PreparedWork::Value(value)=>Ok(value),
                PreparedWork::UpdateChecked(status)=>self.accept_update_check(status),
                PreparedWork::UpdateReady{..}=>Err(ApiError::new("invalid_job_phase","Update preparation must be authorized before effect")),
                PreparedWork::UpdateInstalled(installed)=>self.accept_update_install(installed),
                PreparedWork::TableCopy{prepared,events}=>{if prepared.result["merged"]==true{self.validate_job_events(&events)?;self.jobs.authorize_claimed_effect(ready)?;}self.publish_table_copy(prepared)},
                PreparedWork::TextCopy{result,events}=>{if result["merged"]==true{self.validate_job_events(&events)?;self.jobs.authorize_claimed_effect(ready)?;}self.publish_text_result(result)},
                PreparedWork::AppList{event,apps}=>self.commit_open_apps(event,apps),
                PreparedWork::DeleteDeleted(deleted)=>self.finish_delete(deleted),
                PreparedWork::DeletePrepared(_)=>Err(ApiError::new("invalid_job_phase","Delete preparation must be authorized before effect")),
                PreparedWork::Blob(prepared)=> { if prepared.is_table(){self.check_tables_enabled()?;}else if prepared.is_link(){self.check_links_enabled()?;}else if prepared.is_pod(){self.check_pods_enabled()?;}else if prepared.is_text(){self.check_text_enabled()?;}else{self.check_ingest_enabled(prepared.name())?;} let id=storage::commit_prepared_bytes(&mut self.db,&self.root,self.quota,prepared)?; Ok(json!(self.get(id)?)) }
                PreparedWork::Capture{ticket,snapshot}=>self.complete_capture(ticket,snapshot),
                PreparedWork::LegacyCapture(snapshot)=>snapshot.and_then(|snapshot|self.commit_capture_snapshot(snapshot,capture::sequence_number())),
                PreparedWork::Reference(prepared)=>self.commit_prepared_reference(prepared),
                PreparedWork::Copy{events,plan,mut sink}=> {
                    self.validate_job_events(&events)?;
                    let receipt=copy_out::commit(&plan,&mut sink).map_err(|e|ApiError::new("clipboard_error",e))?;if receipt.failures.is_empty() {self.record_selection_publication(receipt.sequence,&events.iter().map(|event|event.id).collect::<Vec<_>>());}else{self.record_clipboard_publication(receipt.sequence);}
                    Ok(json!({"copied":receipt.count,"clipboard_sequence":receipt.sequence,"failures":receipt.failures,"warnings":receipt.warnings}))
                }
                PreparedWork::Drag{events,prepared}=> {self.validate_job_events(&events)?;self.commit_drag_preparation(prepared)}
            });
            self.jobs.finish(ready, result)?;
        }
        Ok(json!(self.jobs.status(id)?))
    }
}

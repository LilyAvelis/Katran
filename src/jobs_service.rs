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
pub(crate) struct FolderCopyBatch {
    pending: std::collections::VecDeque<(i64, Result<Event>)>,
    items: Vec<Value>,
    direct: bool,
    moving: bool,
    target: Option<interaction::ShelfTab>,
    folder: PathBuf,
    root: PathBuf,
    quota: u64,
    origin: Origin,
    references: Vec<PathBuf>,
}
fn prepare_folder_copy_step(
    mut batch: FolderCopyBatch,
    cancel: domain_jobs::Cancellation,
) -> Result<PreparedWork> {
    cancel.check()?;
    let (id, event) = batch
        .pending
        .pop_front()
        .expect("folder copy has pending item");
    let prepared = event.and_then(|event| {
        folder_import::prepare_folder_copy(
            event,
            &batch.root,
            batch.quota,
            &batch.origin,
            &batch.references,
            &cancel,
        )
    });
    cancel.check()?;
    Ok(PreparedWork::FolderCopy {
        batch,
        id,
        prepared,
    })
}
pub(crate) struct FolderExportBatch {
    folder: PathBuf,
    ids: Vec<i64>,
    events: Vec<Event>,
    errors: std::collections::HashMap<i64, Value>,
    moving: bool,
    paste: bool,
    locks: std::collections::HashMap<i64, crate::delete_selection::LockedOriginal>,
    lock_specs: std::collections::HashMap<i64, (PathBuf, PathBuf, String)>,
}
pub(crate) enum PreparedWork {
    PackImageCollection(image_collection::Packed),
    UnpackImageCollection {
        event: Event,
        stamp: media_service::Stamp,
        members: storage::CollectionMembers,
        pages: Vec<Option<storage::PreparedBlob>>,
        newest_first: bool,
    },
    ImageTranslationBatch(image_translation_batch::Prepared),
    ImageTranslation {
        event: Event,
        stamp: media_service::Stamp,
        blob: storage::PreparedBlob,
        target: settings::Language,
        source: ImageTranslationSource,
        dimensions: [u32; 2],
        report: image_translate::RenderReport,
    },
    ImageUpscale {
        event: Event,
        stamp: media_service::Stamp,
        blob: storage::PreparedBlob,
        dimensions: [u32; 4],
        operation: UpscaleOperation,
    },
    AudioExtraction {
        event: Event,
        stamp: media_service::Stamp,
        staged: audio_extract::StagedAudio,
        destination: PathBuf,
        audio_track: u32,
        clipboard_sequence: u32,
    },
    AudioReference { result: Value, clipboard_sequence: u32, prepared: Result<files_pointer::Pointer> },
    AudioCopy { result: Value, clipboard_sequence: u32, prepared: std::result::Result<(copy_out::Plan, copy_out::PreparedNativeClipboard), String> },
    MediaCapture {
        lease: media_service::CaptureLease,
        prepared: media_capture::PreparedCapture,
    },
    MediaCaptureCopy {
        result: Value,
        clipboard_sequence: u32,
        prepared: std::result::Result<(copy_out::Plan, copy_out::PreparedNativeClipboard), String>,
    },
    Media(crate::media_service::PreparedMedia),
    EmojiAi {
        value: Value,
        view_generation: u64,
    },
    ExternalPasteReady {
        folder: PathBuf,
        ids: Vec<i64>,
        events: Vec<Event>,
        errors: std::collections::HashMap<i64, Value>,
    },
    ExternalPasteFinished {
        ids: Vec<i64>,
        items: std::collections::HashMap<i64, Value>,
    },
    FolderExportReady(FolderExportBatch),
    FolderExportCopied {
        batch: FolderExportBatch,
        receipt: Result<folder_transfer::TransferReceipt>,
    },
    FolderCopy {
        batch: FolderCopyBatch,
        id: i64,
        prepared: Result<folder_import::PreparedFolderCopy>,
    },
    FolderBrowse {
        snapshot: work_folder::State,
        generation: u64,
        view_generation: u64,
        previous_query: String,
        query: String,
        sort: sorting::SortSpec,
        page: Value,
        select_all: bool,
        selection: Option<(Vec<i64>, Option<i64>)>,
    },
    VideoBackground {
        prepared: video_backgrounds::PreparedVideo,
        event: Option<Event>,
        activate: bool,
    },
    VideoWallpaper {
        event: Event,
        path: PathBuf,
        silent: bool,
    },
    EmojiFont(emoji_fonts::PreparedFont),
    ThemeBackground {
        prepared: theme_backgrounds::PreparedBackground,
        event: Option<Event>,
        activate: bool,
        desktop_wallpaper: bool,
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
    Files {
        prepared: files_service::PreparedFiles,
        origin: Origin,
    },
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
    fn commit_owned_audio_pointer(&mut self, pointer: files_pointer::Pointer, output: &std::path::Path) -> Result<Value> {
        let profile = self.root.canonicalize()?;
        let root = profile.join("audio-extractions").canonicalize()?;
        if root.parent() != Some(profile.as_path()) {
            return Err(ApiError::new("source_changed", "Audio storage path changed"));
        }
        let path = std::fs::canonicalize(output)?;
        let directory = path.parent().ok_or_else(|| ApiError::new("source_changed", "Invalid audio output"))?;
        if path != pointer.path || directory.parent() != Some(root.as_path())
            || !directory.file_name().is_some_and(|n| n.to_string_lossy().starts_with("audio-"))
            || path.file_name().is_none_or(|n| n != "audio.m4a") {
            return Err(ApiError::new("source_changed", "Audio output path changed"));
        }
        let meta = std::fs::symlink_metadata(&path)?;
        if !meta.file_type().is_file() || meta.len() != pointer.size || meta.modified()? != pointer.modified {
            return Err(ApiError::new("source_changed", "Audio output changed before registration"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(ApiError::new("source_changed", "Audio output became a reparse point"));
            }
        }
        let path = path.to_str().ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode audio path"))?;
        let origin = serde_json::to_string(&Origin::default()).map_err(|e| ApiError::new("invalid_argument", e))?;
        let id = files_store::insert_or_reuse(&mut self.db, &files_store::PreparedFile {
            path, path_key: path, name: "audio.m4a", size: pointer.size,
            reference_hash: &pointer.content_hash, modified_at_ms: sorting::modified_ms(pointer.modified),
            detected_at_ms: now_ms(), origin_json: &origin,
        })?;
        Ok(json!(self.get(id)?))
    }
    fn begin_folder_semantic(
        &mut self,
        query: String,
        offset: u32,
        limit: u32,
        snapshot: Option<String>,
    ) -> Result<crate::domain_jobs::JobId> {
        crate::folder_semantic::validate(&query, limit)?;
        if offset >= 8192 || (offset > 0 && snapshot.is_none()) {
            return Err(ApiError::new(
                "invalid_argument",
                "A valid snapshot is required for subsequent pages",
            ));
        }
        let saved: Vec<_> = crate::pinned::folder_favorites(&self.db)?
            .into_iter()
            .chain(crate::pinned::folder_history(&self.db)?)
            .map(|entry| entry.path)
            .collect();
        let backend = std::sync::Arc::clone(&self.folder_semantic);
        let source = std::sync::Arc::clone(&self.folder_semantic_source);
        let gpu = self.settings.folder_ai_gpu;
        let session = self.folder_semantic_session.clone();
        self.jobs.submit(Lane::Discovery, move |cancel| {
            cancel.check()?;
            let mut backend = backend.lock().map_err(|_| {
                ApiError::new(
                    "folder_ai_unavailable",
                    "Local folder AI backend is unavailable",
                )
            })?;
            let mut source = source.lock().map_err(|_| {
                ApiError::new(
                    "folder_ai_unavailable",
                    "Folder search index is unavailable",
                )
            })?;
            let request = crate::folder_semantic::PageRequest {
                query: &query,
                offset,
                limit,
                expected_snapshot: snapshot.as_deref(),
                gpu,
                session: &session,
            };
            let result = crate::folder_semantic::search(
                &mut **backend,
                &mut **source,
                request,
                &saved,
                &cancel,
            )?;
            cancel.check()?;
            Ok(PreparedWork::Value(result))
        })
    }

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
    pub(crate) fn validate_job_events(&self, events: &[Event]) -> Result<()> {
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
        if matches!(&command, Command::DeleteSelection { ids, .. } | Command::DeleteEvents { ids, .. } if ids.iter().any(|id| pods::is_emoji_id(*id)))
        {
            return Err(ApiError::new(
                "read_only",
                "Emoji catalogue entries are read-only",
            ));
        }
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
        if matches!(command, Command::CopySelectionAction) {
            if self.interaction.tab == interaction::ShelfTab::Pods
                && self.interaction.emoji_catalogue
            {
                let value = self.dispatch(command)?;
                return self.ready_value_job(value);
            }
            command = Command::CopySelectedObjects;
        }
        if matches!(command, Command::PasteToFolder) {
            let folder = self
                .work_folder
                .current()
                .ok_or_else(|| {
                    ApiError::new("folder_not_selected", "Choose a working folder first")
                })?
                .to_owned();
            let root = folder.canonicalize()?;
            if !root.is_dir() {
                return Err(ApiError::new(
                    "invalid_folder",
                    "Working folder is unavailable",
                ));
            }
            let staged_current = self.transfer_intent.is_some()
                && self
                    .transfer_sequence
                    .is_none_or(|sequence| sequence == self.clipboard_files.sequence());
            if staged_current {
                let (items, moving) = self.transfer_intent.as_ref().expect("checked intent");
                let mut ids = Vec::new();
                let mut events = Vec::new();
                let mut errors = std::collections::HashMap::new();
                for item in items {
                    ids.push(item.id);
                    match self.get(item.id) {
                        Ok(event) if event.revision == item.revision => events.push(event),
                        Ok(_) => {
                            errors.insert(item.id, json!({"id":item.id,"path":null,"status":"failed","error":{"code":"stale_selection","message":"Selection changed"}}));
                        }
                        Err(error) => {
                            errors.insert(
                                item.id,
                                json!({"id":item.id,"path":null,"status":"failed","error":error}),
                            );
                        }
                    }
                }
                let batch = FolderExportBatch {
                    folder: root,
                    ids,
                    events,
                    errors,
                    moving: *moving,
                    paste: true,
                    locks: std::collections::HashMap::new(),
                    lock_specs: std::collections::HashMap::new(),
                };
                let id = self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    Ok(PreparedWork::FolderExportReady(batch))
                })?;
                self.transfer_intent = None;
                self.transfer_sequence = None;
                return Ok(json!({"job_id":id,"state":"queued"}));
            }
            self.transfer_intent = None;
            self.transfer_sequence = None;
            let paths = self.clipboard_files.files()?;
            if paths.is_empty() || paths.len() > 1000 {
                return Err(ApiError::new("invalid_argument", "Expected 1..1000 files"));
            }
            let id = self.jobs.submit(Lane::Foreground, move |cancel| {
                let mut events = Vec::new();
                let mut errors = std::collections::HashMap::new();
                let ids: Vec<i64> = (0..paths.len()).map(|index| -(index as i64) - 1).collect();
                for (id, path) in ids.iter().copied().zip(paths) {
                    cancel.check()?;
                    match prepare_reference(path, "external_paste".into(), Origin::default()) {
                        Ok(input) => events.push(Event {
                            kind: EventKind::FileReference,
                            text_preview: None,
                            delete_policy: source_policy::DeletePolicy::ShelfOnly,
                            revision: format!("ref:{}", input.content_hash),
                            origin: input.origin,
                            id,
                            detected_at_ms: now_ms(),
                            modified_at_ms: sorting::modified_ms(input.modified),
                            source: input.source,
                            name: input.name,
                            storage: "reference".into(),
                            path: input.path,
                            size: input.size,
                            available: true,
                        }),
                        Err(error) => {
                            errors.insert(
                                id,
                                json!({"id":id,"path":null,"status":"failed","error":error}),
                            );
                        }
                    }
                }
                Ok(PreparedWork::ExternalPasteReady {
                    folder: root,
                    ids,
                    events,
                    errors,
                })
            })?;
            self.transfer_intent = None;
            self.transfer_sequence = None;
            return Ok(json!({"job_id":id,"state":"queued"}));
        }
        let folder_copy = match &command {
            Command::CopySelection if self.interaction.tab == interaction::ShelfTab::All => {
                let ids = if self.interaction.selected.is_empty() {
                    self.interaction.cursor.into_iter().collect::<Vec<_>>()
                } else {
                    self.interaction.selected.clone()
                };
                Some((ids, true, false))
            }
            Command::TransferSelection if self.interaction.tab == interaction::ShelfTab::All => {
                let ids = if self.interaction.selected.is_empty() {
                    self.interaction.cursor.into_iter().collect::<Vec<_>>()
                } else {
                    self.interaction.selected.clone()
                };
                Some((ids, true, true))
            }
            Command::AddFolderFiles { ids } => Some((ids.clone(), false, false)),
            _ => None,
        };
        if let Some((ids, direct, moving)) = folder_copy {
            if ids.is_empty() || ids.len() > 1000 || ids.iter().any(|id| *id >= 0) {
                return Err(ApiError::new(
                    "invalid_argument",
                    "Expected 1..1000 folder IDs",
                ));
            }
            let mut seen = std::collections::HashSet::new();
            if ids.iter().any(|id| !seen.insert(*id)) {
                return Err(ApiError::new("invalid_argument", "Duplicate item ID"));
            }
            let folder = self
                .work_folder
                .current()
                .ok_or_else(|| ApiError::new("folder_not_selected", "Folder unavailable"))?
                .to_owned();
            let batch = FolderCopyBatch {
                pending: ids.into_iter().map(|id| (id, self.get(id))).collect(),
                items: Vec::new(),
                direct,
                moving,
                target: None,
                folder,
                root: self.root.clone(),
                quota: self.quota,
                origin: self.ingest_origin.clone(),
                references: self.reference_paths()?,
            };
            let id = self.jobs.submit(Lane::Foreground, move |cancel| {
                prepare_folder_copy_step(batch, cancel)
            })?;
            return Ok(json!({"job_id":id,"state":"queued"}));
        }
        if matches!(command, Command::CopySelection | Command::TransferSelection)
            && self.interaction.tab != interaction::ShelfTab::All
        {
            let ids = if self.interaction.selected.is_empty() {
                self.interaction.cursor.into_iter().collect::<Vec<_>>()
            } else {
                self.interaction.selected.clone()
            };
            if ids.is_empty() || ids.len() > 1000 {
                return Err(ApiError::new("invalid_argument", "Select 1..1000 objects"));
            }
            let folder = self
                .work_folder
                .current()
                .ok_or_else(|| ApiError::new("folder_not_selected", "Folder unavailable"))?
                .to_owned();
            if !folder.is_dir() || folder.canonicalize().ok().as_deref() != Some(folder.as_path()) {
                return Err(ApiError::new("folder_not_selected", "Folder unavailable"));
            }
            let mut events = Vec::new();
            let mut errors = std::collections::HashMap::new();
            for &id in &ids {
                match self.get(id) {
                    Ok(event) => events.push(event),
                    Err(error) => {
                        errors.insert(
                            id,
                            json!({"id":id,"path":null,"status":"failed","error":error}),
                        );
                    }
                }
            }
            let moving = matches!(command, Command::TransferSelection);
            let batch = FolderExportBatch {
                folder,
                ids,
                events,
                errors,
                moving,
                paste: false,
                locks: std::collections::HashMap::new(),
                lock_specs: std::collections::HashMap::new(),
            };
            let id = self.jobs.submit(Lane::Foreground, move |cancel| {
                cancel.check()?;
                Ok(PreparedWork::FolderExportReady(batch))
            })?;
            self.transfer_intent = None;
            self.transfer_sequence = None;
            return Ok(json!({"job_id":id,"state":"queued"}));
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
            command @ (Command::TranslateSelectedImage | Command::TranslateSelectedImageFrom { .. }) => {
                let source_language = match command {
                    Command::TranslateSelectedImageFrom { source } => source,
                    _ => ImageTranslationSource::Japanese,
                };
                let numbered_only = self.settings.translation_numbered_only;
                let selection = self.selected_refs()?;
                if selection.len() > 1 {
                    if selection.len() > image_translation_batch::MAX_PAGES {
                        return Err(ApiError::new("invalid_selection", "Select up to 32 pages"));
                    }
                    let events = selection.iter().map(|item| self.get(item.id)).collect::<Result<Vec<_>>>()?;
                    let root = self.root.clone();
                    let quota = self.quota;
                    let references = self.reference_paths()?;
                    let session = self.image_translation.clone();
                    let id = self.jobs.submit(Lane::ImageTranslate, move |cancel| {
                        let mut session = session.lock()
                            .map_err(|_| ApiError::new("image_translate_error", "Translation session failed"))?;
                        Ok(PreparedWork::ImageTranslationBatch(image_translation_batch::prepare(
                            events, &root, quota, &references, source_language, numbered_only, &mut session, &cancel,
                        )?))
                    })?;
                    return Ok(json!({"job_id":id,"state":"queued","total":selection.len()}));
                }
                let event = self.selected_background_image()?;
                let root = self.root.clone();
                let quota = self.quota;
                let references = self.reference_paths()?;
                let target = settings::Language::English;
                let session = self.image_translation.clone();
                self.jobs.submit(Lane::ImageTranslate, move |cancel| {
                    use std::io::Read;
                    const MAX_SOURCE: u64 = 32 * 1024 * 1024;
                    cancel.check()?;
                    let components = translate_components::discover(&root, translate_components::MODEL)?
                        .resolve(source_language)?;
                    let stamp = media_service::stamp(&event.path)?;
                    let path = wallpaper_engine::validate(&root, &event)?;
                    let mut source = Vec::new();
                    std::fs::File::open(&path)?.take(MAX_SOURCE + 1).read_to_end(&mut source)?;
                    if source.len() as u64 > MAX_SOURCE {
                        return Err(ApiError::new("invalid_image", "Image exceeds 32 MiB"));
                    }
                    if media_service::stamp(&path)? != stamp {
                        return Err(ApiError::new("source_changed", "Image changed during preparation"));
                    }
                    let staging = root.join("translate-work");
                    std::fs::create_dir_all(&staging)?;
                    let language = match target {
                        settings::Language::English => "en",
                        settings::Language::Russian => "ru",
                    };
                    let rendered = session
                        .lock()
                        .map_err(|_| ApiError::new("image_translate_error", "Translation session failed"))?
                        .translate(&source, &staging, components, source_language, language, numbered_only, &cancel)?;
                    let bytes = std::fs::read(rendered.path())?;
                    let name = format!("translation-{language}.png");
                    let blob = storage::prepare_bytes(
                        &root, quota,
                        storage::ByteIngest { name: &name, source: "image_translation", bytes: &bytes, origin: &Origin::default() },
                        &references,
                    )?;
                    cancel.check()?;
                    Ok(PreparedWork::ImageTranslation { event, stamp, blob, target, source: source_language,
                        dimensions: [rendered.width, rendered.height],
                        report: rendered.report,
                    })
                })?
            }
            Command::CreateImageCollection => {
                let translated = match self.interaction.tab {
                    interaction::ShelfTab::Images => false,
                    interaction::ShelfTab::Translate => true,
                    _ => return Err(ApiError::new("invalid_selection", "Use the BULL command in Images or Translate")),
                };
                let ids = if self.interaction.selected.is_empty() {
                    self.interaction.cursor.as_slice()
                } else {
                    self.interaction.selected.as_slice()
                };
                if !(1..=baloh_collection::codec::MAX_PAGES).contains(&ids.len()) {
                    return Err(ApiError::new("invalid_selection", "Select images or BULL collections"));
                }
                let events = ids.iter().map(|&id| self.get(id)).collect::<Result<Vec<_>>>()?;
                for event in &events {
                    if !storage::visible_collection_input(&self.db, event.id, translated)? {
                        return Err(ApiError::new("invalid_selection", "Select visible cards from the same shelf"));
                    }
                }
                let root = self.root.clone();
                let quota = self.quota;
                let references = self.reference_paths()?;
                if let [event] = events.as_slice() && image_collection::is_managed(event) {
                    let event = event.clone();
                    let members = storage::collection_members(&self.db, event.id)?;
                    let newest_first = !self.interaction.sort.reverse;
                    self.jobs.submit(Lane::Foreground, move |cancel| {
                        let stamp = media_service::stamp(&event.path)?;
                        let pages = image_collection::unpack(&event, &members, &root, quota, &references, &cancel)?;
                        Ok(PreparedWork::UnpackImageCollection { event, stamp, members, pages, newest_first })
                    })?
                } else {
                    if !events.iter().all(image_collection::is_image)
                        && !events.iter().all(image_collection::is_managed)
                    {
                        return Err(ApiError::new("invalid_selection", "Select only images or only BULL collections"));
                    }
                    self.jobs.submit(Lane::Foreground, move |cancel| {
                        Ok(PreparedWork::PackImageCollection(image_collection::pack(events, translated, &root, quota, &references, &cancel)?))
                    })?
                }
            }
            Command::SearchSelectedImageWithGoogleLens => {
                let event = self.selected_background_image()?;
                let launcher = self.file_launcher.clone();
                let language = match self.settings.language {
                    settings::Language::English => "en",
                    settings::Language::Russian => "ru",
                };
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    google_lens::search(&event, language, |url| {
                        launcher.lock()
                            .map_err(|_| ApiError::new("open_busy", "Could not use the default browser"))?
                            .launch(std::path::Path::new(url), open_file::Operation::Open)
                    }, &cancel)?;
                    Ok(PreparedWork::Value(json!({"google_lens_requested":true})))
                })?
            }
            Command::UpscaleSelectedImage { operation } => {
                let event = self.selected_background_image()?;
                let root = self.root.clone();
                let quota = self.quota;
                let references = self.reference_paths()?;
                self.jobs.submit(Lane::Upscale, move |cancel| {
                    use std::io::Read;
                    const MAX_SOURCE: u64 = 32 * 1024 * 1024;
                    cancel.check()?;
                    let runtime = upscale_components::resolve(&root, operation.weights())?;
                    let stamp = media_service::stamp(&event.path)?;
                    let path = wallpaper_engine::validate(&root, &event)?;
                    let mut source = Vec::new();
                    std::fs::File::open(&path)?.take(MAX_SOURCE + 1).read_to_end(&mut source)?;
                    if source.len() as u64 > MAX_SOURCE {
                        return Err(ApiError::new("invalid_image", "Image exceeds 32 MiB"));
                    }
                    if media_service::stamp(&path)? != stamp {
                        return Err(ApiError::new("source_changed", "Image changed during preparation"));
                    }
                    let staging = root.join("upscale-work");
                    std::fs::create_dir_all(&staging)?;
                    let staged = image_upscale::upscale(
                        &source, &staging, &runtime.python, &runtime.model,
                        operation.factor(), &cancel,
                    )?;
                    let bytes = std::fs::read(staged.path())?;
                    let name = format!("{}-{}.png", operation.weights().trim_end_matches(".pth"), operation.factor());
                    let blob = storage::prepare_bytes(
                        &root, quota,
                        storage::ByteIngest { name: &name, source: "image_upscale", bytes: &bytes, origin: &Origin::default() },
                        &references,
                    )?;
                    cancel.check()?;
                    Ok(PreparedWork::ImageUpscale {
                        event, stamp, blob,
                        dimensions: [staged.input_width, staged.input_height, staged.width, staged.height],
                        operation,
                    })
                })?
            }
            Command::ExtractVideoAudio { audio_track } => {
                let event = self.selected_background_video()?;
                let root = self.root.clone();
                let clipboard_sequence = capture::sequence_number();
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let stamp = media_service::stamp(&event.path)?;
                    let path = wallpaper_engine::validate(&root, &event)?;
                    let (destination, staging) = audio_extract::directories(&root)?;
                    let staged = audio_extract::extract(&path, &staging, audio_track, &cancel)?;
                    if media_service::stamp(&path)? != stamp {
                        return Err(ApiError::new("source_changed", "Video changed during audio extraction"));
                    }
                    cancel.check()?;
                    Ok(PreparedWork::AudioExtraction {
                        event,
                        stamp,
                        staged,
                        destination,
                        audio_track: audio_track.unwrap_or(0),
                        clipboard_sequence,
                    })
                })?
            }
            Command::CaptureMediaFrame { session } => return self.capture_media_frame(session),
            Command::OpenMedia { id, revision } => {
                let (event, identity, epoch, desktop_digest) =
                    self.start_media_prepare(id, revision)?;
                self.jobs.submit(Lane::Discovery, move |cancel| {
                    cancel.check()?;
                    let prepared = media_service::prepare(event, identity, epoch, desktop_digest)?;
                    cancel.check()?;
                    Ok(PreparedWork::Media(prepared))
                })?
            }
            Command::SearchEmojiAi {
                query,
                offset,
                limit,
            } => {
                if self.interaction.tab != interaction::ShelfTab::Pods
                    || !self.interaction.emoji_catalogue
                {
                    return Err(ApiError::new("invalid_tab", "Emoji catalogue is not open"));
                }
                emoji_ai::validate(&query, offset, limit)?;
                let view_generation = self.interaction.view_generation;
                let backend = std::sync::Arc::clone(&self.emoji_ai);
                self.jobs.submit(Lane::EmojiAi, move |cancel| {
                    cancel.check()?;
                    let mut backend = backend.lock().map_err(|_| {
                        ApiError::new(
                            "emoji_ai_unavailable",
                            "Local emoji AI backend is unavailable",
                        )
                    })?;
                    cancel.check()?;
                    let value = emoji_ai::search_cancellable(
                        &mut **backend,
                        &query,
                        offset,
                        limit,
                        &cancel,
                    )?;
                    cancel.check()?;
                    Ok(PreparedWork::EmojiAi {
                        value,
                        view_generation,
                    })
                })?
            }
            Command::ListDesktopAssets => {
                let root = self.root.clone();
                let theme = self.settings.theme.clone();
                let remembered = self.settings.desktop_wallpaper_id.clone();
                self.jobs.submit(Lane::Discovery, move |cancel| {
                    cancel.check()?;
                    let assets =
                        desktop_assets::list_available(&root, &theme, remembered.as_deref())?;
                    cancel.check()?;
                    Ok(PreparedWork::Value(assets))
                })?
            }
            Command::BrowseShelfSorted {
                query,
                limit,
                cursor,
            } if self.interaction.tab == interaction::ShelfTab::All => {
                let (generation, snapshot) = self.work_folder.reserve_snapshot();
                let view_generation = self.interaction.view_generation;
                let previous_query = self.interaction.browse_query.clone();
                let sort = self.interaction.sort;
                self.jobs.submit(Lane::Discovery, move |cancel| {
                    cancel.check()?;
                    let page = snapshot.browse(&query, limit, cursor.as_deref(), sort)?;
                    cancel.check()?;
                    Ok(PreparedWork::FolderBrowse {
                        snapshot,
                        generation,
                        view_generation,
                        previous_query,
                        query,
                        sort,
                        page,
                        select_all: false,
                        selection: None,
                    })
                })?
            }
            Command::SelectAll
                if self.interaction.tab == interaction::ShelfTab::All
                    && self.work_folder.current().is_some() =>
            {
                let (generation, snapshot) = self.work_folder.reserve_snapshot();
                let view_generation = self.interaction.view_generation;
                let previous_query = self.interaction.browse_query.clone();
                let query = previous_query.clone();
                let sort = self.interaction.sort;
                let selection = (self.interaction.selected.clone(), self.interaction.cursor);
                self.jobs.submit(Lane::Discovery, move |cancel| {
                    let mut cursor = None;
                    let mut ids = Vec::new();
                    loop {
                        cancel.check()?;
                        let page = snapshot.browse(&query, 1000, cursor.as_deref(), sort)?;
                        for event in page["events"].as_array().ok_or_else(|| {
                            ApiError::new("invalid_page", "Folder page has no events")
                        })? {
                            ids.push(event["id"].as_i64().ok_or_else(|| {
                                ApiError::new("invalid_page", "Folder event has no ID")
                            })?);
                        }
                        cursor = page["next_cursor"].as_str().map(str::to_owned);
                        if cursor.is_none() {
                            break;
                        }
                    }
                    cancel.check()?;
                    Ok(PreparedWork::FolderBrowse {
                        snapshot,
                        generation,
                        view_generation,
                        previous_query,
                        query,
                        sort,
                        page: json!(ids),
                        select_all: true,
                        selection: Some(selection),
                    })
                })?
            }
            Command::EnsureMediaRuntime => self.jobs.submit(Lane::Foreground, move |cancel| {
                cancel.begin_effect()?;
                crate::media_runtime::install()
                    .map(PreparedWork::Value)
                    .map_err(|e| ApiError::new("media_runtime_error", e))
            })?,
            Command::ImportVideoBackground { path } => {
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let prepared = video_backgrounds::prepare(&path)
                        .map_err(|e| ApiError::new("video_background_error", e))?;
                    cancel.check()?;
                    Ok(PreparedWork::VideoBackground {
                        prepared,
                        event: None,
                        activate: false,
                    })
                })?
            }
            Command::UseSelectedVideoAsBackground => {
                let event = self.selected_background_video()?;
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let prepared = video_backgrounds::prepare_event(&event)
                        .map_err(|e| ApiError::new("video_background_error", e))?;
                    cancel.check()?;
                    Ok(PreparedWork::VideoBackground {
                        prepared,
                        event: Some(event),
                        activate: true,
                    })
                })?
            }
            Command::UseSelectedVideoAsDesktopWallpaper
            | Command::UseSelectedVideoAsSilentDesktopWallpaper => {
                let silent = matches!(command, Command::UseSelectedVideoAsSilentDesktopWallpaper);
                let event = self.selected_background_video()?;
                let root = self.root.clone();
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let path = if silent {
                        wallpaper_engine::stage_silent(&root, &event)?
                    } else {
                        wallpaper_engine::stage(&root, &event)?
                    };
                    cancel.check()?;
                    Ok(PreparedWork::VideoWallpaper {
                        event,
                        path,
                        silent,
                    })
                })?
            }
            Command::UseSelectedImageAsDesktopWallpaper => {
                let event = self.selected_background_image()?;
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    cancel.check()?;
                    let prepared = theme_backgrounds::prepare_event(&event)?;
                    cancel.check()?;
                    Ok(PreparedWork::ThemeBackground {
                        prepared,
                        event: Some(event),
                        activate: false,
                        desktop_wallpaper: true,
                    })
                })?
            }
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
                        desktop_wallpaper: false,
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
                        desktop_wallpaper: false,
                    })
                })?
            }
            Command::SearchFoldersSemantic { query, limit } => {
                self.begin_folder_semantic(query, 0, limit, None)?
            }
            Command::SearchFoldersSemanticPage {
                query,
                offset,
                limit,
                snapshot,
            } => self.begin_folder_semantic(query, offset, limit, snapshot)?,
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
                        let files_enabled = self.settings.tab_enabled(interaction::ShelfTab::Files);
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
                                        files_enabled,
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
                let files_enabled = self.settings.tab_enabled(interaction::ShelfTab::Files);
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
                                files_enabled,
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
            Command::TranslationCollectionPage { id, revision, index } => {
                let event = self.get(id)?;
                if !image_collection::is_managed(&event) || event.revision != revision
                    || (event.source == "image_translation_collection" && !self.db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM translated_images WHERE event_id=?1)",
                        [id], |row| row.get::<_, bool>(0)
                    )?)
                {
                    return Err(ApiError::new("stale_selection", "Saved collection is unavailable"));
                }
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    Ok(PreparedWork::Value(image_translation_batch::page(&event, index, &cancel)?))
                })?
            }
            Command::PreviewPublishedTranslation { id, revision } => {
                let event = self.get(id)?;
                let published: bool = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM translated_images WHERE event_id=?1)",
                    [id], |row| row.get(0)
                )?;
                if !published || event.revision != revision {
                    return Err(ApiError::new("stale_selection", "Saved translation is unavailable"));
                }
                self.submit_named_preview(event, false)?
            }
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
            Command::AddFiles { paths } => {
                files_service::validate_paths(&paths)?;
                let origin = self.ingest_origin.clone();
                self.jobs.submit(Lane::Foreground, move |cancel| {
                    let mut prepared = Vec::with_capacity(paths.len());
                    for path in paths {
                        cancel.check()?;
                        let pointer = files_pointer::prepare_pointer(&path).map_err(ApiError::from);
                        prepared.push((path, pointer));
                    }
                    cancel.check()?;
                    Ok(PreparedWork::Files { prepared, origin })
                })?
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
        if image_collection::is_managed(&event) {
            if event.source == "image_translation_collection" {
                return self.begin_job(Command::PreviewPublishedTranslation { id, revision: event.revision });
            }
            let id = self.submit_named_preview(event, true)?;
            return Ok(json!({"job_id":id,"state":"queued"}));
        }
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
    fn submit_named_preview(&mut self, event: Event, wrap_receipt: bool) -> Result<JobId> {
        let root = self.root.clone();
        let quota = self.quota;
        let launcher = self.external_preview.clone();
        self.jobs.submit(Lane::Foreground, move |cancel| {
            cancel.check()?;
            let plan = copy_out::prepare_with_budget(
                &root.join("open-cache"), std::slice::from_ref(&event), quota,
            ).map_err(|e| ApiError::new("open_cache_failed", e))?;
            let path = plan.paths.first().ok_or_else(|| {
                ApiError::new("preview_unavailable", "Could not prepare the saved file")
            })?;
            let launcher = launcher.lock().map_err(|_| {
                ApiError::new("quicklook_busy", "Preview launcher lock poisoned")
            })?;
            cancel.begin_effect()?;
            // QuickLook can open the named snapshot after acknowledging the request.
            plan.retain_without_retiring();
            let receipt = launcher.launch_receipt(path)?;
            Ok(PreparedWork::Value(if wrap_receipt {
                json!({"id":event.id,"revision":event.revision,"backend":"quicklook","requested":true,"receipt":receipt})
            } else {
                receipt
            }))
        })
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
                Ok(PreparedWork::AudioExtraction {event, stamp, staged, destination, audio_track, clipboard_sequence}) => {
                    let saved = (|| -> Result<Value> {
                        let current = self.get(event.id).map_err(|_| ApiError::new("stale_selection", "Video is no longer available"))?;
                        if current.revision != event.revision || current.path != event.path || current.kind != event.kind || media_service::stamp(&event.path)? != stamp {
                            return Err(ApiError::new("source_changed", "Video changed during audio extraction"));
                        }
                        self.jobs.authorize_claimed_effect(ready)?;
                        let path = staged.publish(&destination)?;
                        Ok(json!({"path":path,"audio_track":audio_track}))
                    })();
                    let saved = match saved { Ok(saved) => saved, Err(error) => { self.jobs.finish(ready, Err(error))?; continue; } };
                    let fallback = saved.clone();
                    let path = PathBuf::from(saved["path"].as_str().expect("published path"));
                    if let Err(error) = self.jobs.continue_claimed(ready, move |_| {
                        let prepared = files_pointer::prepare_pointer(&path).map_err(ApiError::from);
                        Ok(PreparedWork::AudioReference { result: saved, clipboard_sequence, prepared })
                    }) {
                        let mut saved = fallback;
                        saved["files"] = json!({"ok":false,"error":error.message});
                        saved["clipboard"] = json!({"ok":false,"error":"Audio was saved but could not be registered in Files"});
                        self.jobs.finish(ready, Ok(saved))?;
                    }
                    continue;
                }
                Ok(PreparedWork::AudioReference { mut result, clipboard_sequence, prepared }) => {
                    let registration = prepared.and_then(|pointer| {
                        let path = PathBuf::from(result["path"].as_str().expect("published path"));
                        self.commit_owned_audio_pointer(pointer, &path)
                    });
                    let event = match registration {
                        Ok(event) => event,
                        Err(error) => {
                            result["files"] = json!({"ok":false,"error":error.message});
                            result["clipboard"] = json!({"ok":false,"error":"Audio was saved but could not be registered in Files"});
                            self.jobs.finish(ready, Ok(result))?;
                            continue;
                        }
                    };
                    result["files"] = json!({"ok":true,"event":event.clone()});
                    result["event"] = event.clone();
                    let parsed = serde_json::from_value::<Event>(event);
                    let references = self.reference_paths();
                    let root = self.root.clone();
                    let quota = self.quota;
                    let fallback = result.clone();
                    if let Err(error) = self.jobs.continue_claimed(ready, move |_| {
                        let prepared = (|| {
                            let event = parsed.map_err(|e| e.to_string())?;
                            let references = references.map_err(|e| e.message)?;
                            let mut plan = copy_out::prepare_with_budget(&root, &[event], quota)?;
                            plan.protect_references(&references)?;
                            let sink = copy_out::PreparedNativeClipboard::prepare(&plan.paths);
                            Ok((plan, sink))
                        })();
                        Ok(PreparedWork::AudioCopy { result, clipboard_sequence, prepared })
                    }) {
                        let mut result = fallback;
                        result["clipboard"] = json!({"ok":false,"error":error.message});
                        self.jobs.finish(ready, Ok(result))?;
                    }
                    continue;
                }
                Ok(PreparedWork::MediaCapture { lease, prepared }) => {
                    let clipboard_sequence = lease.clipboard_sequence;
                    let saved = self
                        .validate_media_capture(&lease)
                        .and_then(|()| self.jobs.authorize_claimed_effect(ready))
                        .and_then(|()| self.commit_media_capture(prepared));
                    let mut saved = match saved {
                        Ok(value) => value,
                        Err(error) => {
                            self.jobs.finish(ready, Err(error))?;
                            continue;
                        }
                    };
                    let event: Event = match serde_json::from_value(saved["event"].clone()) {
                        Ok(event) => event,
                        Err(error) => {
                            saved["clipboard"] = json!({"ok":false,"error":error.to_string()});
                            self.jobs.finish(ready, Ok(saved))?;
                            continue;
                        }
                    };
                    let references = match self.reference_paths() {
                        Ok(value) => value,
                        Err(error) => {
                            saved["clipboard"] = json!({"ok":false,"error":error.message});
                            self.jobs.finish(ready, Ok(saved))?;
                            continue;
                        }
                    };
                    let root = self.root.clone();
                    let quota = self.quota;
                    let fallback = saved.clone();
                    let continued = self.jobs.continue_claimed(ready, move |_cancel| {
                        let prepared = (|| {
                            let mut plan = copy_out::prepare_with_budget(&root, &[event], quota)?;
                            plan.protect_references(&references)?;
                            let sink = copy_out::PreparedNativeClipboard::prepare(&plan.paths);
                            Ok((plan, sink))
                        })();
                        Ok(PreparedWork::MediaCaptureCopy {
                            result: saved,
                            clipboard_sequence,
                            prepared,
                        })
                    });
                    if let Err(error) = continued {
                        let mut result = fallback;
                        result["clipboard"] = json!({"ok":false,"error":error.message});
                        self.jobs.finish(ready, Ok(result))?;
                    }
                    continue;
                }
                Ok(PreparedWork::VideoWallpaper {
                    event,
                    path,
                    silent,
                }) => {
                    let root = self.root.clone();
                    let continued = self
                        .validate_job_events(std::slice::from_ref(&event))
                        .and_then(|()| {
                            self.jobs.authorize_claimed_effect(ready)?;
                            self.jobs.continue_claimed(ready, move |_cancel| {
                                Ok(PreparedWork::Value(wallpaper_engine::handoff(
                                    &root,
                                    &path,
                                    &event.path,
                                    silent,
                                )?))
                            })
                        });
                    if let Err(error) = continued {
                        self.jobs.finish(ready, Err(error))?;
                    }
                    continue;
                }
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
                Ok(PreparedWork::ExternalPasteReady {
                    folder,
                    ids,
                    events,
                    mut errors,
                }) => {
                    if self.work_folder.current() != Some(folder.as_path()) {
                        self.jobs.finish(
                            ready,
                            Err(ApiError::new(
                                "stale_folder",
                                "Working folder changed before paste",
                            )),
                        )?;
                        continue;
                    }
                    let continued = self.jobs.authorize_claimed_effect(ready).and_then(|()| {
                        self.jobs.continue_claimed(ready, move |_cancel| {
                            for event in events {
                                let value = match folder_transfer::materialize(std::slice::from_ref(&event), &folder) {
                                    Ok(receipt) => serde_json::to_value(&receipt.items[0]).map_err(|e| ApiError::new("serialization_error", e))?,
                                    Err(error) => json!({"id":event.id,"path":null,"status":"failed","error":error}),
                                };
                                errors.insert(event.id, value);
                            }
                            Ok(PreparedWork::ExternalPasteFinished { ids, items: errors })
                        })
                    });
                    if let Err(error) = continued {
                        self.jobs.finish(ready, Err(error))?;
                    }
                    continue;
                }
                Ok(PreparedWork::ExternalPasteFinished { ids, mut items }) => {
                    let items: Vec<_> = ids.iter().filter_map(|id| items.remove(id)).collect();
                    self.jobs.finish(
                        ready,
                        Ok(json!({"folder_paste":true,"cut":false,"external":true,"items":items})),
                    )?;
                    continue;
                }
                Ok(PreparedWork::FolderExportReady(mut batch)) => {
                    if self.work_folder.current() != Some(batch.folder.as_path()) {
                        self.jobs.finish(
                            ready,
                            Err(ApiError::new(
                                "stale_folder",
                                "Working folder changed before copy",
                            )),
                        )?;
                        continue;
                    }
                    let mut events = Vec::new();
                    for event in std::mem::take(&mut batch.events) {
                        if !self.get(event.id).is_ok_and(|current| {
                            current.revision == event.revision && current.path == event.path
                        }) {
                            batch.errors.insert(event.id, json!({"id":event.id,"path":null,"status":"failed","error":{"code":"stale_selection","message":"Selection changed before copy"}}));
                            continue;
                        }
                        if batch.moving {
                            if event
                                .path
                                .canonicalize()
                                .ok()
                                .and_then(|path| path.parent().map(|parent| parent == batch.folder))
                                .unwrap_or(false)
                            {
                                self.jobs.authorize_claimed_effect(ready)?;
                                let value = if batch.paste {
                                    json!({"id":event.id,"path":event.path,"status":"same_folder","error":null})
                                } else {
                                    self.prepare_job_same_folder_move(&event, &batch.folder)
                                };
                                batch.errors.insert(event.id, value);
                                continue;
                            }
                            match self.prepare_job_move_source(&event, !batch.paste) {
                                Ok(Some(spec)) => {
                                    batch.lock_specs.insert(event.id, spec);
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    batch.errors.insert(event.id, json!({"id":event.id,"path":null,"status":"failed","error":error}));
                                    continue;
                                }
                            }
                        }
                        events.push(event);
                    }
                    batch.events = events;
                    if batch.events.is_empty() {
                        let items: Vec<_> = batch
                            .ids
                            .iter()
                            .filter_map(|id| batch.errors.remove(id))
                            .collect();
                        let value = if batch.paste {
                            json!({"folder_paste":true,"cut":batch.moving,"items":items})
                        } else {
                            json!({"direction":"to_folder","items":items,"interaction":self.interaction})
                        };
                        self.jobs.finish(ready, Ok(value))?;
                        continue;
                    }
                    let fallback_ids = batch.ids.clone();
                    let mut fallback_items = batch.errors.clone();
                    let fallback_events: Vec<_> =
                        batch.events.iter().map(|event| event.id).collect();
                    let fallback_paste = batch.paste;
                    let fallback_moving = batch.moving;
                    let continued = self.jobs.authorize_claimed_effect(ready).and_then(|()| {
                        self.jobs.continue_claimed(ready, move |_cancel| {
                            let mut batch = batch;
                            let mut events = Vec::new();
                            for event in std::mem::take(&mut batch.events) {
                                if let Some((path, root, hash)) = batch.lock_specs.remove(&event.id) {
                                    match delete_selection::prepare_locked_original(&path, &root, &hash) {
                                        Ok(lock) => { batch.locks.insert(event.id, lock); }
                                        Err(error) => {
                                            batch.errors.insert(event.id, json!({"id":event.id,"path":null,"status":"failed","error":error}));
                                            continue;
                                        }
                                    }
                                }
                                events.push(event);
                            }
                            batch.events = events;
                            let receipt = if batch.events.is_empty() {
                                Ok(folder_transfer::TransferReceipt { items: Vec::new() })
                            } else { folder_transfer::materialize(&batch.events, &batch.folder) };
                            Ok(PreparedWork::FolderExportCopied { batch, receipt })
                        })
                    });
                    if let Err(error) = continued {
                        for id in fallback_events {
                            fallback_items.insert(
                                id,
                                json!({"id":id,"path":null,"status":"failed","error":error}),
                            );
                        }
                        let items: Vec<_> = fallback_ids
                            .iter()
                            .filter_map(|id| fallback_items.remove(id))
                            .collect();
                        let value = if fallback_paste {
                            json!({"folder_paste":true,"cut":fallback_moving,"items":items})
                        } else {
                            json!({"direction":"to_folder","items":items,"interaction":self.interaction})
                        };
                        self.jobs.finish(ready, Ok(value))?;
                    }
                    continue;
                }
                Ok(PreparedWork::FolderExportCopied { mut batch, receipt }) => {
                    match receipt {
                        Ok(receipt) => {
                            for item in receipt.items {
                                let value = if batch.moving && item.status == "copied" {
                                    let event = batch
                                        .events
                                        .iter()
                                        .find(|event| event.id == item.id)
                                        .expect("receipt event");
                                    self.finish_cut(
                                        event,
                                        item.path,
                                        item.content_hash,
                                        batch.locks.remove(&item.id),
                                        !batch.paste,
                                    )
                                } else {
                                    serde_json::to_value(&item).map_err(|error| {
                                        ApiError::new("serialization_error", error)
                                    })?
                                };
                                batch.errors.insert(item.id, value);
                            }
                        }
                        Err(error) => {
                            for event in &batch.events {
                                batch.errors.insert(event.id, json!({"id":event.id,"path":null,"status":"failed","error":error}));
                            }
                        }
                    }
                    let items: Vec<_> = batch
                        .ids
                        .iter()
                        .filter_map(|id| batch.errors.remove(id))
                        .collect();
                    let interaction_error = if batch.moving && !batch.paste {
                        self.select_tab(interaction::ShelfTab::All).err()
                    } else {
                        None
                    };
                    let value = if batch.paste {
                        json!({"folder_paste":true,"cut":batch.moving,"items":items})
                    } else {
                        json!({"direction":"to_folder","items":items,"interaction":self.interaction,"interaction_error":interaction_error})
                    };
                    self.jobs.finish(ready, Ok(value))?;
                    continue;
                }
                Ok(PreparedWork::FolderCopy {
                    mut batch,
                    id,
                    prepared,
                }) => {
                    let result = prepared.and_then(|prepared| {
                        if self.work_folder.current() != Some(batch.folder.as_path()) {
                            return Err(ApiError::new(
                                "stale_folder",
                                "Working folder changed before import",
                            ));
                        }
                        let current = self.get(id)?;
                        if current.revision != prepared.event.revision
                            || current.path != prepared.event.path
                        {
                            return Err(ApiError::new(
                                "stale_selection",
                                "Folder item changed before import",
                            ));
                        }
                        self.jobs.authorize_claimed_effect(ready)?;
                        let source = prepared.event.clone();
                        let event = self.commit_folder_copy(prepared)?;
                        Ok((source, event))
                    });
                    batch.items.push(match result {
                        Ok((source, event)) => {
                            if batch.moving && batch.target.is_none() && event["imported"].as_u64().unwrap_or(0) > 0 {
                                batch.target = match event["kind"].as_str() {
                                    Some("image") => Some(interaction::ShelfTab::Images),
                                    Some("text") => Some(interaction::ShelfTab::Text),
                                    Some("table") => Some(interaction::ShelfTab::Tables),
                                    Some("links") => Some(interaction::ShelfTab::Links),
                                    _ => None,
                                };
                            }
                            let effect = if batch.moving {
                                self.finish_prepared_folder_move(&source, &event)
                            } else { Ok(json!({"status":"copied"})) };
                            match effect {
                                Ok(effect) => json!({"id":id,"ok":event["failed"].as_u64().unwrap_or(0)==0 && effect["status"] != "recycle_failed","status":effect["status"],"recycle":effect["recycle"],"error":null,"event":event}),
                                Err(error) => json!({"id":id,"ok":false,"status":"source_not_moved","error":error,"event":event}),
                            }
                        }
                        Err(error) => json!({"id":id,"ok":false,"error":error}),
                    });
                    if batch.pending.is_empty() {
                        let interaction_error = if batch.moving
                            && let Some(tab) = batch.target
                        {
                            self.select_tab(tab).err()
                        } else {
                            None
                        };
                        let value = if batch.direct {
                            json!({"direction":"from_folder","items":batch.items,"interaction":self.interaction,"interaction_error":interaction_error})
                        } else {
                            json!({"folder_import":true,"items":batch.items})
                        };
                        self.jobs.finish(ready, Ok(value))?;
                    } else {
                        let direct = batch.direct;
                        let fallback_items = batch.items.clone();
                        let fallback_ids: Vec<_> =
                            batch.pending.iter().map(|(id, _)| *id).collect();
                        if let Err(error) = self.jobs.continue_claimed(ready, move |cancel| {
                            prepare_folder_copy_step(batch, cancel)
                        }) {
                            let mut items = fallback_items;
                            items.extend(
                                fallback_ids
                                    .into_iter()
                                    .map(|id| json!({"id":id,"ok":false,"error":error})),
                            );
                            let value = if direct {
                                json!({"direction":"from_folder","items":items,"interaction":self.interaction})
                            } else {
                                json!({"folder_import":true,"items":items})
                            };
                            self.jobs.finish(ready, Ok(value))?;
                        }
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
                PreparedWork::PackImageCollection(packed) => {
                    for input in &packed.inputs {
                        let event = &input.event;
                        let current = self.get(event.id)?;
                        if current.revision != event.revision || current.path != event.path
                            || media_service::stamp(&event.path)? != input.stamp
                        {
                            return Err(ApiError::new("source_changed", "Image changed before collection publication"));
                        }
                    }
                    self.jobs.authorize_claimed_effect(ready)?;
                    let count = packed.count;
                    let merged = image_collection::is_managed(&packed.inputs[0].event);
                    let consumed: Vec<_> = packed.inputs.iter().map(|input| input.event.id).collect();
                    let (id, cleanup_pending) = storage::commit_shelf_image_collection(&mut self.db, &self.root, self.quota, packed)?;
                    self.interaction.replace_collection_rows(&consumed, Some(id));
                    Ok(json!({"event":self.get(id)?,"count":count,"image_collection":true,"merged":merged,"cleanup_pending":cleanup_pending}))
                },
                PreparedWork::UnpackImageCollection { event, stamp, members, pages, newest_first } => {
                    self.validate_job_events(std::slice::from_ref(&event))?;
                    if media_service::stamp(&event.path)? != stamp {
                        return Err(ApiError::new("source_changed", "Collection changed before extraction"));
                    }
                    self.jobs.authorize_claimed_effect(ready)?;
                    let (items, cleanup_pending) = storage::commit_extracted_collection_pages(
                        &mut self.db, &self.root, self.quota, event.id, &members, pages,
                        event.source == "image_translation_collection", newest_first,
                    )?;
                    self.interaction.replace_collection_rows(&[event.id], items.first().copied());
                    Ok(json!({"count":items.len(),"items":items,"image_collection":true,"unpacked":true,"cleanup_pending":cleanup_pending}))
                },
                PreparedWork::ImageTranslationBatch(batch) => {
                    for (event, stamp) in &batch.validated_sources {
                        let current = self.get(event.id)?;
                        if current.revision != event.revision || current.path != event.path
                            || media_service::stamp(&event.path)? != *stamp
                        {
                            return Err(ApiError::new("source_changed", "Page changed before collection publication"));
                        }
                    }
                    self.jobs.authorize_claimed_effect(ready)?;
                    let id = storage::commit_translation_collection(&mut self.db, &self.root, self.quota, batch.blob)?;
                    let mut receipt = batch.receipt;
                    receipt["event"] = serde_json::to_value(self.get(id)?)
                        .map_err(|error| ApiError::new("collection_error", error))?;
                    Ok(receipt)
                },
                PreparedWork::ImageTranslation { event, stamp, blob, target, source, dimensions: [width,height], report } => {
                    let current = self.get(event.id).map_err(|_| ApiError::new("stale_selection","Image is no longer available"))?;
                    if current.revision != event.revision || current.path != event.path || current.kind != event.kind
                        || media_service::stamp(&event.path)? != stamp {
                        return Err(ApiError::new("source_changed","Image changed during translation"));
                    }
                    self.jobs.authorize_claimed_effect(ready)?;
                    let id = storage::commit_translated_png(&mut self.db, &self.root, self.quota, blob)?;
                    Ok(json!({"event": self.get(id)?, "source_language": source.code(), "target_language": target,
                        "width": width, "height": height, "rendered_blocks": report.rendered,
                        "skipped_blocks": report.skipped, "fallback_blocks": report.fallback,
                        "side_blocks": report.side, "source_left": report.left_pad}))
                },
                PreparedWork::ImageUpscale { event, stamp, blob, dimensions, operation } => {
                    let current = self.get(event.id).map_err(|_| {
                        ApiError::new("stale_selection", "Image is no longer available")
                    })?;
                    if current.revision != event.revision
                        || current.path != event.path
                        || current.kind != event.kind
                        || media_service::stamp(&event.path)? != stamp
                    {
                        return Err(ApiError::new("source_changed", "Image changed during upscale"));
                    }
                    self.jobs.authorize_claimed_effect(ready)?;
                    let id = storage::commit_prepared_bytes(&mut self.db, &self.root, self.quota, blob)?;
                    let [input_width, input_height, width, height] = dimensions;
                    Ok(json!({"event": self.get(id)?, "input_width": input_width, "input_height": input_height,
                        "width": width, "height": height, "operation": operation}))
                },
                PreparedWork::AudioExtraction {..} | PreparedWork::AudioReference {..} => Err(ApiError::new("invalid_job_phase", "Audio extraction requires continuation")),
                PreparedWork::AudioCopy {mut result, clipboard_sequence, prepared} => {
                    let event = serde_json::from_value::<Event>(result["event"].clone()).map_err(|e| e.to_string());
                    let publication = if capture::sequence_number() != clipboard_sequence {
                        Err("Clipboard changed since extraction was requested".to_owned())
                    } else {
                        event.and_then(|event| self.validate_job_events(&[event]).map_err(|e| e.message))
                            .and_then(|()| prepared.and_then(|(plan, mut sink)| self.commit_media_copy_plan(&plan, &mut sink)))
                    };
                    result["clipboard"] = match publication {
                        Ok(receipt) => {
                            let ok = receipt.failures.is_empty() && receipt.count == 1;
                            if ok {
                                if let Some(id) = result["event"]["id"].as_i64() { self.record_selection_publication(receipt.sequence, &[id]); }
                            } else { self.record_clipboard_publication(receipt.sequence); }
                            json!({"ok":ok,"count":receipt.count,"failures":receipt.failures,"warnings":receipt.warnings,"sequence":receipt.sequence,"publication":receipt.publication})
                        }
                        Err(error) => json!({"ok":false,"error":error}),
                    };
                    Ok(result)
                },
                PreparedWork::MediaCapture{..}=>Err(ApiError::new("invalid_job_phase", "Media capture requires continuation")),
                PreparedWork::MediaCaptureCopy{result,clipboard_sequence,prepared}=>Ok(self.publish_media_capture(result,prepared,clipboard_sequence)),
                PreparedWork::Media(prepared)=>{self.jobs.authorize_claimed_effect(ready)?;self.commit_media(prepared)},
                PreparedWork::ExternalPasteReady{..} | PreparedWork::ExternalPasteFinished{..} => Err(ApiError::new("invalid_job_phase", "External paste requires continuation")),
                PreparedWork::FolderExportReady(_) | PreparedWork::FolderExportCopied{..} => Err(ApiError::new("invalid_job_phase", "Folder export requires continuation")),
                PreparedWork::FolderCopy{..}=>Err(ApiError::new("invalid_job_phase","Folder copy requires continuation")),
                PreparedWork::FolderBrowse{snapshot,generation,view_generation,previous_query,query,sort,page,select_all,selection}=>{
                    if self.interaction.tab != interaction::ShelfTab::All || self.interaction.view_generation != view_generation || self.interaction.sort != sort || self.interaction.browse_query != previous_query {
                        return Err(ApiError::new("stale_folder", "Folder browsing context changed"));
                    }
                    if let Some((selected,cursor))=selection && (self.interaction.selected != selected || self.interaction.cursor != cursor) {
                        return Err(ApiError::new("stale_folder", "Folder selection changed"));
                    }
                    self.work_folder.commit_snapshot(generation,snapshot)?;
                    self.interaction.browse_query=query;
                    if select_all {
                        let ids: Vec<i64> = serde_json::from_value(page).map_err(|e| ApiError::new("invalid_page",e))?;
                        self.interaction.select_all(ids);
                        Ok(json!(self.interaction))
                    } else { Ok(page) }
                },
                PreparedWork::ThemeBackground{prepared,event,activate,desktop_wallpaper}=>{
                    if let Some(event)=event {self.validate_job_events(&[event])?;}
                    self.jobs.authorize_claimed_effect(ready)?;
                    let result=theme_backgrounds::commit(&self.root,prepared)?;
                    if desktop_wallpaper {
                        let path = result["path"].as_str().ok_or_else(|| ApiError::new("wallpaper_error", "Managed wallpaper path is missing"))?;
                        desktop_wallpaper::set_wallpaper(std::path::Path::new(path), &desktop_wallpaper::NativeWallpaperHost)
                            .map_err(|e| ApiError::new("wallpaper_error", e))?;
                        self.record_desktop_wallpaper(result["id"].as_str().unwrap_or(""))?;
                        Ok(json!({"desktop_wallpaper_applied":true,"background":result}))
                    } else if activate {self.activate_background(result)}else{Ok(result)}
                },
                PreparedWork::VideoBackground{prepared,event,activate}=>{
                    if let Some(event)=event {self.validate_job_events(&[event])?;}
                    self.jobs.authorize_claimed_effect(ready)?;
                    let result=video_backgrounds::commit(&self.root,prepared).map_err(|e|ApiError::new("video_background_error",e))?;
                    if activate {self.activate_video_background(result)}else{Ok(result)}
                },
                PreparedWork::VideoWallpaper{..}=>Err(ApiError::new("invalid_job_phase","Wallpaper request must be authorized before effect")),
                PreparedWork::EmojiFont(font)=> {self.jobs.authorize_claimed_effect(ready)?; emoji_fonts::commit(&self.root,font)},
                PreparedWork::EmojiAi{value,view_generation}=>{
                    if self.interaction.tab != interaction::ShelfTab::Pods || !self.interaction.emoji_catalogue || self.interaction.view_generation != view_generation {
                        return Err(ApiError::new("stale_context", "Emoji catalogue context changed during search"));
                    }
                    Ok(value)
                },
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
                PreparedWork::Files{prepared,origin}=>{self.jobs.authorize_claimed_effect(ready)?;self.commit_files(prepared,origin)},
                PreparedWork::Reference(prepared)=>self.commit_prepared_reference(prepared),
                PreparedWork::Copy{events,plan,mut sink}=> {
                    self.validate_job_events(&events)?;
                    let receipt=copy_out::commit(&plan,&mut sink).map_err(|e|ApiError::new("clipboard_error",e))?;if receipt.failures.is_empty() {self.record_selection_publication(receipt.sequence,&events.iter().map(|event|event.id).collect::<Vec<_>>());}else{self.record_clipboard_publication(receipt.sequence);}
                    Ok(json!({"copied":receipt.count,"clipboard_sequence":receipt.sequence,"failures":receipt.failures,"warnings":receipt.warnings,"publication":receipt.publication}))
                }
                PreparedWork::Drag{events,prepared}=> {self.validate_job_events(&events)?;self.commit_drag_preparation(prepared)}
            });
            self.jobs.finish(ready, result)?;
        }
        Ok(json!(self.jobs.status(id)?))
    }
}

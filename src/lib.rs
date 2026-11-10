use rusqlite::{Connection, OptionalExtension, params};
pub mod capture;
pub mod capture_events;
mod catalogue_query;
mod catalogue_schema;
mod classification_runtime;
pub mod clipboard_listener;
pub mod copy_out;
mod delete_jobs;
mod delete_selection;
pub mod delete_shelf;
pub mod density;
pub mod diagnostics;
pub mod domain_jobs;
pub mod downloads;
pub mod drag_out;
mod drag_session;
pub mod file_types;
pub mod file_visual;
mod folder_commit;
pub mod folder_watch;
pub mod interaction;
mod jobs_service;
pub mod origin;
pub mod screenshot_identity;
pub mod settings;
pub mod sharex;
mod shelf_tabs;
pub mod shell_visual_worker;
pub mod shortcuts;
pub mod source_policy;
pub mod source_runtime;
mod source_service;
pub mod tab_layout;
use interaction::Interaction;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub mod actions;
pub mod api;
pub mod preview;
pub mod preview_view;
pub mod quicklook;
pub mod quicklook_backend;
pub mod quicklook_process;
pub mod quicklook_transport;
pub mod storage;
pub use api::*;

/// The only action dispatcher. Transports and future UI call this contract.
pub struct Service {
    trash: std::sync::Arc<std::sync::Mutex<Box<dyn downloads::Trash>>>,
    jobs: domain_jobs::DomainJobs<jobs_service::PreparedWork>,
    classification: classification_runtime::ClassificationRuntime,
    source_runtime: source_runtime::SourceRuntime,
    source_configured: bool,
    source_runtime_started: bool,
    external_preview: std::sync::Arc<std::sync::Mutex<Box<dyn quicklook::Launcher>>>,
    sharex_scanner: sharex::SharexScanner,
    downloads_scanner: downloads::DownloadsScanner,
    capture_events: capture_events::Schedule,
    capture_clock: Box<dyn capture_events::Clock>,
    capture_source: Option<Box<dyn capture_events::Source>>,
    pending_drag: Option<drag_session::Prepared>,
    drag_runner: Box<dyn drag_out::Runner>,
    diagnostics: Option<diagnostics::Logger>,
    db: Connection,
    root: PathBuf,
    quota: u64,
    capture_enabled: bool,
    clipboard_sequence: u32,
    interaction: Interaction,
    settings: settings::Settings,
    shortcuts: shortcuts::Shortcuts,
    ingest_origin: origin::Origin,
    read_only: bool,
}
impl Service {
    /// Injectable external-preview boundary for synthetic tests and embedded clients.
    pub fn set_preview_launcher(&mut self, launcher: Box<dyn quicklook::Launcher>) {
        self.external_preview = std::sync::Arc::new(std::sync::Mutex::new(launcher));
    }
    pub fn set_trash(&mut self, trash: Box<dyn downloads::Trash>) {
        self.trash = std::sync::Arc::new(std::sync::Mutex::new(trash));
    }

    pub fn attach_logger(&mut self, logger: diagnostics::Logger) {
        self.diagnostics = Some(logger);
    }
    pub fn logger(&self) -> Option<diagnostics::Logger> {
        self.diagnostics.clone()
    }
    pub fn open(root: &Path, quota: u64) -> Result<Self> {
        fs::create_dir_all(root.join("blobs"))?;
        let root = fs::canonicalize(root)?;
        let db = Connection::open(root.join("index.sqlite3"))?;
        catalogue_schema::check_version(&db)?;
        catalogue_schema::backup_before_migration(&db, &root)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS blobs(hash TEXT PRIMARY KEY, size INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS events(
              id INTEGER PRIMARY KEY, detected_at_ms INTEGER NOT NULL, source TEXT NOT NULL,
              name TEXT NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL,
              blob_hash TEXT REFERENCES blobs(hash));",
        )?;
        let has_origin: bool = db.query_row(
            "SELECT COUNT(*)>0 FROM pragma_table_info('events') WHERE name='origin_json'",
            [],
            |r| r.get(0),
        )?;
        if !has_origin {
            db.execute_batch(
                "ALTER TABLE events ADD COLUMN origin_json TEXT NOT NULL DEFAULT '{}';",
            )?;
        }
        storage::initialize(&db, &root)?;
        catalogue_schema::migrate(&db)?;
        classification_runtime::recover_startup(&db)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS preferences (key TEXT PRIMARY KEY, document TEXT NOT NULL);")?;
        let stored: Option<String> = db
            .query_row(
                "SELECT document FROM preferences WHERE key='general'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let settings: settings::Settings = match stored {
            Some(s) => serde_json::from_str(&s).map_err(|e| ApiError::new("settings_error", e))?,
            None => settings::Settings::defaults(quota),
        };
        settings
            .validate(0)
            .map_err(|e| ApiError::new("settings_error", e))?;
        let mut interaction = Interaction::default();
        interaction.view = settings.default_view;
        let shortcuts =
            shortcuts::Shortcuts::load(&root).map_err(|e| ApiError::new(&e.code, e.message))?;
        Ok(Self {
            trash: std::sync::Arc::new(std::sync::Mutex::new(Box::new(downloads::NativeTrash))),
            jobs: Default::default(),
            classification: Default::default(),
            source_runtime: Default::default(),
            source_configured: false,
            source_runtime_started: false,
            external_preview: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                quicklook::NativeLauncher,
            ))),
            sharex_scanner: Default::default(),
            downloads_scanner: Default::default(),
            capture_events: Default::default(),
            capture_clock: Box::new(capture_events::SystemClock::default()),
            capture_source: None,
            pending_drag: None,
            drag_runner: Box::new(drag_out::NativeRunner),
            diagnostics: None,
            db,
            root,
            quota: settings.quota_bytes,
            capture_enabled: settings.capture_on_start,
            clipboard_sequence: 0,
            interaction,
            settings,
            shortcuts,
            ingest_origin: origin::Origin::default(),
            read_only: false,
        })
    }

    pub fn open_read_only(root: &Path) -> Result<Self> {
        let root = fs::canonicalize(root)?;
        let db = Connection::open_with_flags(
            root.join("index.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        Ok(Self {
            trash: std::sync::Arc::new(std::sync::Mutex::new(Box::new(downloads::NativeTrash))),
            jobs: Default::default(),
            classification: Default::default(),
            source_runtime: Default::default(),
            source_configured: false,
            source_runtime_started: false,
            external_preview: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                quicklook::NativeLauncher,
            ))),
            sharex_scanner: Default::default(),
            downloads_scanner: Default::default(),
            capture_events: Default::default(),
            capture_clock: Box::new(capture_events::SystemClock::default()),
            capture_source: None,
            pending_drag: None,
            drag_runner: Box::new(drag_out::NativeRunner),
            diagnostics: None,
            db,
            root,
            quota: DEFAULT_QUOTA,
            capture_enabled: false,
            clipboard_sequence: 0,
            interaction: Interaction::default(),
            settings: settings::Settings::default(),
            shortcuts: shortcuts::Shortcuts::default(),
            ingest_origin: origin::Origin::default(),
            read_only: true,
        })
    }
    pub fn execute(&mut self, request: Request) -> serde_json::Value {
        let started = std::time::Instant::now();
        let hot = diagnostics::hot_command(&request.command);
        if hot.is_none()
            && let Some(log) = &self.diagnostics
        {
            log.record("command",serde_json::json!({"request_id":request.request_id,"command":diagnostics::summarize_command(&request.command),"cursor":self.interaction.cursor,"selected_count":self.interaction.selected.len(),"selected_sample":self.interaction.selected.iter().take(32).collect::<Vec<_>>()}));
        }
        let result = if request.api_version != API_VERSION {
            Err(ApiError::new(
                "unsupported_version",
                "Expected api_version 1",
            ))
        } else {
            self.dispatch(request.command)
        };
        if (hot.is_none()
            || result.is_err()
            || started.elapsed() >= std::time::Duration::from_millis(10))
            && let Some(log) = &self.diagnostics
        {
            log.record("result",match &result{Ok(value)=>serde_json::json!({"request_id":request.request_id,"ok":true,"elapsed_us":started.elapsed().as_micros(),"id":value.get("id"),"copied":value.get("copied"),"deleted":value.get("deleted"),"captured":value.get("captured")}),Err(error)=>serde_json::json!({"request_id":request.request_id,"ok":false,"elapsed_us":started.elapsed().as_micros(),"error":error})});
        }
        match result {
            Ok(value) => {
                serde_json::json!({"api_version":API_VERSION,"request_id":request.request_id,"ok":true,"result":value})
            }
            Err(error) => {
                serde_json::json!({"api_version":API_VERSION,"request_id":request.request_id,"ok":false,"error":error})
            }
        }
    }

    fn usage(&self) -> Result<u64> {
        Ok(self
            .db
            .query_row("SELECT COALESCE(SUM(size),0) FROM blobs", [], |r| r.get(0))?)
    }
    fn dispatch(&mut self, command: Command) -> Result<serde_json::Value> {
        if self.read_only
            && !matches!(
                command,
                Command::Get { .. } | Command::PreviewImage { .. } | Command::PreviewFile { .. }
            )
        {
            return Err(ApiError::new(
                "read_only",
                "Preview client cannot mutate state",
            ));
        }
        match command {
            Command::BeginJob{command} => self.begin_job(*command),
            Command::PollJob{job_id} => self.poll_job(job_id),
            Command::CancelJob{job_id} => {
                Ok(serde_json::json!(self.jobs.cancel(job_id)?))
            }
            Command::EnsurePreviewBackend => quicklook_backend::warmup(),
            Command::PreviewBackendStatus => quicklook_backend::status(),
            Command::EnsureSources => self.ensure_sources(),
            Command::PollSources { limit } => self.poll_sources(limit),
            Command::SourceStatus => Ok(serde_json::json!({"sources":self.source_runtime.statuses()})),
            Command::RescanSource { source_id } => {
                self.ensure_sources()?;
                self.source_runtime.rescan(source_id)?;
                Ok(serde_json::json!({"scheduled":true,"sources":self.source_runtime.statuses()}))
            }
            Command::AdjustDensity { delta } => {
                let mut settings = self.settings.clone();
                match self.interaction.view {
                    interaction::View::Gallery => settings.gallery_columns=density::adjust_density(settings.gallery_columns,delta),
                    interaction::View::List => settings.stream_rows=density::adjust_density(settings.stream_rows,delta),
                }
                self.dispatch(Command::SetSettings{settings})
            },
            Command::CommitFolderCandidates { .. } => Err(ApiError::new("untrusted_worker_result","Prepared source results are accepted only from the domain supervisor")),
            Command::ScreenshotMatchInputs => self.screenshot_match_inputs(),
            Command::CommitScreenshotMatches { .. } => Err(ApiError::new("untrusted_worker_result","Pixel identity must be verified by the domain worker")),
            Command::SelectTabPosition { position } => {
                let tab = tab_layout::tab_at(&self.settings.tab_order,position)?;
                self.interaction.set_tab(tab);
                Ok(serde_json::json!(self.interaction))
            },
            Command::MoveTab { tab,index } => {
                let mut settings=self.settings.clone();
                tab_layout::reorder(&mut settings.tab_order,tab,index).map_err(|e|ApiError::new("invalid_tab_order",e))?;
                self.dispatch(Command::SetSettings { settings })
            },
            Command::SetTab { tab } => { self.interaction.set_tab(tab); Ok(serde_json::json!(self.interaction)) },
            Command::ToggleTab => { self.interaction.toggle_tab(); Ok(serde_json::json!(self.interaction)) },
            Command::BrowseShelf { query, limit, after_id } => self.browse_shelf(&query, limit, after_id),
            Command::ScanSharex => self.dispatch(Command::RescanSource{source_id:source_policy::SourceId::Sharex}),
            Command::ScanDownloads => self.dispatch(Command::RescanSource{source_id:source_policy::SourceId::Downloads}),
            Command::CaptureClipboard { trigger } => self.capture_from_event(trigger),
            Command::ActivateWindow => {
                self.interaction.activation_serial = self.interaction.activation_serial.wrapping_add(1);
                Ok(serde_json::json!({"activation_requested": true, "serial": self.interaction.activation_serial}))
            }
            Command::PreviewGeometry { viewport, fitted } => {
                if self.interaction.preview.is_none() { return Err(ApiError::new("preview_closed", "Open an image preview first")); }
                self.interaction.preview_view.geometry(viewport, fitted)
            }
            Command::ZoomPreview { multiplier, anchor, viewport, fitted } => {
                if self.interaction.preview.is_none() { return Err(ApiError::new("preview_closed", "Open an image preview first")); }
                self.interaction.preview_view.zoom(multiplier, anchor, viewport, fitted)?;
                Ok(serde_json::json!(self.interaction))
            }
            Command::PanPreview { delta, viewport, fitted } => {
                if self.interaction.preview.is_none() { return Err(ApiError::new("preview_closed", "Open an image preview first")); }
                self.interaction.preview_view.pan(delta, viewport, fitted)?;
                Ok(serde_json::json!(self.interaction))
            }
            Command::PreviewExternal { id } => self.preview_external(id),
            Command::TogglePreview => {
                if self.interaction.preview.is_none() && let Some(id)=self.interaction.cursor {
                    let event=self.get(id)?;
                    if !quicklook::is_native_image(&event) { return self.preview_external(event.id); }
                }
                self.interaction.set_preview(if self.interaction.preview.is_some() {
                    None
                } else {
                    self.interaction.cursor
                });
                Ok(serde_json::json!(self.interaction))
            }
            Command::NavigatePreview { delta } => {
                if self.interaction.preview.is_some() {
                    self.interaction.navigate(delta, interaction::SelectionMode::Replace);
                    self.interaction.set_preview(self.interaction.cursor);
                }
                Ok(serde_json::json!(self.interaction))
            }
            Command::DragEventsChecked { items } => self.prepare_drag(items),
            Command::RunPreparedDrag { token, owner_window } => self.run_prepared_drag(&token, owner_window),
            Command::CancelPreparedDrag { token } => self.cancel_prepared_drag(&token),
            Command::MaintainCache=>{let before=self.usage()?;storage::initialize(&self.db,&self.root)?;Ok(serde_json::json!({"reclaimed_blob_bytes":before.saturating_sub(self.usage()?)}))},
            Command::CopyEventsChecked{items}=>{
                if items.is_empty(){return Err(ApiError::new("invalid_argument","No selected objects"));}
                for item in &items {let event=self.get(item.id).map_err(|_|ApiError::new("stale_selection","Selection no longer exists; refresh the shelf"))?;if event.revision!=item.revision{return Err(ApiError::new("stale_selection","Selected content changed; refresh before copying"));}
if let Some(expected)=event.revision.strip_prefix("ref:") && preview::content_hash(&event.path)?!=expected{return Err(ApiError::new("stale_selection","Source contents changed; copy the current source again"));}}
                self.dispatch(Command::CopyEvents{ids:items.into_iter().map(|i|i.id).collect()})
            }
            Command::CopyEvents { ids } => {
                if ids.is_empty() || ids.len() > 1000 {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Expected 1..1000 selected events",
                    ));
                }
                let mut unique = std::collections::HashSet::new();
                let events = ids
                    .into_iter()
                    .filter(|id| unique.insert(*id))
                    .map(|id| self.get(id))
                    .collect::<Result<Vec<_>>>()?;
                let mut plan = copy_out::prepare_with_budget(&self.root, &events,self.quota)
                    .map_err(|e| ApiError::new("copy_error", e))?;
                let references={let mut statement=self.db.prepare("SELECT path FROM events WHERE blob_hash IS NULL")?;statement.query_map([],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?};
                plan.protect_references(&references.into_iter().map(PathBuf::from).collect::<Vec<_>>()).map_err(|e|ApiError::new("copy_error",e))?;
                let receipt = copy_out::commit(&plan, &mut copy_out::NativeClipboard)
                    .map_err(|e| ApiError::new("clipboard_error", e))?;
                self.clipboard_sequence = receipt.sequence;
                Ok(
                    serde_json::json!({"copied":receipt.count,"failures":receipt.failures,"warnings":receipt.warnings}),
                )
            }
            Command::IngestWithOrigin { origin, command } => {
                if !matches!(
                    *command,
                    Command::IngestPath { .. } | Command::IngestBytes { .. }
                ) {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Expected ingest_path or ingest_bytes",
                    ));
                }
                let clipboard_source = match command.as_ref() {
                    Command::IngestPath { source, .. } | Command::IngestBytes { source, .. } => source.starts_with("clipboard_"),
                    _ => false,
                };
                if clipboard_source && origin.source_app.as_deref().is_some_and(|app| app.eq_ignore_ascii_case("sharex")) {
                    return Ok(serde_json::json!({"ignored":true,"reason":"sharex_uses_folder_source"}));
                }
                self.ingest_origin = origin;
                let result = self.dispatch(*command);
                self.ingest_origin = origin::Origin::default();
                result
            }
            Command::DeleteSelection { ids, confirmed } => self.delete_selection(ids, confirmed),
            Command::DeleteEvents { ids, confirmed } => {
                let result =
                    delete_shelf::delete_events(&mut self.db, &self.root, &ids, confirmed)?;
                if result.deleted > 0 {
                    self.interaction
                        .selected
                        .retain(|id| !result.ids.contains(id));
                    if self
                        .interaction
                        .preview
                        .is_some_and(|id| result.ids.contains(&id))
                    {
                        self.interaction.set_preview(None);
                    }
                }
                Ok(serde_json::json!(result))
            }
            Command::Shortcuts => Ok(serde_json::json!(self.shortcuts.catalogue())),
            Command::SetShortcut {
                id,
                binding,
                replace,
                reset,
            } => self
                .shortcuts
                .set(&self.root, &id, binding, replace, reset)
                .map(|items| serde_json::json!(items))
                .map_err(|e| ApiError::new(&e.code, e.message)),
            Command::GetSettings => Ok(serde_json::json!(self.settings)),
            Command::SetSettings { settings } => {
                settings
                    .validate(self.usage()?)
                    .map_err(|e| ApiError::new("invalid_settings", e))?;
                let document = serde_json::to_string(&settings)
                    .map_err(|e| ApiError::new("settings_error", e))?;
                self.db.execute("INSERT INTO preferences(key,document) VALUES('general',?1) ON CONFLICT(key) DO UPDATE SET document=excluded.document",[document])?;
                if self.settings.sharex_folder != settings.sharex_folder { self.sharex_scanner = Default::default(); }
                if self.settings.downloads_folder != settings.downloads_folder || self.settings.downloads_enabled != settings.downloads_enabled { self.downloads_scanner = Default::default(); }
                let sources_changed=self.settings.sharex_folder!=settings.sharex_folder || self.settings.downloads_enabled!=settings.downloads_enabled || self.settings.downloads_folder!=settings.downloads_folder;
                self.quota = settings.quota_bytes;
                self.settings = settings;
                if sources_changed {self.source_configured = false;}
                self.configure_sources(self.source_runtime_started)?;
                Ok(serde_json::json!(self.settings))
            }
            Command::ShowSettings => {
                self.interaction.settings_open = true;
                Ok(serde_json::json!(self.interaction))
            }
            Command::CloseSettings => {
                self.interaction.settings_open = false;
                Ok(serde_json::json!(self.interaction))
            }
            Command::Search{query,limit}=>{let events=storage::query_events(&self.db,&query,limit,None)?;let cursor=if events.len()==limit as usize{events.last().map(|e|e.id)}else{None};Ok(serde_json::json!({"events":events,"next_cursor":cursor}))},
            Command::SearchPage{query,limit,after_id}=>{let events=storage::query_events(&self.db,&query,limit,after_id)?;let cursor=if events.len()==limit as usize{events.last().map(|e|e.id)}else{None};Ok(serde_json::json!({"events":events,"next_cursor":cursor}))},
            Command::Interaction => Ok(serde_json::json!(self.interaction)),
            Command::SetVisible { ids } => {
                self.interaction.visible(ids);
                Ok(serde_json::json!(self.interaction))
            }
            Command::Select { id, mode } => {
                self.interaction.select(id, mode);
                Ok(serde_json::json!(self.interaction))
            }
            Command::Navigate { delta, mode } => {
                self.interaction.navigate(delta, mode);
                Ok(serde_json::json!(self.interaction))
            }
            Command::SetView { view } => {
                self.interaction.view = view;
                Ok(serde_json::json!(self.interaction))
            }
            Command::OpenCurrent => {
                if let Some(id)=self.interaction.cursor {
                    let event=self.get(id)?;
                    if !quicklook::is_native_image(&event) { return self.preview_external(event.id); }
                }
                self.interaction.set_preview(self.interaction.cursor);
                Ok(serde_json::json!(self.interaction))
            }
            Command::ClosePreview => {
                self.interaction.set_preview(None);
                Ok(serde_json::json!(self.interaction))
            }
            Command::Actions => Ok(serde_json::Value::Array(actions::DEFS.iter().filter(|d|d.in_palette).map(|d|serde_json::json!({"id":d.id,"label":if d.id=="capture" {if self.capture_enabled {"Приостановить захват"}else{"Включить захват"}}else{d.label},"group":d.group,"shortcut":self.shortcuts.binding(d.id)})).collect())),
            Command::RunAction{id}=>match actions::resolve(&id,&self.interaction,self.capture_enabled) {Some(actions::Dispatch::Command(command))=>self.dispatch(command),Some(actions::Dispatch::Intent(value))=>Ok(value),None=>Err(ApiError::new("unknown_action","Unknown action"))},
            Command::PreviewImage{id,max_edge}=>preview::render(&self.root,&self.get(id)?,max_edge),
            Command::PreviewFile{id,max_edge}=>file_visual::render(&self.root,&self.get(id)?,max_edge),
            Command::SetCaptureEnabled { enabled } => {
                self.capture_events.cancel();
                self.capture_enabled = enabled;
                #[cfg(windows)]
                {
                    self.clipboard_sequence = unsafe {
                        windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber()
                    };
                }
                Ok(serde_json::json!({"capture_enabled":enabled}))
            }
            Command::PollClipboard => self.poll_clipboard(),
            Command::Status => Ok(
                serde_json::json!({"cached_bytes":self.usage()?,"quota_bytes":self.quota,"capture_enabled":self.capture_enabled,"quota_policy":"reject_new","session_cleanup":"not_implemented","diagnostics":self.diagnostics.as_ref().map(|l|l.status())}),
            ),
            Command::List{limit}=>Ok(serde_json::json!({"events":storage::query_events(&self.db,"",limit,None)?})),
            Command::Get { id } => Ok(serde_json::json!(self.get(id)?)),
            Command::IngestPath { path, source } => {
                validate_source(&source)?;
                let path = fs::canonicalize(path)?;
                if path.starts_with(&self.root){return Err(ApiError::new("managed_source","Katran's managed files cannot be ingested as external references"));}
                let meta = fs::metadata(&path)?;
                if !meta.is_file() {
                    return Err(ApiError::new("invalid_argument", "Expected regular file"));
                }
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode filename"))?
                    .to_owned();
                let id = self.insert(&name, &source, &path, meta.len(), None)?;
                Ok(serde_json::json!(self.get(id)?))
            }
            Command::IngestBytes{name,bytes,source}=>{validate_name(&name)?;validate_source(&source)?;let id=storage::ingest_bytes(&mut self.db,&self.root,self.quota,storage::ByteIngest{name:&name,source:&source,bytes:&bytes,origin:&self.ingest_origin})?;Ok(serde_json::json!(self.get(id)?))},
            Command::Import { ids, destination } => {
                if ids.is_empty() || ids.len() > 1000 {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Expected 1..1000 event IDs",
                    ));
                }
                let destination = fs::canonicalize(destination)?;
                if !destination.is_dir() {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Destination must be an existing directory",
                    ));
                }
                // Validate all IDs before starting. Individual copy failures are
                // returned explicitly; a batch is not an atomic filesystem transaction.
                let events = ids
                    .into_iter()
                    .map(|id| self.get(id))
                    .collect::<Result<Vec<_>>>()?;
                let results: Vec<_> = events
                    .into_iter()
                    .map(|event| {
                        let result = export(&event, &destination);
                        match result {
                            Ok(path) => serde_json::json!({"id":event.id,"ok":true,"path":path}),
                            Err(error) => {
                                serde_json::json!({"id":event.id,"ok":false,"error":error})
                            }
                        }
                    })
                    .collect();
                Ok(serde_json::json!({"items":results,"atomic":false}))
            }
        }
    }
    fn insert(
        &self,
        name: &str,
        source: &str,
        path: &Path,
        size: u64,
        hash: Option<&str>,
    ) -> Result<i64> {
        let path = path
            .to_str()
            .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode path"))?;
        let reference_hash = if hash.is_none() {
            Some(preview::content_hash(Path::new(path))?)
        } else {
            None
        };
        self.db.execute("INSERT INTO events(detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![now_ms(),source,name,path,size,hash,serde_json::to_string(&self.ingest_origin).unwrap(),reference_hash])?;
        let id = self.db.last_insert_rowid();
        classification_runtime::enqueue(&self.db, id)?;
        Ok(id)
    }
    fn preview_external(&self, id: i64) -> Result<serde_json::Value> {
        let launcher = self
            .external_preview
            .lock()
            .map_err(|_| ApiError::new("quicklook_busy", "Preview launcher lock poisoned"))?;
        quicklook::preview(&self.get(id)?, launcher.as_ref())
    }
    fn get(&self, id: i64) -> Result<Event> {
        storage::get_event(&self.db, id)
    }
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn validate_source(source: &str) -> Result<()> {
    if source.trim().is_empty() || source.len() > 256 {
        return Err(ApiError::new(
            "invalid_argument",
            "source must contain 1..256 bytes",
        ));
    }
    Ok(())
}
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 200
        || name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        || name.ends_with([' ', '.'])
        || name == "."
        || name == ".."
    {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected a safe plain filename",
        ));
    }
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&base.as_str())
    {
        return Err(ApiError::new(
            "invalid_argument",
            "Reserved Windows filename",
        ));
    }
    Ok(())
}
pub(crate) fn export(event: &Event, destination: &Path) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    validate_name(&event.name)?;
    let expected = event
        .revision
        .strip_prefix("ref:")
        .or_else(|| event.revision.strip_prefix("blob:"))
        .ok_or_else(|| {
            ApiError::new("source_changed", "Object has no recorded content identity")
        })?;
    let mut source = fs::File::open(&event.path)?;
    let stem = Path::new(&event.name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("object");
    let ext = Path::new(&event.name).extension().and_then(|s| s.to_str());
    for n in 0..10000 {
        let name = if n == 0 {
            event.name.clone()
        } else {
            format!(
                "{stem} ({n}){}",
                ext.map(|s| format!(".{s}")).unwrap_or_default()
            )
        };
        let path = destination.join(name);
        let mut output = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        };
        let result = (|| -> Result<()> {
            let mut digest = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                let n = source.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                output.write_all(&buffer[..n])?;
                digest.update(&buffer[..n]);
            }
            if format!("{:x}", digest.finalize()) != expected {
                return Err(ApiError::new(
                    "source_changed",
                    "Source contents changed since this arrival",
                ));
            }
            output.sync_all()?;
            Ok(())
        })();
        drop(output);
        if let Err(e) = result {
            let _ = fs::remove_file(&path);
            return Err(e);
        }
        return Ok(path);
    }
    Err(ApiError::new("name_conflict", "No free destination name"))
}

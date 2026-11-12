use rusqlite::{Connection, OptionalExtension, params};
pub mod capture;
pub mod capture_events;
mod catalogue_query;
mod catalogue_schema;
mod classification_runtime;
pub mod clipboard_files;
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
pub mod everything;
pub mod file_types;
pub mod file_visual;
mod files_pointer;
mod files_service;
mod files_store;
mod folder_commit;
mod folder_delete;
mod folder_import;
pub mod folder_launch;
pub mod folder_transfer;
pub mod folder_watch;
pub mod interaction;
mod jobs_service;
pub mod link_fragments;
pub mod markdown;
pub mod media_runtime;
pub mod open_file;
pub mod origin;
mod paste_service;
pub mod pinned;
mod pinned_open;
pub mod safe_recycle;
pub mod screenshot_identity;
pub mod settings;
pub mod sharex;
mod shelf_tabs;
pub mod shell_visual_worker;
pub mod shortcuts;
pub mod sorting;
pub mod source_policy;
pub mod source_runtime;
mod source_service;
pub mod tab_layout;
pub mod tab_service;
pub mod table_clipboard;
pub mod table_fragments;
mod table_service;
mod tag_service;
pub mod tag_slots;
pub mod text_fragments;
mod text_service;
mod transfer_service;
pub mod work_folder;
mod workspace_service;
use interaction::Interaction;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub mod actions;
pub mod api;
pub mod app_handlers;
mod app_picker;
pub mod preview;
pub mod preview_snapshot;
pub mod preview_view;
pub mod quicklook;
pub mod quicklook_backend;
pub mod quicklook_process;
pub mod quicklook_transport;
pub mod storage;
pub mod table_view;
pub mod updater;
pub use api::*;
pub mod emoji_fonts;
pub mod emoji_render;
pub mod overlay;
mod overlay_service;
pub mod overlay_windows;
mod pod_metadata;
pub mod pods;
pub mod theme;
pub mod theme_backgrounds;
mod theme_service;
pub mod tray;
pub mod video_backgrounds;

/// The only action dispatcher. Transports and future UI call this contract.
pub struct Service {
    window_visible: bool,
    overlay_session: overlay::Session,
    overlay_host: Box<dyn overlay::Host + Send>,
    overlay_copy: Option<(domain_jobs::JobId, u64)>,
    clipboard_files: Box<dyn clipboard_files::Source>,
    transfer_sequence: Option<u32>,
    transfer_intent: Option<(Vec<SelectionRef>, bool)>,
    work_folder: work_folder::State,
    updater: updater::Updater,
    checked_update: Option<updater::Release>,
    update_status: serde_json::Value,
    app_provider: std::sync::Arc<std::sync::Mutex<Box<dyn app_handlers::Provider>>>,
    app_sessions: app_picker::Sessions,
    table_clipboard: std::sync::Arc<std::sync::Mutex<Box<dyn table_clipboard::Sink>>>,
    text_clipboard: std::sync::Arc<std::sync::Mutex<Box<dyn text_fragments::ClipboardSink>>>,
    trash: std::sync::Arc<std::sync::Mutex<Box<dyn downloads::Trash>>>,
    jobs: domain_jobs::DomainJobs<jobs_service::PreparedWork>,
    classification: classification_runtime::ClassificationRuntime,
    source_runtime: source_runtime::SourceRuntime,
    source_configured: bool,
    source_runtime_started: bool,
    external_preview: std::sync::Arc<std::sync::Mutex<Box<dyn quicklook::Launcher>>>,
    file_launcher: std::sync::Arc<std::sync::Mutex<Box<dyn open_file::Launcher>>>,
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
    owned_table_tokens: std::collections::VecDeque<String>,
    interaction: Interaction,
    settings: settings::Settings,
    shortcuts: shortcuts::Shortcuts,
    ingest_origin: origin::Origin,
    read_only: bool,
}
impl Service {
    pub fn set_updater(&mut self, updater: updater::Updater) {
        self.updater = updater;
        self.checked_update = None;
        self.update_status =
            serde_json::json!({"state":"not_checked","current_version":env!("CARGO_PKG_VERSION")});
    }
    fn accept_update_check(&mut self, status: updater::UpdateStatus) -> Result<serde_json::Value> {
        self.checked_update = status.release.clone();
        self.update_status =
            serde_json::to_value(status).map_err(|error| ApiError::new("update_error", error))?;
        Ok(self.update_status.clone())
    }
    fn accept_update_install(
        &mut self,
        installed: updater::InstalledUpdate,
    ) -> Result<serde_json::Value> {
        self.checked_update = None;
        self.update_status = serde_json::json!({"state":"installed","current_version":env!("CARGO_PKG_VERSION"),"installed":installed});
        Ok(self.update_status.clone())
    }
    pub fn set_table_clipboard_sink(&mut self, sink: Box<dyn table_clipboard::Sink>) {
        self.table_clipboard = std::sync::Arc::new(std::sync::Mutex::new(sink));
    }
    pub fn set_text_clipboard_sink(&mut self, sink: Box<dyn text_fragments::ClipboardSink>) {
        self.text_clipboard = std::sync::Arc::new(std::sync::Mutex::new(sink));
    }
    pub fn set_app_provider(&mut self, provider: Box<dyn app_handlers::Provider>) {
        self.app_provider = std::sync::Arc::new(std::sync::Mutex::new(provider));
        self.app_sessions = Default::default();
    }
    /// Injectable external-preview boundary for synthetic tests and embedded clients.
    pub fn set_preview_launcher(&mut self, launcher: Box<dyn quicklook::Launcher>) {
        self.external_preview = std::sync::Arc::new(std::sync::Mutex::new(launcher));
    }
    pub fn set_file_launcher(&mut self, launcher: Box<dyn open_file::Launcher>) {
        self.file_launcher = std::sync::Arc::new(std::sync::Mutex::new(launcher));
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
        interaction.sort = settings.sort.normalized();
        if !settings.tab_enabled(interaction.tab) {
            interaction.tab = interaction::ShelfTab::All;
        }
        interaction.last_tab = settings
            .visible_tabs()
            .first()
            .copied()
            .unwrap_or(interaction::ShelfTab::All);
        let shortcuts =
            shortcuts::Shortcuts::load(&root).map_err(|e| ApiError::new(&e.code, e.message))?;
        Ok(Self {
            window_visible: true,
            overlay_session: Default::default(),
            overlay_host: Box::new(overlay_windows::WindowsHost),
            overlay_copy: None,
            clipboard_files: Box::new(clipboard_files::Native),
            transfer_sequence: None,
            transfer_intent: None,
            work_folder: Default::default(),
            updater: updater::Updater::native(env!("CARGO_PKG_VERSION"))?,
            checked_update: None,
            update_status: serde_json::json!({"state":"not_checked","current_version":env!("CARGO_PKG_VERSION")}),
            app_provider: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                app_handlers::NativeProvider,
            ))),
            app_sessions: Default::default(),
            table_clipboard: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                table_clipboard::NativeClipboard,
            ))),
            text_clipboard: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                text_fragments::NativeClipboard,
            ))),
            trash: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                downloads::RecycleOnlyTrash,
            ))),
            jobs: Default::default(),
            classification: Default::default(),
            source_runtime: Default::default(),
            source_configured: false,
            source_runtime_started: false,
            external_preview: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                quicklook::NativeLauncher,
            ))),
            file_launcher: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                open_file::NativeLauncher,
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
            owned_table_tokens: Default::default(),
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
        pods::register_sql(&db)?;
        Ok(Self {
            window_visible: true,
            overlay_session: Default::default(),
            overlay_host: Box::new(overlay_windows::WindowsHost),
            overlay_copy: None,
            clipboard_files: Box::new(clipboard_files::Native),
            transfer_sequence: None,
            transfer_intent: None,
            work_folder: Default::default(),
            updater: updater::Updater::native(env!("CARGO_PKG_VERSION"))?,
            checked_update: None,
            update_status: serde_json::json!({"state":"not_checked","current_version":env!("CARGO_PKG_VERSION")}),
            app_provider: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                app_handlers::NativeProvider,
            ))),
            app_sessions: Default::default(),
            table_clipboard: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                table_clipboard::NativeClipboard,
            ))),
            text_clipboard: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                text_fragments::NativeClipboard,
            ))),
            trash: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                downloads::RecycleOnlyTrash,
            ))),
            jobs: Default::default(),
            classification: Default::default(),
            source_runtime: Default::default(),
            source_configured: false,
            source_runtime_started: false,
            external_preview: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                quicklook::NativeLauncher,
            ))),
            file_launcher: std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                open_file::NativeLauncher,
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
            owned_table_tokens: Default::default(),
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
                Command::GetUpdateStatus
                    | Command::Get { .. }
                    | Command::PreviewImage { .. }
                    | Command::PreviewFile { .. }
                    | Command::GetText { .. }
                    | Command::GetLink { .. }
                    | Command::GetTable { .. }
                    | Command::GetDocumentText { .. }
                    | Command::PreviewPath { .. }
                    | Command::ReadDocumentPath { .. }
            )
        {
            return Err(ApiError::new(
                "read_only",
                "Preview client cannot mutate state",
            ));
        }
        match command {
            Command::MediaRuntimeStatus => Ok(media_runtime::status()),
            Command::EnsureMediaRuntime => media_runtime::install().map_err(|e|ApiError::new("media_runtime_error",e)),
            Command::RestartApplication => Ok(serde_json::json!({"restart_requested":true})),
            Command::VideoBackgrounds => video_backgrounds::catalogue(&self.root).map(|items|serde_json::json!({"items":items})).map_err(|e|ApiError::new("video_background_error",e)),
            Command::GetVideoBackground {id} => video_backgrounds::get(&self.root,&id).map_err(|e|ApiError::new("video_background_error",e)),
            Command::ImportVideoBackground {path} => {
                let prepared=video_backgrounds::prepare(&path).map_err(|e|ApiError::new("video_background_error",e))?;
                video_backgrounds::commit(&self.root,prepared).map_err(|e|ApiError::new("video_background_error",e))
            },
            Command::UseSelectedVideoAsBackground => {
                let event=self.selected_background_video()?;
                let prepared=video_backgrounds::prepare_event(&event).map_err(|e|ApiError::new("video_background_error",e))?;
                self.validate_job_events(&[event])?;
                let result=video_backgrounds::commit(&self.root,prepared).map_err(|e|ApiError::new("video_background_error",e))?;
                self.activate_video_background(result)
            },
            Command::UseSelectedImageAsBackground => {
                let event=self.selected_background_image()?;
                let prepared=theme_backgrounds::prepare_event(&event)?;
                let result=theme_backgrounds::commit(&self.root,prepared)?;
                self.activate_background(result)
            },
            Command::ImportThemeBackground {path} => theme_backgrounds::import(&self.root,&path),
            Command::ListThemeBackgrounds => theme_backgrounds::list(&self.root),
            Command::GetThemeBackground {id} => theme_backgrounds::get(&self.root,&id),
            Command::Themes => Ok(theme::catalogue(&self.settings.theme)),
            Command::SetTheme {theme} => {let mut settings=self.settings.clone();settings.theme=theme;self.dispatch(Command::SetSettings{settings})?;Ok(theme::catalogue(&self.settings.theme))},
            Command::WindowVisibility => Ok(serde_json::json!({"visible":self.window_visible})),
            Command::SetWindowVisibility { visible } => {self.window_visible=visible;if !visible {self.interaction.settings_open=false;}Ok(serde_json::json!({"visible":visible}))},
            Command::ToggleWindowVisibility => {self.window_visible = !self.window_visible;if !self.window_visible {self.interaction.settings_open=false;}Ok(serde_json::json!({"visible":self.window_visible}))},
            Command::ToggleResidentOverlay { owner_window,capture } => self.toggle_resident_overlay(owner_window,capture),
            Command::DescribePod { id } => self.describe_pod(id),
            Command::SummonCapturedOverlay { owner_window, capture } => self.summon_captured_overlay(owner_window,capture),
            Command::SummonOverlay { owner_window } => self.summon_overlay(owner_window),
            Command::BeginPastePrevious { owner_window } => self.begin_paste_previous(owner_window),
            Command::CompletePastePrevious { owner_window } => self.complete_paste_previous(owner_window),
            Command::PreviewPath {path,revision,max_edge,image_only} => preview_snapshot::preview_file(&path,&revision,max_edge,image_only,&self.root),
            Command::ReadDocumentPath {path,revision} => preview_snapshot::read_document(&path,&revision),
            Command::StageTransfer {items,cut} => self.stage_transfer(items,cut),
            Command::TransferSelection => self.transfer_selection(),
            Command::CopySelection => self.copy_selection(),
            Command::PasteToFolder => self.paste_to_folder(),
            Command::PasteFilesToFolder {paths} => self.paste_files_to_folder(paths),
            Command::PasteClipboardToFolder => self.paste_clipboard_to_folder(),
            Command::AddFolderFiles {ids} => self.import_from_folder(ids),
            Command::AddFiles {paths} => self.add_files(paths),
            Command::ChooseFolder { path } => self.choose_folder(path),
            Command::CurrentFolder => Ok(serde_json::json!({"path":self.work_folder.current()})),
            Command::SearchFolders {query,limit} => everything::search_folders(&query,limit as usize).map(|v|serde_json::json!(v)).map_err(|e|ApiError::new(&e.code,e.message)),
            Command::FolderHistory {query} => self.folder_entries(false,&query),
            Command::FolderRecovery => {let path=self.work_folder.current().ok_or_else(||ApiError::new("folder_not_selected","Choose a folder first"))?;safe_recycle::list_recovery(path)},
            Command::FavoriteFolders {query} => self.folder_entries(true,&query),
            Command::ToggleFavoriteFolder => {
                let path=self.work_folder.current().ok_or_else(||ApiError::new("no_folder","Choose a working folder first"))?;
                Ok(serde_json::json!({"favorite":pinned::toggle_favorite(&self.db,path)?}))
            },
            Command::ClearFolderHistory {confirmed} => {
                if !confirmed {Ok(serde_json::json!({"confirmation_required":true}))} else {pinned::clear_folder_history(&self.db)?;Ok(serde_json::json!({"cleared":true}))}
            },
            Command::TogglePins {ids} => self.toggle_pins(ids),
            Command::GetPinnedProperties {id} => Ok(serde_json::json!({"properties":pinned::properties(&self.db,id)?,"tags":pinned::tags(&self.db,id)?})),
            Command::SetPinnedProperties {id,display_name,opener} => {self.validate_pinned_opener(id,&opener)?;let value=pinned::set_properties(&self.db,id,display_name,opener)?;self.interaction.view_generation=self.interaction.view_generation.wrapping_add(1);Ok(serde_json::json!(value))},
            Command::SetPinnedTags {id,tags} => {let value=pinned::set_tags(&self.db,id,&tags)?;self.interaction.view_generation=self.interaction.view_generation.wrapping_add(1);Ok(serde_json::json!({"tags":value}))},
            Command::PinnedTags => self.pinned_tags(),
            Command::FilterPinnedTag {tag} => {
                if self.interaction.tab==interaction::ShelfTab::All {return Err(ApiError::new("folder_has_no_tags","Folder has no tag filters"));}
                let tag=tag.map(|t|t.trim().to_lowercase()).filter(|t|!t.is_empty());
                let next=if self.interaction.tag_filter==tag {None}else{tag};
                self.interaction.reset_context();self.interaction.pinned_only=true;self.interaction.tag_filter=next;
                Ok(serde_json::json!(self.interaction))
            },
            Command::ListTagSlots => self.list_tag_slots(),
            Command::SaveTagSlot {slot} => self.save_tag_slot(slot),
            Command::AssignTagSlot {ids,slot} => self.assign_tag_slot(ids,slot),
            Command::GetTagSlot {id} => self.get_tag_slot(id),
            Command::GetTagSlots {ids} => self.get_tag_slots(ids),
            Command::FilterTagSlot {slot} => self.filter_tag_slot(slot),
            Command::GetUpdateStatus => Ok(self.update_status.clone()),
            Command::CheckUpdates => {let status=self.updater.check()?;self.accept_update_check(status)},
            Command::InstallUpdate => {
                let release=self.checked_update.clone().ok_or_else(||ApiError::new("update_not_checked","Check for an available update first"))?;
                let prepared=self.updater.prepare(&release)?;
                let installed=self.updater.commit(prepared)?;
                self.accept_update_install(installed)
            },
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
                if self.interaction.tab==interaction::ShelfTab::Pods {
                    settings.pods_tile_size=(i32::from(settings.pods_tile_size)-i32::from(delta)*12).clamp(48,192) as u16;
                } else {match self.interaction.view {
                    interaction::View::Gallery => settings.gallery_columns=density::adjust_density(settings.gallery_columns,delta),
                    interaction::View::List => settings.stream_rows=density::adjust_density(settings.stream_rows,delta),
                }}
                self.dispatch(Command::SetSettings{settings})
            },
            Command::CommitFolderCandidates { .. } => Err(ApiError::new("untrusted_worker_result","Prepared source results are accepted only from the domain supervisor")),
            Command::ScreenshotMatchInputs => self.screenshot_match_inputs(),
            Command::CommitScreenshotMatches { .. } => Err(ApiError::new("untrusted_worker_result","Pixel identity must be verified by the domain worker")),
            Command::IngestText {text,source} => {
                if pods::is_pod(&text) {self.check_pods_enabled()?;} else {self.check_text_enabled()?;}
                validate_source(&source)?;
                let prepared=text_fragments::prepare(&self.root,self.quota,&text,&source,&self.ingest_origin,&self.reference_paths()?)?;
                let id=storage::commit_prepared_bytes(&mut self.db,&self.root,self.quota,prepared)?;
                Ok(serde_json::json!(self.get(id)?))
            }
            Command::IngestTable { rows,source } => self.ingest_table(table_fragments::Table{rows},&source),
            Command::IngestTableTsv { text,source } => {self.check_tables_enabled()?;let table=table_fragments::parse_tsv(&text,true).map_err(table_service::invalid)?;self.ingest_table(table,&source)},
            Command::IngestTableHtml { html,source } => {self.check_tables_enabled()?;let table=table_fragments::parse_html(&html).map_err(table_service::invalid)?;self.ingest_table(table,&source)},
            Command::GetTable {id} => self.get_table(id),
            Command::CopyTables {items} => self.copy_tables(items),
            Command::GetLink {id} => self.get_link(id),
            Command::CopyLinks {items} => self.copy_links(&items),
            Command::IngestLink {url,source} => { self.check_links_enabled()?; validate_source(&source)?; let prepared=link_fragments::prepare(&self.root,self.quota,&url,&source,&self.ingest_origin,&self.reference_paths()?)?; let id=storage::commit_prepared_bytes(&mut self.db,&self.root,self.quota,prepared)?; Ok(serde_json::json!(self.get(id)?)) },
            Command::GetText {id}=>self.get_text(id),
            Command::GetDocumentText {id}=>self.get_document_text(id),
            Command::MergeText {items,separator}=>self.merge_text(&items,&separator),
            Command::CopyText {items,separator}=>self.copy_text(&items,&separator),
            Command::CopySelectedObjects=>{let items=self.selected_refs()?;self.dispatch(Command::CopySmart{items})},
            Command::CopySmart{items}=>{if self.selection_is_tables(&items)?{self.copy_tables(items)}else if self.selection_is_links(&items)?{self.copy_links(&items)}else if self.selection_is_text(&items)?{self.copy_text(&items,if self.selection_is_pods(&items)? {""} else {"\n\n"})}else{self.dispatch(Command::CopyEventsChecked{items})}},
            Command::CopySelectedText=>{let items=self.selected_refs()?;self.copy_text(&items,"\n\n")},
            Command::ListOpenApps {id} => self.list_open_apps(id),
            Command::OpenWithApp {token,app_id} => self.open_with_app(&token,app_id),
            Command::ReleaseOpenApps {token} => Ok(serde_json::json!({"released":self.app_sessions.release(&token)})),
            Command::Tabs => Ok(self.tabs()),
            Command::SetTabEnabled {tab, enabled} => self.set_tab_enabled(tab, enabled),
            Command::ToggleTabEnabled {tab} => self.set_tab_enabled(tab, !self.settings.tab_enabled(tab)),
            Command::ShowTabSettings => {
                self.interaction.settings_section=2;
                self.interaction.settings_open=true;
                Ok(serde_json::json!(self.interaction))
            },
            Command::OpenFile { id, operation } => {
                let event=self.get(id)?;
                if id>0 && operation==open_file::Operation::Open && self.pinned_app_launcher(&event)?.is_some() {return self.open_pinned_default(&event);}
                let launcher=self.file_launcher.lock().map_err(|_|ApiError::new("open_busy","File launcher lock poisoned"))?;
                if event.kind==EventKind::LinkFragment && operation==open_file::Operation::Open { open_file::execute_link(&event,launcher.as_ref()) } else { open_file::execute_with_cache(&event,operation,launcher.as_ref(),&self.root,self.quota) }
            },
            Command::SelectTabPosition { position } => {
                let tab = tab_layout::tab_at_visible(&self.settings.visible_tabs(),position)?;
                self.interaction.select_position(tab);
                Ok(serde_json::json!(self.interaction))
            },
            Command::TogglePinnedView => {
                if self.interaction.tab == interaction::ShelfTab::All {
                    return Err(ApiError::new("folder_has_no_pins", "Folder has no pinned view"));
                }
                self.interaction.select_position(self.interaction.tab);
                Ok(serde_json::json!(self.interaction))
            },
            Command::NavigateTab { delta } => self.navigate_tab(delta),
            Command::MoveTab { tab,index } => {
                let mut settings=self.settings.clone();
                let visible=settings.visible_tabs();
                let target=visible.get(index).ok_or_else(||ApiError::new("invalid_tab_position","No visible tab at this position"))?;
                if !settings.tab_enabled(tab) {return Err(ApiError::new("tab_disabled","Cannot drag a disabled tab"));}
                let target_index=settings.tab_order.iter().position(|t|t==target).unwrap();
                tab_layout::reorder(&mut settings.tab_order,tab,target_index).map_err(|e|ApiError::new("invalid_tab_order",e))?;
                self.dispatch(Command::SetSettings { settings })
            },
            Command::SetTab { tab } => self.select_tab(tab),
            Command::ToggleTab => { self.normalize_tabs(); self.interaction.toggle_tab(); Ok(serde_json::json!(self.interaction)) },
            Command::BrowseShelf { query, limit, after_id } => self.browse_shelf(&query, limit, after_id),
            Command::BrowseShelfSorted { query, limit, cursor } => self.browse_shelf_sorted(&query, limit, cursor.as_deref()),
            Command::SetSort { mode, reverse, seed } => {
                let mut settings = self.settings.clone();
                settings.sort = sorting::SortSpec { mode, reverse, seed: seed.unwrap_or_else(|| if mode == sorting::SortMode::Shuffle { self.interaction.sort.seed.wrapping_add(now_ms() as u64 | 1) } else { self.interaction.sort.seed }) }.normalized();
                self.dispatch(Command::SetSettings { settings })?;
                Ok(serde_json::json!(self.interaction))
            },
            Command::ScanSharex => self.dispatch(Command::RescanSource{source_id:source_policy::SourceId::Sharex}),
            Command::ScanDownloads => self.dispatch(Command::RescanSource{source_id:source_policy::SourceId::Downloads}),
            Command::CaptureClipboard { trigger } => self.capture_from_event(trigger),
            Command::ActivateWindow => {
                self.interaction.activation_serial = self.interaction.activation_serial.wrapping_add(1);
                Ok(serde_json::json!({"activation_requested": true, "serial": self.interaction.activation_serial}))
            }
            Command::TableViewGeometry { viewport, content } => {
                self.require_table_view()?;
                self.interaction.table_view.geometry(viewport,content)
            }
            Command::ZoomTableView { multiplier, anchor, viewport, content } => {
                self.require_table_view()?;
                self.interaction.table_view.zoom(multiplier,anchor,viewport,content)?;
                Ok(serde_json::json!(self.interaction))
            }
            Command::PanTableView { delta, viewport, content } => {
                self.require_table_view()?;
                self.interaction.table_view.pan(delta,viewport,content)?;
                Ok(serde_json::json!(self.interaction))
            }
            Command::FitTableView { viewport, content } => {
                self.require_table_view()?;
                self.interaction.table_view.fit(viewport,content)?;
                Ok(serde_json::json!(self.interaction))
            }
            Command::ResetTableView => {
                self.require_table_view()?;
                self.interaction.table_view=Default::default();
                Ok(serde_json::json!(self.interaction))
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
                    if !file_types::has_internal_preview(&event) { return self.preview_external(event.id); }
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
                if receipt.failures.is_empty() {self.record_selection_publication(receipt.sequence,&events.iter().map(|event|event.id).collect::<Vec<_>>());}
                else {self.record_clipboard_publication(receipt.sequence);}
                Ok(
                    serde_json::json!({"copied":receipt.count,"failures":receipt.failures,"warnings":receipt.warnings}),
                )
            }
            Command::IngestWithOrigin { origin, command } => {
                if !matches!(
                    *command,
                    Command::IngestPath { .. } | Command::IngestBytes { .. } | Command::IngestText {..} | Command::IngestTable {..} | Command::IngestTableTsv {..} | Command::IngestTableHtml {..}
                ) {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Expected ingest_path or ingest_bytes",
                    ));
                }
                let clipboard_source = match command.as_ref() {
                    Command::IngestPath { source, .. } | Command::IngestBytes { source, .. } | Command::IngestText {source,..} | Command::IngestTable {source,..} | Command::IngestTableTsv {source,..} | Command::IngestTableHtml {source,..} => source.starts_with("clipboard_"),
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
            Command::DeleteSelection { ids, confirmed } => if ids.iter().any(|id|*id<0) {self.delete_folder_selection(ids,confirmed)}else{self.delete_selection(ids, confirmed)},
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
            Command::EmojiFonts => emoji_fonts::catalogue(&self.root, &self.settings.emoji_font),
            Command::ResolveEmojiFont { id } => emoji_fonts::resolve(&self.root, &id),
            Command::ImportEmojiFont { path } => emoji_fonts::commit(&self.root, emoji_fonts::prepare(&path)?),
            Command::GetSettings => Ok(serde_json::json!(self.settings)),
            Command::SetSettings { settings } => {
                if settings.emoji_font != "apple" && settings.emoji_font != "system" {
                    let font = emoji_fonts::resolve(&self.root, &settings.emoji_font)?;
                    if font["available"] != true { return Err(ApiError::new("invalid_font", "Шрифт эмодзи недоступен")); }
                }
                if let Some(id)=&settings.theme.custom_background {theme_backgrounds::get(&self.root,id)?;}
                if let Some(id)=&settings.theme.video_background {
                    if !video_backgrounds::valid_id(id) {return Err(ApiError::new("invalid_video","Invalid video background ID"));}
                    if settings.theme.video_is_enabled() {video_backgrounds::get(&self.root,id).map_err(|e|ApiError::new("video_background_error",e))?;}
                }
                settings
                    .validate(self.usage()?)
                    .map_err(|e| ApiError::new("invalid_settings", e))?;
                let document = serde_json::to_string(&settings)
                    .map_err(|e| ApiError::new("settings_error", e))?;
                self.db.execute("INSERT INTO preferences(key,document) VALUES('general',?1) ON CONFLICT(key) DO UPDATE SET document=excluded.document",[document])?;
                if self.settings.sharex_folder != settings.sharex_folder { self.sharex_scanner = Default::default(); }
                if self.settings.downloads_folder != settings.downloads_folder || self.settings.downloads_enabled != settings.downloads_enabled { self.downloads_scanner = Default::default(); }
                let sources_changed=self.settings.sharex_folder!=settings.sharex_folder || self.settings.downloads_enabled!=settings.downloads_enabled || self.settings.downloads_folder!=settings.downloads_folder || self.settings.hidden_tabs!=settings.hidden_tabs;
                if self.settings.tab_enabled(interaction::ShelfTab::Images)!=settings.tab_enabled(interaction::ShelfTab::Images) || self.settings.tab_enabled(interaction::ShelfTab::Text)!=settings.tab_enabled(interaction::ShelfTab::Text) || self.settings.tab_enabled(interaction::ShelfTab::Links)!=settings.tab_enabled(interaction::ShelfTab::Links) || self.settings.tab_enabled(interaction::ShelfTab::Tables)!=settings.tab_enabled(interaction::ShelfTab::Tables) || self.settings.tab_enabled(interaction::ShelfTab::Pods)!=settings.tab_enabled(interaction::ShelfTab::Pods) || self.settings.tab_enabled(interaction::ShelfTab::Files)!=settings.tab_enabled(interaction::ShelfTab::Files) {self.capture_events.cancel();}
                self.quota = settings.quota_bytes;
                self.settings = settings;
                self.interaction.sort = self.settings.sort.normalized();
                self.normalize_tabs();
                if sources_changed {self.source_configured = false;}
                self.configure_sources(self.source_runtime_started)?;
                Ok(serde_json::json!(self.settings))
            }
            Command::ShowSettings => {
                self.interaction.settings_section = 0;
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
            Command::ClearSelection => {
                self.interaction.clear_selection();
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
                    if !file_types::has_internal_preview(&event) { return self.preview_external(event.id); }
                }
                self.interaction.set_preview(self.interaction.cursor);
                Ok(serde_json::json!(self.interaction))
            }
            Command::ClosePreview => {
                self.interaction.set_preview(None);
                Ok(serde_json::json!(self.interaction))
            }
            Command::Actions => Ok(serde_json::Value::Array(actions::DEFS.iter().filter(|d|d.in_palette && self.action_available(d.id)).map(|d|serde_json::json!({"id":d.id,"label":self.action_label(d.id,d.label),"group":d.group,"shortcut":self.shortcuts.binding(d.id)})).collect())),
            Command::RunAction{id}=>match actions::resolve(&id,&self.interaction,self.capture_enabled) {Some(actions::Dispatch::Command(command))=>self.dispatch(*command),Some(actions::Dispatch::Intent(value))=>Ok(value),None=>Err(ApiError::new("unknown_action","Unknown action"))},
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
                self.check_ingest_enabled(&path.to_string_lossy())?;
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
            Command::IngestBytes{name,bytes,source}=>{self.check_ingest_enabled(&name)?;validate_name(&name)?;validate_source(&source)?;let id=storage::ingest_bytes(&mut self.db,&self.root,self.quota,storage::ByteIngest{name:&name,source:&source,bytes:&bytes,origin:&self.ingest_origin})?;Ok(serde_json::json!(self.get(id)?))},
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
        if hash.is_none() {
            let modified = fs::metadata(path)?
                .modified()
                .ok()
                .and_then(sorting::modified_ms);
            self.db.execute(
                "UPDATE events SET modified_at_ms=?1 WHERE id=?2",
                params![modified, id],
            )?;
        }
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
        if id < 0 {
            self.work_folder.get(id)
        } else {
            storage::get_event(&self.db, id)
        }
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

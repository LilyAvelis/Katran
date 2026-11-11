//! Versioned wire contract. Keep serde names stable.
use crate::{
    interaction::{SelectionMode, View},
    origin, settings,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub const API_VERSION: u32 = 1;
pub const DEFAULT_QUOTA: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub api_version: u32,
    pub request_id: String,
    pub command: Command,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SummonOverlay { owner_window: u64 },
    BeginPastePrevious { owner_window: u64 },
    CompletePastePrevious { owner_window: u64 },
    PreviewPath {path: PathBuf, revision: String, max_edge: u32, #[serde(default)] image_only: bool},
    ReadDocumentPath {path: PathBuf, revision: String},
    StageTransfer {items: Vec<SelectionRef>, #[serde(default)] cut: bool},
    TransferSelection,
    PasteToFolder,
    PasteFilesToFolder {paths: Vec<PathBuf>},
    PasteClipboardToFolder,
    AddFolderFiles {ids: Vec<i64>},
    ChooseFolder { path: PathBuf },
    CurrentFolder,
    SearchFolders { query: String, limit: u32 },
    FolderHistory { query: String },
    FavoriteFolders { query: String },
    ToggleFavoriteFolder,
    ClearFolderHistory { #[serde(default)] confirmed: bool },
    FolderRecovery,
    TogglePins { ids: Vec<i64> },
    GetPinnedProperties { id: i64 },
    SetPinnedProperties { id: i64, display_name: Option<String>, opener: Option<PathBuf> },
    SetPinnedTags { id: i64, tags: Vec<String> },
    PinnedTags,
    FilterPinnedTag { tag: Option<String> },
    GetUpdateStatus,
    CheckUpdates,
    InstallUpdate,
    TableViewGeometry {
        viewport: crate::preview_view::Size,
        content: crate::preview_view::Size,
    },
    ZoomTableView {
        multiplier: f32,
        anchor: crate::preview_view::Point,
        viewport: crate::preview_view::Size,
        content: crate::preview_view::Size,
    },
    PanTableView {
        delta: crate::preview_view::Point,
        viewport: crate::preview_view::Size,
        content: crate::preview_view::Size,
    },
    FitTableView {
        viewport: crate::preview_view::Size,
        content: crate::preview_view::Size,
    },
    ResetTableView,
    IngestTable {
        rows: Vec<Vec<String>>,
        source: String,
    },
    IngestTableTsv {
        text: String,
        source: String,
    },
    IngestTableHtml {
        html: String,
        source: String,
    },
    GetTable {
        id: i64,
    },
    CopyTables {
        items: Vec<SelectionRef>,
    },
    IngestLink {
        url: String,
        source: String,
    },
    GetLink {
        id: i64,
    },
    CopyLinks {
        items: Vec<SelectionRef>,
    },
    IngestText {
        text: String,
        source: String,
    },
    GetText {
        id: i64,
    },
    GetDocumentText {
        id: i64,
    },
    MergeText {
        items: Vec<SelectionRef>,
        separator: String,
    },
    CopyText {
        items: Vec<SelectionRef>,
        separator: String,
    },
    CopySelectedText,
    CopySmart {
        items: Vec<SelectionRef>,
    },
    CopySelectedObjects,
    ListOpenApps {
        id: i64,
    },
    OpenWithApp {
        token: String,
        app_id: Option<String>,
    },
    ReleaseOpenApps {
        token: String,
    },
    Tabs,
    SetTabEnabled {
        tab: crate::interaction::ShelfTab,
        enabled: bool,
    },
    ToggleTabEnabled {
        tab: crate::interaction::ShelfTab,
    },
    ShowTabSettings,
    OpenFile {
        id: i64,
        operation: crate::open_file::Operation,
    },
    BeginJob {
        command: Box<Command>,
    },
    PollJob {
        job_id: crate::domain_jobs::JobId,
    },
    CancelJob {
        job_id: crate::domain_jobs::JobId,
    },
    EnsurePreviewBackend,
    PreviewBackendStatus,
    EnsureSources,
    PollSources {
        limit: u8,
    },
    SourceStatus,
    RescanSource {
        source_id: crate::source_policy::SourceId,
    },
    AdjustDensity {
        delta: i8,
    },
    PreviewExternal {
        id: i64,
    },
    SelectTabPosition {
        position: u8,
    },
    NavigateTab {
        delta: i8,
    },
    MoveTab {
        tab: crate::interaction::ShelfTab,
        index: usize,
    },
    CommitFolderCandidates {
        items: Vec<crate::folder_watch::PreparedCandidate>,
    },
    ScreenshotMatchInputs,
    CommitScreenshotMatches {
        pairs: Vec<crate::screenshot_identity::MatchPair>,
    },
    DeleteSelection {
        ids: Vec<i64>,
        #[serde(default)]
        confirmed: bool,
    },
    SetTab {
        tab: crate::interaction::ShelfTab,
    },
    ToggleTab,
    BrowseShelf {
        query: String,
        limit: u32,
        after_id: Option<i64>,
    },
    BrowseShelfSorted {
        query: String,
        limit: u32,
        cursor: Option<String>,
    },
    SetSort {
        mode: crate::sorting::SortMode,
        #[serde(default)]
        reverse: bool,
        seed: Option<u64>,
    },
    ScanSharex,
    ScanDownloads,
    CaptureClipboard {
        trigger: crate::capture_events::Trigger,
    },
    ActivateWindow,
    DragEventsChecked {
        items: Vec<SelectionRef>,
    },
    RunPreparedDrag {
        token: String,
        owner_window: u64,
    },
    CancelPreparedDrag {
        token: String,
    },
    PreviewGeometry {
        viewport: crate::preview_view::Size,
        fitted: crate::preview_view::Size,
    },
    ZoomPreview {
        multiplier: f32,
        anchor: crate::preview_view::Point,
        viewport: crate::preview_view::Size,
        fitted: crate::preview_view::Size,
    },
    PanPreview {
        delta: crate::preview_view::Point,
        viewport: crate::preview_view::Size,
        fitted: crate::preview_view::Size,
    },
    TogglePreview,
    NavigatePreview {
        delta: i32,
    },
    MaintainCache,
    SearchPage {
        query: String,
        #[serde(default = "default_limit")]
        limit: u32,
        after_id: Option<i64>,
    },
    CopyEventsChecked {
        items: Vec<SelectionRef>,
    },
    CopyEvents {
        ids: Vec<i64>,
    },
    IngestWithOrigin {
        origin: origin::Origin,
        command: Box<Command>,
    },
    DeleteEvents {
        ids: Vec<i64>,
        #[serde(default)]
        confirmed: bool,
    },
    Shortcuts,
    SetShortcut {
        id: String,
        binding: Option<String>,
        #[serde(default)]
        replace: bool,
        #[serde(default)]
        reset: bool,
    },
    GetSettings,
    SetSettings {
        settings: settings::Settings,
    },
    ShowSettings,
    CloseSettings,
    Search {
        query: String,
        #[serde(default = "default_limit")]
        limit: u32,
    },
    Interaction,
    SetVisible {
        ids: Vec<i64>,
    },
    Select {
        id: i64,
        #[serde(default)]
        mode: SelectionMode,
    },
    ClearSelection,
    Navigate {
        delta: i32,
        #[serde(default)]
        mode: SelectionMode,
    },
    SetView {
        view: View,
    },
    OpenCurrent,
    ClosePreview,
    Actions,
    RunAction {
        id: String,
    },
    PreviewImage {
        id: i64,
        max_edge: u32,
    },
    PreviewFile {
        id: i64,
        max_edge: u32,
    },
    SetCaptureEnabled {
        enabled: bool,
    },
    PollClipboard,
    Status,
    List {
        #[serde(default = "default_limit")]
        limit: u32,
    },
    Get {
        id: i64,
    },
    IngestPath {
        path: PathBuf,
        source: String,
    },
    IngestBytes {
        name: String,
        bytes: Vec<u8>,
        source: String,
    },
    Import {
        ids: Vec<i64>,
        destination: PathBuf,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SelectionRef {
    pub id: i64,
    pub revision: String,
}
fn default_limit() -> u32 {
    100
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
impl ApiError {
    pub(crate) fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}
impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io_error", e)
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("store_error", e)
    }
}
pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    #[serde(default)]
    pub kind: EventKind,
    #[serde(default)]
    pub text_preview: Option<String>,
    #[serde(default)]
    pub delete_policy: crate::source_policy::DeletePolicy,
    #[serde(default)]
    pub revision: String,
    pub origin: origin::Origin,
    pub id: i64,
    pub detected_at_ms: i64,
    #[serde(default)]
    pub modified_at_ms: Option<i64>,
    pub source: String,
    pub name: String,
    pub storage: String,
    pub path: PathBuf,
    pub size: u64,
    pub available: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    #[default]
    FileReference,
    ManagedImage,
    TextFragment,
    LinkFragment,
    TableFragment,
}

//! Versioned wire contract. Keep serde names stable.
use crate::{
    interaction::{SelectionMode, View},
    origin, settings,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub const API_VERSION: u32 = 1;
pub const DEFAULT_QUOTA: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageTranslationSource {
    Japanese,
    Chinese,
    TraditionalChinese,
    Korean,
}

impl ImageTranslationSource {
    pub fn code(self) -> &'static str {
        match self {
            Self::Japanese => "ja",
            Self::Chinese => "zh",
            Self::TraditionalChinese => "zh-hant",
            Self::Korean => "ko",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpscaleOperation {
    RemacriX2,
    RemacriX4,
    RealEsrganX2,
    RealEsrganX4,
}
impl UpscaleOperation {
    pub(crate) fn factor(self) -> u32 {
        match self {
            Self::RemacriX2 | Self::RealEsrganX2 => 2,
            Self::RemacriX4 | Self::RealEsrganX4 => 4,
        }
    }

    pub(crate) fn weights(self) -> &'static str {
        match self {
            Self::RemacriX2 | Self::RemacriX4 => "4x_foolhardy_Remacri.pth",
            Self::RealEsrganX2 => "RealESRGAN_x2plus.pth",
            Self::RealEsrganX4 => "RealESRGAN_x4.pth",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub api_version: u32,
    pub request_id: String,
    pub command: Command,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NotionPageFields {
    pub title: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
// Keep Settings by value in the public command contract; boxing it would
// change every dispatcher client just to reduce this enum's stack size.
#[allow(clippy::large_enum_variant)]
pub enum Command {
    AiComponentsStatus,
    LaunchAiComponent {
        component: crate::ai_components::Component,
        action: crate::ai_components::Action,
    },
    MediaRuntimeStatus,
    EnsureMediaRuntime,
    RestartApplication,
    VideoBackgrounds,
    GetVideoBackground {
        id: String,
    },
    ImportVideoBackground {
        path: PathBuf,
    },
    UseSelectedVideoAsBackground,
    UseSelectedVideoAsDesktopWallpaper,
    UseSelectedVideoAsSilentDesktopWallpaper,
    UseSelectedImageAsBackground,
    UseSelectedImageAsDesktopWallpaper,
    ListDesktopAssets,
    DeleteDesktopAsset {
        id: String,
    },
    ApplyDesktopAsset {
        id: String,
        target: String,
    },
    ImportThemeBackground {
        path: std::path::PathBuf,
    },
    ListThemeBackgrounds,
    GetThemeBackground {
        id: String,
    },
    Themes,
    SetTheme {
        theme: crate::theme::ThemePreferences,
    },
    WindowVisibility,
    SetWindowVisibility {
        visible: bool,
    },
    ToggleWindowVisibility,
    ToggleResidentOverlay {
        owner_window: u64,
        capture: Option<crate::overlay::SummonReceipt>,
    },
    EmojiCategories,
    EmojiAiStatus,
    SearchEmojiAi {
        query: String,
        offset: u32,
        limit: u32,
    },
    SetEmojiCategory {
        group: Option<String>,
    },
    SetEmojiCatalogue {
        enabled: bool,
    },
    CopyEmojiSelectionToPods,
    DescribePod {
        id: i64,
    },
    SetTabGlobalHotkey {
        tab: crate::interaction::ShelfTab,
        binding: Option<String>,
    },
    SummonTabOverlay {
        tab: crate::interaction::ShelfTab,
        owner_window: u64,
        capture: Option<crate::overlay::SummonReceipt>,
    },
    SummonCapturedOverlay {
        owner_window: u64,
        capture: crate::overlay::SummonReceipt,
    },
    SummonOverlay {
        owner_window: u64,
    },
    BeginPastePrevious {
        owner_window: u64,
    },
    BeginPasteNotionPage {
        owner_window: u64,
        page_id: String,
        #[serde(default)]
        title: String,
    },
    BeginPasteCollectionPage {
        owner_window: u64,
        id: i64,
        revision: String,
        index: usize,
    },
    CompletePastePrevious {
        owner_window: u64,
    },
    PreviewPath {
        path: PathBuf,
        revision: String,
        max_edge: u32,
        #[serde(default)]
        image_only: bool,
    },
    ReadDocumentPath {
        path: PathBuf,
        revision: String,
    },
    StageTransfer {
        items: Vec<SelectionRef>,
        #[serde(default)]
        cut: bool,
    },
    TransferSelection,
    CopySelection,
    PasteToFolder,
    PasteFilesToFolder {
        paths: Vec<PathBuf>,
    },
    PasteClipboardToFolder,
    AddFolderFiles {
        ids: Vec<i64>,
    },
    AddFiles {
        paths: Vec<std::path::PathBuf>,
    },
    ChooseFolder {
        path: PathBuf,
    },
    CurrentFolder,
    ListKnownFolders,
    SearchFolders {
        query: String,
        limit: u32,
    },
    NotionStatus,
    GetNotionReaderPreferences,
    SetNotionReaderPreferences {
        preferences: crate::settings::NotionReaderPreferences,
    },
    GetTextReaderPreferences,
    SetTextReaderPreferences {
        preferences: crate::settings::NotionReaderPreferences,
    },
    SetNotionToken {
        token: Option<crate::notion::Token>,
    },
    SearchNotionPages {
        query: String,
    },
    SearchNotionDatabases {
        query: String,
    },
    QueryNotionDatabase {
        source_id: String,
        query: String,
        cursor: Option<String>,
        sort: Option<String>,
        descending: bool,
    },
    ReadNotionPage {
        page_id: String,
        #[serde(default)]
        title: String,
    },
    ReadNotionPageExcerpt {
        page_id: String,
    },
    CopyNotionPage {
        page_id: String,
        #[serde(default)]
        title: String,
    },
    ListSavedNotionPages {
        favorites: bool,
    },
    RecordNotionDatabaseVisit {
        source_id: String,
        title: String,
    },
    CopyNotionPageFields {
        pages: Vec<NotionPageFields>,
    },
    CopyNotionReaderSelection {
        text: String,
    },
    PrepareNotionPagesDrag {
        pages: Vec<crate::notion::Page>,
    },
    OpenNotionPage {
        page: crate::notion::Page,
    },
    ToggleFavoriteNotionPage {
        page: crate::notion::Page,
    },
    ToggleNotionDatabaseFavorite {
        source_id: String,
        title: String,
    },
    SearchFoldersSemantic {
        query: String,
        limit: u32,
    },
    SearchFoldersSemanticPage {
        query: String,
        offset: u32,
        limit: u32,
        snapshot: Option<String>,
    },
    FolderHistory {
        query: String,
    },
    FavoriteFolders {
        query: String,
    },
    ToggleFavoriteFolder,
    ToggleFavoriteFolderPath {
        path: PathBuf,
    },
    ClearFolderHistory {
        #[serde(default)]
        confirmed: bool,
    },
    FolderRecovery,
    TogglePins {
        ids: Vec<i64>,
    },
    GetPinnedProperties {
        id: i64,
    },
    SetPinnedProperties {
        id: i64,
        display_name: Option<String>,
        opener: Option<PathBuf>,
    },
    SetPinnedTags {
        id: i64,
        tags: Vec<String>,
    },
    PinnedTags,
    FilterPinnedTag {
        tag: Option<String>,
    },
    ListTagSlots,
    ListTagSlotsForTab {
        tab: crate::interaction::ShelfTab,
    },
    SaveTagSlotForTab {
        tab: crate::interaction::ShelfTab,
        slot: crate::tag_slots::Slot,
    },
    SaveTagSlotChecked {
        tab: crate::interaction::ShelfTab,
        slot: crate::tag_slots::Slot,
        expected_revision: String,
    },
    ReorderTagSlots {
        tab: crate::interaction::ShelfTab,
        order: Vec<u8>,
        expected_revision: String,
    },
    SaveTagSlot {
        slot: crate::tag_slots::Slot,
    },
    AssignTagSlot {
        ids: Vec<i64>,
        slot: Option<u8>,
    },
    GetTagSlot {
        id: i64,
    },
    GetTagSlots {
        ids: Vec<i64>,
    },
    /// Read an exact link and optionally fetch a bounded favicon. Call on a worker.
    GetLinkVisual {
        id: i64,
    },
    /// Explicitly refresh the derived favicon cache for a saved link.
    RefreshLinkVisual {
        id: i64,
    },
    /// Page through stored Link IDs for an explicit retrospective refresh.
    ListLinkIds {
        after_id: i64,
        limit: u32,
    },
    FilterTagSlot {
        slot: Option<u8>,
    },
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
    ApplyTextDraft {
        id: i64,
    },
    ShowTextVault,
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
    CopySelectionAction,
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
    ShowTagSettings,
    OpenFile {
        id: i64,
        operation: crate::open_file::Operation,
    },
    OpenMedia {
        id: i64,
        revision: String,
    },
    OpenCollectionMedia {
        id: i64,
        revision: String,
        index: usize,
    },
    ControlMedia {
        session: u64,
        action: crate::media_playback::Action,
    },
    MediaState,
    CaptureMediaFrame {
        #[serde(default)]
        session: Option<u64>,
    },
    ExtractVideoAudio {
        #[serde(default)]
        audio_track: Option<u32>,
    },
    UpscaleSelectedImage {
        operation: UpscaleOperation,
    },
    TranslateSelectedImage,
    CreateImageCollection,
    CreateBsCollection,
    SearchSelectedImageWithGoogleLens,
    TranslateSelectedImageFrom {
        source: ImageTranslationSource,
    },
    TranslationCollectionPage {
        id: i64,
        revision: String,
        index: usize,
    },
    CopyCollectionPage {
        id: i64,
        revision: String,
        index: usize,
    },
    OpenCollectionLink {
        id: i64,
        revision: String,
        index: usize,
    },
    RenameCollectionLink {
        id: i64,
        revision: String,
        index: usize,
        title: String,
    },
    CollectionRenameInfo {
        id: i64,
    },
    RenameCollection {
        id: i64,
        revision: String,
        name: String,
    },
    OpenBsEntry {
        id: i64,
        revision: String,
        index: usize,
    },
    ToggleMediaPause {
        #[serde(default)]
        session: Option<u64>,
    },
    MediaFrame {
        session: u64,
        after_serial: u64,
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
    PreviewPublishedTranslation {
        id: i64,
        revision: String,
    },
    SelectTabPosition {
        position: u8,
    },
    TogglePinnedView,
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
    /// Stateless first page for an enabled category; safe on a read-only worker.
    /// Uses that tab's saved sort and no pinned/tag filters. Folder is session-only.
    BrowseTabSnapshot {
        tab: crate::interaction::ShelfTab,
        query: String,
        limit: u32,
    },
    /// Conservative cache stamp. Compare only on the same Service connection.
    CatalogueStamp,
    SetSort {
        mode: crate::sorting::SortMode,
        #[serde(default)]
        reverse: bool,
        seed: Option<u64>,
    },
    MovePinned {
        id: i64,
        direction: crate::pinned_order::Direction,
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
    EmojiFonts,
    ResolveEmojiFont {
        id: String,
    },
    ImportEmojiFont {
        path: PathBuf,
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
    SelectAll,
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
    /// Palette rows for sorting only; does not resolve the current selection.
    SortActions,
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

#[path = "api_wire_v2.rs"]
pub mod wire_v2;

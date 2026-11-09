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
    DragEventsChecked {
        items: Vec<SelectionRef>,
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
    pub revision: String,
    pub origin: origin::Origin,
    pub id: i64,
    pub detected_at_ms: i64,
    pub source: String,
    pub name: String,
    pub storage: String,
    pub path: PathBuf,
    pub size: u64,
    pub available: bool,
}

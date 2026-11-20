//! Provenance labels are not capabilities. Only a verified live-source commit
//! can grant an original-file policy.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceId {
    Sharex,
    SnippingTool,
    Downloads,
}
impl SourceId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sharex => "sharex",
            Self::SnippingTool => "snipping_tool",
            Self::Downloads => "downloads",
        }
    }
    pub fn is_screenshot(self) -> bool {
        matches!(self, Self::Sharex | Self::SnippingTool)
    }
    pub(crate) fn classification_kind(self) -> &'static str {
        if self == Self::SnippingTool {
            "snipping_tool"
        } else {
            "source"
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Sharex => "ShareX folder",
            Self::SnippingTool => "Windows Snipping Tool",
            Self::Downloads => "Downloads",
        }
    }
}
impl From<crate::folder_watch::FolderSource> for SourceId {
    fn from(value: crate::folder_watch::FolderSource) -> Self {
        match value {
            crate::folder_watch::FolderSource::Sharex => Self::Sharex,
            crate::folder_watch::FolderSource::SnippingTool => Self::SnippingTool,
            crate::folder_watch::FolderSource::Downloads => Self::Downloads,
        }
    }
}
impl From<SourceId> for crate::folder_watch::FolderSource {
    fn from(value: SourceId) -> Self {
        match value {
            SourceId::Sharex => Self::Sharex,
            SourceId::SnippingTool => Self::SnippingTool,
            SourceId::Downloads => Self::Downloads,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceConfiguration {
    pub source_id: SourceId,
    pub generation: u64,
    pub root: PathBuf,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginalDeleteMode {
    Permanent,
    Recycle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeletePolicy {
    #[default]
    ShelfOnly,
    LiveOriginal {
        source_id: SourceId,
        generation: u64,
        mode: OriginalDeleteMode,
    },
}

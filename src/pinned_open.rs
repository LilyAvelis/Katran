//! Open a pinned object with a previously selected, opaque Shell handler ID.
//!
//! `katran_properties.opener_path` is a legacy column name: its value is a
//! provider-issued handler ID, never an executable path or shell command.
use crate::{ApiError, Event, EventKind, Result, Service, app_picker, open_file, pinned};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

impl Service {
    /// A saved ID must have been offered by the current provider for this
    /// event's extension. `None` clears the selection; it does not launch.
    pub(crate) fn validate_pinned_opener(&self, id: i64, opener: &Option<PathBuf>) -> Result<()> {
        let event = self.get(id)?;
        if !pinned::properties(&self.db, id)?.pinned {
            return Err(ApiError::new(
                "not_pinned",
                "Only pinned records have editable properties",
            ));
        }
        if let Some(opener) = opener {
            let id = opener.to_str().ok_or_else(|| {
                ApiError::new("invalid_argument", "Application ID must be Unicode")
            })?;
            self.validate_handler(&event, id)?;
        }
        Ok(())
    }

    fn validate_handler(&self, event: &Event, id: &str) -> Result<()> {
        let provider = self.app_provider.lock().map_err(|_| {
            ApiError::new("handlers_unavailable", "Application provider lock poisoned")
        })?;
        if !app_picker::discover(event, provider.as_ref())?
            .iter()
            .any(|app| app.id == id)
        {
            return Err(ApiError::new(
                "handler_unavailable",
                "Selected application is no longer offered for this object",
            ));
        }
        Ok(())
    }

    /// Snapshot a validated launcher on the dispatcher thread before a job;
    /// the provider checks association again during its native invocation.
    pub(crate) fn pinned_app_launcher(
        &self,
        event: &Event,
    ) -> Result<Option<app_picker::AppLauncher>> {
        let props = pinned::properties(&self.db, event.id)?;
        if !props.pinned {
            return Ok(None);
        }
        let Some(opener) = props.opener else {
            return Ok(None);
        };
        let id = opener
            .to_str()
            .ok_or_else(|| ApiError::new("invalid_argument", "Application ID must be Unicode"))?;
        self.validate_handler(event, id)?;
        Ok(Some(self.app_launcher(event, Some(id.to_owned()), None)))
    }

    pub(crate) fn open_pinned_default(&self, event: &Event) -> Result<Value> {
        let launcher = self
            .pinned_app_launcher(event)?
            .ok_or_else(|| ApiError::new("no_pinned_opener", "No pinned application selected"))?;
        execute_pinned(event, &launcher, &self.root, self.quota)
    }
}

/// Link content stays an exact validated URL; the provider receives that URL,
/// never the backing TXT. Native Shell handlers may reject URL data objects.
pub(crate) fn execute_pinned(
    event: &Event,
    launcher: &app_picker::AppLauncher,
    root: &Path,
    quota: u64,
) -> Result<Value> {
    let mut result = if event.kind == EventKind::LinkFragment {
        open_file::execute_link(event, launcher)?
    } else {
        open_file::execute_with_cache(event, open_file::Operation::Open, launcher, root, quota)?
    };
    result["application_id"] = json!(launcher.app_id);
    Ok(result)
}

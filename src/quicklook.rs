//! External preview boundary. Only QuickLook receives the source path.
use crate::{ApiError, Event, Result, preview};
use serde_json::{Value, json};
use std::path::Path;
pub trait Launcher: Send {
    fn launch(&self, path: &Path) -> Result<()>;
    fn launch_receipt(&self, path: &Path) -> Result<Value> {
        self.launch(path)?;
        Ok(json!({"requested":true,"acknowledgement":"launcher_returned"}))
    }
}
pub struct NativeLauncher;
pub fn is_native_image(event: &Event) -> bool {
    crate::file_types::is_native_image(Path::new(&event.name))
}
pub fn preview(event: &Event, launcher: &dyn Launcher) -> Result<Value> {
    if !event.path.is_file() {
        return Err(ApiError::new(
            "preview_unavailable",
            "Source file unavailable",
        ));
    }
    let current = if event.revision.starts_with("ref:") {
        format!("ref:{}", preview::content_hash(&event.path)?)
    } else {
        preview::revision(
            &event.path,
            if event.storage == "cached" {
                event.path.file_name().and_then(|s| s.to_str())
            } else {
                None
            },
        )
    };
    if current != event.revision {
        return Err(ApiError::new(
            "source_changed",
            "Source contents changed since this arrival",
        ));
    }
    let receipt = launcher.launch_receipt(&event.path)?;
    Ok(
        json!({"id":event.id,"revision":event.revision,"backend":"quicklook","requested":true,"receipt":receipt}),
    )
}
impl Launcher for NativeLauncher {
    fn launch(&self, path: &Path) -> Result<()> {
        crate::quicklook_backend::toggle(path).map(|_| ())
    }
    fn launch_receipt(&self, path: &Path) -> Result<Value> {
        crate::quicklook_backend::toggle(path)
    }
}
/// Convert canonical Windows paths to the form consumed by QuickLook/.NET.
/// Validation happens before either the pipe or executable boundary.
pub fn protocol_path(path: &Path) -> Result<String> {
    let text = path
        .to_str()
        .filter(|p| !p.contains(['\r', '\n', '|']))
        .ok_or_else(|| {
            ApiError::new(
                "invalid_argument",
                "QuickLook requires a Unicode path without protocol separators",
            )
        })?;
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        Ok(format!(r"\\{unc}"))
    } else if let Some(drive) = text.strip_prefix(r"\\?\") {
        if drive.as_bytes().get(1) == Some(&b':') {
            Ok(drive.to_owned())
        } else {
            Err(ApiError::new(
                "invalid_argument",
                "QuickLook requires a drive or UNC path",
            ))
        }
    } else {
        Ok(text.to_owned())
    }
}
pub fn toggle_message(path: &Path) -> Result<String> {
    Ok(format!(
        "QuickLook.App.PipeMessages.Toggle|{}|\n",
        protocol_path(path)?
    ))
}

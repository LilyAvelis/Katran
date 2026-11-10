//! External preview boundary. Only QuickLook receives the source path.
use crate::{ApiError, Event, Result, preview};
use serde_json::{Value, json};
use std::path::Path;
pub trait Launcher: Send {
    fn launch(&self, path: &Path) -> Result<()>;
}
pub struct NativeLauncher;
pub fn is_native_image(event: &Event) -> bool {
    Path::new(&event.name)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
            )
        })
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
    launcher.launch(&event.path)?;
    Ok(json!({"id":event.id,"revision":event.revision,"backend":"quicklook","requested":true}))
}
impl Launcher for NativeLauncher {
    fn launch(&self, path: &Path) -> Result<()> {
        #[cfg(windows)]
        {
            native::launch(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(ApiError::new(
                "unsupported_platform",
                "QuickLook preview requires Windows",
            ))
        }
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
#[cfg(windows)]
mod native {
    use super::*;
    use std::{path::PathBuf, process::Command};

    // Delegate IPC to QuickLook itself. Opening a probe connection to its pipe
    // can terminate the uncaught reader task in QuickLook 4.5 on empty EOF.
    pub(super) fn launch(path: &Path) -> Result<()> {
        let path = protocol_path(&std::fs::canonicalize(path)?)?;
        let mut candidates = Vec::new();
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(root).join("Programs/QuickLook/QuickLook.exe"));
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(variable) {
                candidates.push(PathBuf::from(root).join("QuickLook/QuickLook.exe"));
            }
        }
        if let Some(root) = std::env::var_os("USERPROFILE") {
            candidates.push(PathBuf::from(root).join("scoop/apps/quicklook/current/QuickLook.exe"));
        }
        let executable = candidates
            .into_iter()
            .find(|p| p.is_absolute() && p.is_file())
            .ok_or_else(|| {
                ApiError::new(
                    "quicklook_not_found",
                    "QuickLook desktop executable was not found in standard installation locations",
                )
            })?;
        Command::new(executable)
            .arg(path)
            .spawn()
            .map_err(|e| ApiError::new("quicklook_launch_failed", e))?;
        Ok(())
    }
}

//! Downloads discovery and recycle boundary. Tests use synthetic folders and fake trash.
use crate::api::{ApiError, Result};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

const MAX_ENTRIES: usize = 20_000;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Revision {
    size: u64,
    modified: SystemTime,
}

#[derive(Clone, Copy)]
struct Observation {
    revision: Revision,
    offered: bool,
}

#[derive(Default)]
pub struct DownloadsScanner {
    observations: HashMap<PathBuf, Observation>,
}

impl DownloadsScanner {
    pub fn scan(&mut self, root: &Path) -> Result<Vec<PathBuf>> {
        let root = fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(ApiError::new(
                "invalid_directory",
                "Downloads root must be a directory",
            ));
        }
        let mut pending = vec![root];
        let mut entries = 0;
        let mut current = HashMap::new();
        let mut ready = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                entries += 1;
                if entries > MAX_ENTRIES {
                    return Err(ApiError::new(
                        "scan_limit",
                        "Downloads scan exceeds 20000 entries",
                    ));
                }
                let kind = match entry.file_type() {
                    Ok(kind) => kind,
                    Err(_) => continue,
                };
                let path = entry.path();
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() && supported(&path) {
                    let Some(revision) = readable_revision(&path) else {
                        continue;
                    };
                    let previous = self.observations.get(&path);
                    let unchanged = previous.is_some_and(|old| old.revision == revision);
                    let offered = unchanged && previous.is_some_and(|old| old.offered);
                    if unchanged && !offered {
                        ready.push(path.clone());
                    }
                    current.insert(
                        path,
                        Observation {
                            revision,
                            offered: unchanged,
                        },
                    );
                }
            }
        }
        self.observations = current;
        ready.sort();
        Ok(ready)
    }
}

fn supported(path: &Path) -> bool {
    !path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["crdownload", "part", "partial", "tmp", "download"]
                .iter()
                .any(|temporary| ext.eq_ignore_ascii_case(temporary))
        })
}

fn readable_revision(path: &Path) -> Option<Revision> {
    let mut file = File::open(path).ok()?;
    let before = file.metadata().ok()?;
    let revision = Revision {
        size: before.len(),
        modified: before.modified().ok()?,
    };
    if before.len() > 0 {
        file.read_exact(&mut [0]).ok()?;
    }
    let after = file.metadata().ok()?;
    (after.len() == revision.size && after.modified().ok()? == revision.modified)
        .then_some(revision)
}

pub fn configured_folder(override_folder: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_folder {
        return Ok(path.to_path_buf());
    }
    system_downloads_folder()
}

#[cfg(windows)]
fn system_downloads_folder() -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    unsafe {
        let value = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None)
            .map_err(|e| ApiError::new("downloads_folder", e.to_string()))?;
        let path = std::ffi::OsString::from_wide(value.as_wide());
        CoTaskMemFree(Some(value.0.cast()));
        Ok(PathBuf::from(path))
    }
}
#[cfg(not(windows))]
fn system_downloads_folder() -> Result<PathBuf> {
    Err(ApiError::new(
        "unsupported_platform",
        "System Downloads discovery requires Windows; configure a test folder",
    ))
}

/// Injectable recycle boundary. No permanent-deletion fallback is allowed.
pub trait Trash: Send {
    fn recycle(&self, path: &Path) -> Result<()>;
}
pub struct NativeTrash;
#[cfg(windows)]
impl Trash for NativeTrash {
    fn recycle(&self, path: &Path) -> Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::{
                System::Com::{
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                    CoInitializeEx, CoUninitialize,
                },
                UI::Shell::*,
            },
            core::PCWSTR,
        };
        let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            // RPC_E_CHANGED_MODE means the caller already owns a different apartment.
            if initialized.is_err() && initialized.0 != 0x80010106u32 as i32 {
                return Err(ApiError::new("recycle_failed", initialized.to_string()));
            }
            let result = (|| -> windows::core::Result<()> {
                let operation: IFileOperation =
                    CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
                operation.SetOperationFlags(
                    FOF_ALLOWUNDO
                        | FOFX_RECYCLEONDELETE
                        | FOFX_EARLYFAILURE
                        | FOF_WANTNUKEWARNING
                        | FOF_RENAMEONCOLLISION
                        | FOF_NOCONFIRMATION
                        | FOF_NOERRORUI
                        | FOF_SILENT,
                )?;
                let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
                operation.DeleteItem(&item, None)?;
                operation.PerformOperations()?;
                if operation.GetAnyOperationsAborted()?.as_bool() {
                    return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                        0x80004004u32 as i32,
                    )));
                }
                Ok(())
            })();
            if initialized.is_ok() {
                CoUninitialize();
            }
            result.map_err(|e| ApiError::new("recycle_failed", e.to_string()))
        }
    }
}
#[cfg(not(windows))]
impl Trash for NativeTrash {
    fn recycle(&self, _: &Path) -> Result<()> {
        Err(ApiError::new(
            "unsupported_platform",
            "Recycle Bin requires Windows",
        ))
    }
}

/// Returns true for an already missing original; checks happen before calling trash.
pub fn recycle_original(event: &crate::Event, root: &Path, trash: &dyn Trash) -> Result<bool> {
    if event.source != "downloads"
        || event
            .path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ApiError::new(
            "unsafe_source",
            "Expected a Downloads original without parent traversal",
        ));
    }
    let root = root.canonicalize()?;
    let mut ancestor = event
        .path
        .parent()
        .ok_or_else(|| ApiError::new("unsafe_source", "Source has no parent"))?;
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ancestor = ancestor.parent().ok_or_else(|| {
                    ApiError::new("unsafe_source", "Source has no existing parent")
                })?
            }
            Err(e) => return Err(e.into()),
        }
    }
    if !ancestor.canonicalize()?.starts_with(&root) {
        return Err(ApiError::new(
            "unsafe_source",
            "Source is outside the Downloads folder",
        ));
    }
    match fs::symlink_metadata(&event.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(e.into()),
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(ApiError::new(
                "unsafe_source",
                "Source is not a regular file",
            ));
        }
        Ok(_) => {}
    }
    let canonical = event.path.canonicalize()?;
    if canonical == root || !canonical.starts_with(&root) {
        return Err(ApiError::new(
            "unsafe_source",
            "Source escapes the Downloads folder",
        ));
    }
    let hash = event
        .revision
        .strip_prefix("ref:")
        .ok_or_else(|| ApiError::new("stale_selection", "Source has no recorded content hash"))?;
    if crate::preview::content_hash(&canonical)? != hash {
        return Err(ApiError::new(
            "stale_selection",
            "Source contents changed; refresh before deleting",
        ));
    }
    trash.recycle(&canonical)?;
    Ok(false)
}

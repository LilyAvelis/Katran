use std::path::{Path, PathBuf};

/// Wallpaper Engine must already be running. The running process supplies the
/// executable path, avoiding guesses about Steam library locations.
#[cfg(windows)]
fn running_executable() -> Option<PathBuf> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
    };
    #[repr(C)]
    struct ProcessEntry {
        size: u32,
        usage: u32,
        pid: u32,
        heap: usize,
        module: u32,
        threads: u32,
        parent: u32,
        priority: i32,
        flags: u32,
        name: [u16; 260],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut core::ffi::c_void;
        fn Process32FirstW(snapshot: *mut core::ffi::c_void, entry: *mut ProcessEntry) -> i32;
        fn Process32NextW(snapshot: *mut core::ffi::c_void, entry: *mut ProcessEntry) -> i32;
    }
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(2, 0);
        if snapshot as isize == -1 {
            return None;
        }
        let mut entry: ProcessEntry = std::mem::zeroed();
        entry.size = std::mem::size_of::<ProcessEntry>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry);
        let mut found = None;
        while more != 0 {
            let len = entry
                .name
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.name.len());
            let name = String::from_utf16_lossy(&entry.name[..len]);
            if name.eq_ignore_ascii_case("wallpaper32.exe")
                || name.eq_ignore_ascii_case("wallpaper64.exe")
            {
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.pid);
                if !process.is_null() {
                    let mut buffer = vec![0u16; 32768];
                    let mut length = buffer.len() as u32;
                    if QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) != 0
                    {
                        use std::os::windows::ffi::OsStringExt;
                        found = Some(PathBuf::from(std::ffi::OsString::from_wide(
                            &buffer[..length as usize],
                        )));
                    }
                    CloseHandle(process);
                }
                if found.is_some() {
                    break;
                }
            }
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
fn running_executable() -> Option<PathBuf> {
    None
}

// Rust canonical paths on Windows commonly carry a verbatim prefix; pass the
// equivalent ordinary DOS/UNC path to Wallpaper Engine for compatibility.
#[cfg(windows)]
fn command_path(path: &Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    let unc: Vec<u16> = r"UNC\".encode_utf16().collect();
    if wide.starts_with(&prefix) {
        let tail = &wide[prefix.len()..];
        let mut normal = Vec::new();
        if tail.starts_with(&unc) {
            normal.extend(r"\\".encode_utf16());
            normal.extend_from_slice(&tail[unc.len()..]);
        } else {
            normal.extend_from_slice(tail);
        }
        return PathBuf::from(std::ffi::OsString::from_wide(&normal));
    }
    path.to_owned()
}
#[cfg(not(windows))]
fn command_path(path: &Path) -> PathBuf {
    path.to_owned()
}

pub(crate) fn validate(root: &Path, event: &crate::Event) -> crate::Result<PathBuf> {
    if crate::desktop_projection::is_desktop_id(event.id) {
        crate::desktop_projection::resolve(root, event.id, &event.revision)?;
    } else if let Some(expected) = event
        .revision
        .strip_prefix("ref:")
        .or_else(|| event.revision.strip_prefix("blob:"))
    {
        if crate::preview::content_hash(&event.path)? != expected {
            return Err(crate::ApiError::new(
                "stale_selection",
                "Video contents changed since selection",
            ));
        }
    } else if crate::preview::revision(&event.path, None) != event.revision {
        return Err(crate::ApiError::new(
            "stale_selection",
            "Video changed since selection",
        ));
    }
    std::path::absolute(&event.path)
        .map_err(|e| crate::ApiError::new("wallpaper_engine_error", e.to_string()))
}

/// Store a content-addressed copy at a durable ASCII-only per-user path. WE's
/// command-line parser may corrupt non-ASCII file paths even when Windows accepts them.
pub(crate) fn stage(root: &Path, event: &crate::Event) -> crate::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.to_str().is_some_and(str::is_ascii))
        .ok_or_else(|| {
            crate::ApiError::new(
                "wallpaper_engine_error",
                "Cannot find an ASCII-only Local AppData path for Wallpaper Engine",
            )
        })?;
    let directory = base.join("Katran").join("WallpaperEngine");
    stage_in(root, event, &directory)
}

fn stage_in(root: &Path, event: &crate::Event, directory: &Path) -> crate::Result<PathBuf> {
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, OpenOptions},
        io::{Read, Write},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if !directory.is_absolute() || !directory.to_str().is_some_and(str::is_ascii) {
        return Err(crate::ApiError::new(
            "wallpaper_engine_error",
            "Wallpaper Engine copy needs an ASCII-only absolute directory",
        ));
    }
    validate(root, event)?;
    fs::create_dir_all(directory)?;
    let suffix = event
        .path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(suffix.as_str(), "mp4" | "m4v" | "mov" | "mkv" | "webm") {
        return Err(crate::ApiError::new(
            "invalid_video",
            "Unsupported video extension",
        ));
    }
    let tmp = directory.join(format!(
        "katran-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> crate::Result<PathBuf> {
        let mut input = fs::File::open(&event.path)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            output.write_all(&buffer[..n])?;
        }
        output.sync_all()?;
        drop(output);
        let digest = format!("{:x}", hash.finalize());
        if let Some(expected) = event
            .revision
            .strip_prefix("ref:")
            .or_else(|| event.revision.strip_prefix("blob:"))
        {
            if digest != expected {
                return Err(crate::ApiError::new(
                    "stale_selection",
                    "Video changed while copying",
                ));
            }
        }
        validate(root, event)?;
        let target = directory.join(format!("{digest}.{suffix}"));
        if target.exists() {
            if crate::preview::content_hash(&target)? != digest {
                return Err(crate::ApiError::new(
                    "wallpaper_engine_error",
                    "Existing Wallpaper Engine copy does not match its content",
                ));
            }
        } else if let Err(error) = fs::rename(&tmp, &target) {
            if !target.exists() || crate::preview::content_hash(&target)? != digest {
                return Err(error.into());
            }
        }
        Ok(target)
    })();
    let _ = fs::remove_file(tmp);
    result
}

pub fn apply(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("Video path must be an existing absolute file".into());
    }
    let executable = running_executable().ok_or(
        "Wallpaper Engine is not running or could not be found; install and start it first",
    )?;
    let status = std::process::Command::new(executable)
        .args(["-control", "openWallpaper", "-file"])
        .arg(command_path(path))
        .status()
        .map_err(|e| format!("Could not contact Wallpaper Engine: {e}"))?;
    if !status.success() {
        return Err(format!("Wallpaper Engine command failed: {status}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Request, Service};

    #[test]
    fn copies_unicode_source_to_durable_ascii_path_without_altering_source() {
        let root = tempfile::tempdir().unwrap();
        let original_dir = tempfile::tempdir().unwrap();
        let unicode_dir = original_dir.path().join("кириллица");
        std::fs::create_dir(&unicode_dir).unwrap();
        let source = unicode_dir.join("видео.mp4");
        let bytes = b"synthetic video bytes";
        std::fs::write(&source, bytes).unwrap();
        let mut service = Service::open(root.path(), 100000).unwrap();
        let result = service.execute(Request {
            api_version: 1,
            request_id: "stage".into(),
            command: Command::AddFiles {
                paths: vec![source.clone()],
            },
        });
        assert_eq!(result["ok"], true, "{result}");
        let event: crate::Event =
            serde_json::from_value(result["result"]["items"][0]["event"].clone()).unwrap();
        let dest = root.path().join("ascii-stage");
        let copy = stage_in(root.path(), &event, &dest).unwrap();
        assert!(copy.to_str().unwrap().is_ascii());
        assert_eq!(std::fs::read(&copy).unwrap(), bytes);
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        assert_eq!(stage_in(root.path(), &event, &dest).unwrap(), copy);
        std::fs::write(&source, b"changed").unwrap();
        assert!(stage_in(root.path(), &event, &dest).is_err());
        assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    }
}

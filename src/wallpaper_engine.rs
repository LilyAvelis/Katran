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

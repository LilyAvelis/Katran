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
            && digest != expected
        {
            return Err(crate::ApiError::new(
                "stale_selection",
                "Video changed while copying",
            ));
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
        } else if let Err(error) = fs::rename(&tmp, &target)
            && (!target.exists() || crate::preview::content_hash(&target)? != digest)
        {
            return Err(error.into());
        }
        Ok(target)
    })();
    let _ = fs::remove_file(tmp);
    result
}

/// Remux only the video stream; no audio, subtitles, or global WE mute.
fn silent_copy(source: &Path) -> crate::Result<PathBuf> {
    use std::{
        fs,
        process::Stdio,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    static NEXT_SILENT: AtomicU64 = AtomicU64::new(0);
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "mp4" | "m4v" | "mov" | "mkv" | "webm") {
        return Err(crate::ApiError::new(
            "invalid_video",
            "Unsupported video extension",
        ));
    }
    let dir = source
        .parent()
        .ok_or_else(|| crate::ApiError::new("wallpaper_engine_error", "Missing video directory"))?;
    let temp = dir.join(format!(
        "katran-silent-{}-{}.{}",
        std::process::id(),
        NEXT_SILENT.fetch_add(1, Ordering::Relaxed),
        ext
    ));
    let result = (|| -> crate::Result<PathBuf> {
        let mut cmd = crate::media_runtime::command("ffmpeg")
            .map_err(|e| crate::ApiError::new("video_background_error", e))?;
        cmd.args(["-v", "error", "-nostdin", "-n", "-i"])
            .arg(source)
            .args([
                "-map",
                "0:v:0",
                "-c:v",
                "copy",
                "-an",
                "-sn",
                "-dn",
                "-map_metadata",
                "-1",
                "-map_chapters",
                "-1",
            ])
            .arg(&temp)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| crate::ApiError::new("video_background_error", e.to_string()))?;
        let start = Instant::now();
        loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| crate::ApiError::new("video_background_error", e.to_string()))?
            {
                if !status.success() {
                    return Err(crate::ApiError::new(
                        "video_background_error",
                        "Could not create a silent video copy",
                    ));
                }
                break;
            }
            if start.elapsed() >= Duration::from_secs(120) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::ApiError::new(
                    "video_background_error",
                    "Silent video preparation timed out",
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let digest = crate::preview::content_hash(&temp)?;
        let target = dir.join(format!("{digest}.{ext}"));
        if target.exists() {
            if crate::preview::content_hash(&target)? != digest {
                return Err(crate::ApiError::new(
                    "wallpaper_engine_error",
                    "Existing silent copy does not match its content",
                ));
            }
        } else if let Err(error) = fs::rename(&temp, &target)
            && (!target.exists() || crate::preview::content_hash(&target)? != digest)
        {
            return Err(error.into());
        }
        Ok(target)
    })();
    let _ = fs::remove_file(temp);
    result
}

pub(crate) fn stage_silent(root: &Path, event: &crate::Event) -> crate::Result<PathBuf> {
    let source = stage(root, event)?;
    let output = silent_copy(&source)?;
    validate(root, event)?;
    Ok(output)
}

pub(crate) fn handoff(
    root: &Path,
    staged: &Path,
    original: &Path,
    muted: bool,
) -> crate::Result<serde_json::Value> {
    handoff_with(root, staged, original, muted, apply)
}

fn handoff_with(
    root: &Path,
    staged: &Path,
    original: &Path,
    muted: bool,
    apply: impl FnOnce(&Path) -> Result<(), String>,
) -> crate::Result<serde_json::Value> {
    let _lock = crate::desktop_assets::wallpaper_lock(root)?;
    let asset = crate::desktop_assets::prepare_wallpaper_video(root, staged, original)?;
    let id = asset["id"]
        .as_str()
        .and_then(|id| id.strip_prefix("wallpaper_video:"))
        .ok_or_else(|| crate::ApiError::new("invalid_video", "Missing wallpaper ID"))?;
    crate::desktop_assets::mark_wallpaper_pending(root, id)?;
    apply(staged).map_err(|error| {
        crate::ApiError::new(
            "wallpaper_outcome_unknown",
            format!("Wallpaper Engine outcome is unknown; the video remains protected: {error}"),
        )
    })?;
    crate::desktop_assets::finish_wallpaper_video(root, id).map_err(|error| crate::ApiError::new("wallpaper_outcome_unknown", format!("Wallpaper Engine accepted the command, but its receipt could not be finalized; the video remains protected: {}", error.message)))?;
    Ok(
        serde_json::json!({"video_desktop_wallpaper_requested": true, "muted": muted, "desktop_asset": asset}),
    )
}

fn wait_for_command(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::ExitStatus, String> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(25))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Wallpaper Engine command timed out".into());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not wait for Wallpaper Engine: {error}"));
            }
        }
    }
}

pub fn apply(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("Video path must be an existing absolute file".into());
    }
    let executable = running_executable().ok_or(
        "Wallpaper Engine is not running or could not be found; install and start it first",
    )?;
    let child = std::process::Command::new(executable)
        .args(["-control", "openWallpaper", "-file"])
        .arg(command_path(path))
        .spawn()
        .map_err(|e| format!("Could not contact Wallpaper Engine: {e}"))?;
    let status = wait_for_command(child, std::time::Duration::from_secs(30))?;
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
    fn ambiguous_handoff_keeps_both_videos_protected_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let staged = tempfile::tempdir().unwrap();
        let first = staged.path().join("first.mp4");
        let second = staged.path().join("second.mp4");
        let third = staged.path().join("third.mp4");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();
        std::fs::write(&third, b"third").unwrap();
        let accepted = handoff_with(root.path(), &first, &first, false, |_| Ok(())).unwrap();
        let first_id = accepted["desktop_asset"]["id"].as_str().unwrap().to_owned();
        let error = handoff_with(root.path(), &second, &second, false, |_| {
            Err("timeout".into())
        })
        .unwrap_err();
        assert_eq!(error.code, "wallpaper_outcome_unknown");
        let second_id = format!(
            "wallpaper_video:{}",
            crate::preview::content_hash(&second).unwrap()
        );
        // A fresh service sees the durable pending marker, as after process interruption.
        let _service = Service::open(root.path(), 100000).unwrap();
        for id in [&first_id, &second_id] {
            assert_eq!(
                crate::desktop_assets::delete(
                    root.path(),
                    id,
                    &crate::theme::ThemePreferences::default(),
                    None,
                    &[]
                )
                .unwrap_err()
                .code,
                "background_in_use"
            );
        }
        let accepted = handoff_with(root.path(), &second, &second, false, |_| Ok(())).unwrap();
        assert_eq!(accepted["desktop_asset"]["id"], second_id);
        crate::desktop_assets::delete(
            root.path(),
            &first_id,
            &crate::theme::ThemePreferences::default(),
            None,
            &[],
        )
        .unwrap();
        assert_eq!(
            crate::desktop_assets::delete(
                root.path(),
                &second_id,
                &crate::theme::ThemePreferences::default(),
                None,
                &[]
            )
            .unwrap_err()
            .code,
            "background_in_use"
        );
        // Unwinding in the external call simulates interruption after preparation.
        assert!(
            std::panic::catch_unwind(|| {
                let _ = handoff_with(root.path(), &third, &third, false, |_| {
                    panic!("interrupted")
                });
            })
            .is_err()
        );
        let third_id = format!(
            "wallpaper_video:{}",
            crate::preview::content_hash(&third).unwrap()
        );
        assert_eq!(
            crate::desktop_assets::delete(
                root.path(),
                &third_id,
                &crate::theme::ThemePreferences::default(),
                None,
                &[]
            )
            .unwrap_err()
            .code,
            "background_in_use"
        );

        let failed_receipt = tempfile::tempdir().unwrap();
        let error = handoff_with(failed_receipt.path(), &first, &first, false, |_| {
            std::fs::create_dir(
                failed_receipt
                    .path()
                    .join("wallpaper-videos")
                    .join("active"),
            )
            .unwrap();
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.code, "wallpaper_outcome_unknown");
        let digest = crate::preview::content_hash(&first).unwrap();
        assert!(
            failed_receipt
                .path()
                .join("wallpaper-videos")
                .join(format!("pending-{digest}"))
                .is_file()
        );
    }

    #[test]
    fn timeout_child_fixture() {
        if std::env::var_os("KATRAN_TIMEOUT_CHILD").is_some() {
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }

    #[test]
    fn stalled_wallpaper_command_is_killed_and_reaped() {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "wallpaper_engine::tests::timeout_child_fixture"])
            .env("KATRAN_TIMEOUT_CHILD", "1")
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        assert!(
            wait_for_command(child, std::time::Duration::from_millis(100))
                .unwrap_err()
                .contains("timed out")
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

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

#[cfg(test)]
mod silent_tests {
    use super::*;
    #[test]
    fn remux_removes_audio_without_changing_original() {
        if crate::media_runtime::command("ffmpeg").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("with-audio.mp4");
        let status = crate::media_runtime::command("ffmpeg")
            .unwrap()
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=size=32x32:rate=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440",
                "-t",
                "1",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-y",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        let before = std::fs::read(&source).unwrap();
        let output = silent_copy(&source).unwrap();
        let probe = crate::media_runtime::command("ffprobe")
            .unwrap()
            .args([
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(probe.status.success());
        assert!(probe.stdout.is_empty());
        assert_eq!(std::fs::read(&source).unwrap(), before);
        assert_ne!(source, output);
    }
}

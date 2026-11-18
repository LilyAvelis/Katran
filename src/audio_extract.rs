//! Background audio extraction. Track ordinals are zero-based among audio streams.
//! The caller owns freshness validation and publication; dropping the result removes
//! its private staged output, including on cancellation or rejected commit.
use crate::{ApiError, Result, domain_jobs::Cancellation};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const MAX_PROBE: usize = 64 * 1024;
const MAX_OUTPUT: u64 = 2 * 1024 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

pub(crate) struct StagedAudio {
    dir: PathBuf,
    path: PathBuf,
}
impl StagedAudio {
    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Publish under an existing profile-owned directory on the same volume.
    pub(crate) fn publish(self, destination: &Path) -> Result<PathBuf> {
        let destination = checked_directory(destination)?;
        let staging =
            checked_directory(self.dir.parent().ok_or_else(|| {
                ApiError::new("audio_extract_error", "Invalid staging directory")
            })?)?;
        if destination.starts_with(&staging) {
            return Err(ApiError::new(
                "audio_extract_error",
                "Audio destination must be separate from staging",
            ));
        }
        for _ in 0..128 {
            let dir = destination.join(format!(
                "audio-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&dir) {
                Ok(()) => {
                    let output = dir.join("audio.m4a");
                    if let Err(error) = fs::rename(&self.path, &output) {
                        let _ = fs::remove_dir(&dir);
                        return Err(error.into());
                    }
                    return Ok(output);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(ApiError::new(
            "audio_extract_error",
            "Could not reserve an audio destination",
        ))
    }
}
impl Drop for StagedAudio {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn checked_directory(path: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)?;
    let mut safe = metadata.is_dir() && !metadata.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        safe &= metadata.file_attributes() & 0x400 == 0;
    }
    if !safe {
        return Err(ApiError::new(
            "audio_extract_error",
            "Unsafe audio directory",
        ));
    }
    Ok(path.canonicalize()?)
}

/// Create and verify the direct managed directories under the trusted profile.
pub(crate) fn directories(root: &Path) -> Result<(PathBuf, PathBuf)> {
    fn child(parent: &Path, name: &str) -> Result<PathBuf> {
        let path = parent.join(name);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let canonical = checked_directory(&path)?;
        if canonical.parent() != Some(parent) {
            return Err(ApiError::new(
                "audio_extract_error",
                "Unsafe audio directory",
            ));
        }
        Ok(canonical)
    }
    let root = root.canonicalize()?;
    let destination = child(&root, "audio-extractions")?;
    let staging = child(&destination, ".staging")?;
    Ok((destination, staging))
}

fn stage(root: &Path) -> Result<StagedAudio> {
    let root = checked_directory(root)?;
    for _ in 0..128 {
        let dir = root.join(format!(
            "audio-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&dir) {
            Ok(()) => {
                return Ok(StagedAudio {
                    path: dir.join("audio.m4a"),
                    dir,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(ApiError::new(
        "audio_extract_error",
        "Could not reserve a temporary audio directory",
    ))
}

fn configure(cmd: &mut Command) {
    cmd.stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
fn wait(child: &mut Child, cancel: &Cancellation, output: Option<&Path>) -> Result<()> {
    let start = Instant::now();
    loop {
        if let Err(e) = cancel.check() {
            terminate(child);
            return Err(e);
        }
        if output
            .and_then(|p| fs::metadata(p).ok())
            .is_some_and(|m| m.len() > MAX_OUTPUT)
        {
            terminate(child);
            return Err(ApiError::new(
                "audio_extract_error",
                "Extracted audio exceeds the size limit",
            ));
        }
        match child.try_wait() {
            Ok(Some(s)) if s.success() => return Ok(()),
            Ok(Some(_)) => {
                return Err(ApiError::new(
                    "audio_extract_error",
                    "Media tool could not process this file",
                ));
            }
            Err(e) => {
                terminate(child);
                return Err(ApiError::new(
                    "audio_extract_error",
                    format!("Could not wait for media tool: {e}"),
                ));
            }
            Ok(None) if start.elapsed() >= TIMEOUT => {
                terminate(child);
                return Err(ApiError::new(
                    "audio_extract_error",
                    "Audio extraction timed out",
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}
fn track_count(bytes: &[u8], overflow: bool) -> Result<usize> {
    if overflow {
        return Err(ApiError::new(
            "audio_extract_error",
            "Too many audio streams",
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ApiError::new("audio_extract_error", "Invalid media stream information"))?;
    let mut count = 0;
    for line in text.lines().filter(|s| !s.trim().is_empty()) {
        line.trim().parse::<u32>().map_err(|_| {
            ApiError::new("audio_extract_error", "Invalid media stream information")
        })?;
        count += 1;
    }
    Ok(count)
}

/// Extract the selected audio stream to AAC in M4A. `None` selects audio ordinal 0.
pub(crate) fn extract(
    source: &Path,
    staging_root: &Path,
    audio_ordinal: Option<u32>,
    cancel: &Cancellation,
) -> Result<StagedAudio> {
    cancel.check()?;
    if !source.is_absolute() || !source.is_file() {
        return Err(ApiError::new(
            "invalid_video",
            "Source must be an existing absolute file",
        ));
    }
    let staged = stage(staging_root)?;
    let mut probe = crate::media_runtime::command("ffprobe")
        .map_err(|e| ApiError::new("audio_extract_error", e))?;
    probe
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            "mov,matroska,webm",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(source)
        .stdout(Stdio::piped());
    configure(&mut probe);
    let mut child = probe
        .spawn()
        .map_err(|e| ApiError::new("audio_extract_error", e.to_string()))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    // Drain even after the bounded buffer fills, avoiding a blocked child.
    let reader = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 4096];
        let mut overflow = false;
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let take = n.min(MAX_PROBE.saturating_sub(kept.len()));
                    kept.extend_from_slice(&buf[..take]);
                    overflow |= take != n;
                }
                Err(_) => return None,
            }
        }
        Some((kept, overflow))
    });
    let status = wait(&mut child, cancel, None);
    let data = reader.join().ok().flatten();
    status?;
    let (bytes, overflow) = data.ok_or_else(|| {
        ApiError::new(
            "audio_extract_error",
            "Could not read media stream information",
        )
    })?;
    let count = track_count(&bytes, overflow)?;
    if count == 0 {
        return Err(ApiError::new("no_audio", "This video has no audio tracks"));
    }
    let ordinal = audio_ordinal.unwrap_or(0);
    if ordinal as usize >= count {
        return Err(ApiError::new(
            "invalid_audio_track",
            format!(
                "Audio track {} is unavailable; this video has {count} audio track(s)",
                u64::from(ordinal) + 1
            ),
        ));
    }
    cancel.check()?;
    let mut ffmpeg = crate::media_runtime::command("ffmpeg")
        .map_err(|e| ApiError::new("audio_extract_error", e))?;
    ffmpeg
        .args([
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            "mov,matroska,webm",
            "-i",
        ])
        .arg(source)
        .args([
            "-map",
            &format!("0:a:{ordinal}"),
            "-vn",
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
        ])
        .arg(&staged.path)
        .stdout(Stdio::null());
    configure(&mut ffmpeg);
    let mut child = ffmpeg
        .spawn()
        .map_err(|e| ApiError::new("audio_extract_error", e.to_string()))?;
    wait(&mut child, cancel, Some(&staged.path))?;
    cancel.check()?;
    let len = fs::metadata(&staged.path)?.len();
    if len == 0 || len > MAX_OUTPUT {
        return Err(ApiError::new(
            "audio_extract_error",
            "Invalid extracted audio size",
        ));
    }
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_probe_output() {
        assert_eq!(track_count(b"1\n3\n", false).unwrap(), 2);
        assert_eq!(track_count(b"", false).unwrap(), 0);
        assert!(track_count(b"x\n", false).is_err());
        assert!(track_count(b"1\n", true).is_err());
    }
    #[test]
    fn stage_is_unique_and_removed_on_drop() {
        let root = tempfile::tempdir().unwrap();
        let a = stage(root.path()).unwrap();
        let b = stage(root.path()).unwrap();
        assert_ne!(a.path(), b.path());
        fs::write(a.path(), b"staged").unwrap();
        let dir = a.dir.clone();
        drop(a);
        assert!(!dir.exists());
        assert!(b.dir.exists());
    }
    #[test]
    fn publish_is_unique_and_cleans_failed_reservation() {
        let root = tempfile::tempdir().unwrap();
        let (destination, staging) = directories(root.path()).unwrap();
        let first = stage(&staging).unwrap();
        fs::write(first.path(), b"first").unwrap();
        let first_path = first.publish(&destination).unwrap();
        let second = stage(&staging).unwrap();
        fs::write(second.path(), b"second").unwrap();
        let second_path = second.publish(&destination).unwrap();
        assert_ne!(first_path, second_path);
        assert_eq!(fs::read(first_path).unwrap(), b"first");
        assert_eq!(fs::read(second_path).unwrap(), b"second");
        let before = fs::read_dir(&destination).unwrap().count();
        let missing = stage(&staging).unwrap();
        assert!(missing.publish(&destination).is_err());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), before);
    }
}

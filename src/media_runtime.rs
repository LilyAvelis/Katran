//! Optional, explicitly installed FFmpeg runtime for video backgrounds.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const URL: &str = concat!(
    "https://github.com/LilyAvelis/Katran/releases/download/v",
    env!("CARGO_PKG_VERSION"),
    "/Katran-",
    env!("CARGO_PKG_VERSION"),
    "-media-runtime.zip"
);
const DIGEST: &str = "4ac2a4a732ee66707ab2a4d7fc135d085e1da691f365cdb2f1b48f8f3816d4d8";
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_EXTRACTED: u64 = 384 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn safe_regular(path: &Path) -> bool {
    let Ok(m) = fs::symlink_metadata(path) else {
        return false;
    };
    if !m.is_file() || m.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
fn digest_file(path: &Path) -> Result<String, String> {
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    if m.len() == 0 || m.len() > MAX_EXTRACTED || !safe_regular(path) {
        return Err("Unsafe media runtime file".into());
    }
    let mut hash = Sha256::new();
    let n = std::io::copy(
        &mut File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_EXTRACTED + 1),
        &mut hash,
    )
    .map_err(|e| e.to_string())?;
    if n != m.len() {
        return Err("Media runtime file changed".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn runtime_base() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    Ok(PathBuf::from(local).join("Katran").join("media-runtime"))
}
fn verify_installed(dir: &Path) -> bool {
    let receipt = dir.join("receipt.json");
    if !safe_regular(&receipt) {
        return false;
    }
    if fs::metadata(&receipt).map_or(true, |m| m.len() > 65536) {
        return false;
    }
    let Ok(data) = fs::read(receipt) else {
        return false;
    };
    if data.len() > 65536 {
        return false;
    }
    let Ok(map) = serde_json::from_slice::<BTreeMap<String, String>>(&data) else {
        return false;
    };
    if !map.contains_key("ffmpeg.exe") || !map.contains_key("ffprobe.exe") || map.len() > 128 {
        return false;
    }
    let mut total = 0u64;
    for (name, hash) in map {
        let Ok(meta) = fs::metadata(dir.join("tools").join(&name)) else {
            return false;
        };
        let Some(sum) = total.checked_add(meta.len()) else {
            return false;
        };
        total = sum;
        if total > MAX_EXTRACTED {
            return false;
        }
        if !tool_name(&name)
            || digest_file(&dir.join("tools").join(&name)).ok().as_deref() != Some(hash.as_str())
        {
            return false;
        }
    }
    true
}
fn tool_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if name != lower || name.contains('/') || name.contains('\\') || name.contains(':') {
        return false;
    }
    lower == "ffmpeg.exe"
        || lower == "ffprobe.exe"
        || ((lower.starts_with("av") || lower.starts_with("sw"))
            && lower.ends_with(".dll")
            && lower
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.' || c == b'_'))
}
fn pair(dir: &Path) -> bool {
    safe_regular(&dir.join("ffmpeg.exe")) && safe_regular(&dir.join("ffprobe.exe"))
}
fn bundled() -> Option<PathBuf> {
    let path = std::env::current_exe()
        .ok()?
        .parent()?
        .join("media-runtime")
        .join("tools");
    if pair(&path) { Some(path) } else { None }
}
fn installed() -> Option<PathBuf> {
    let base = runtime_base().ok()?;
    let mut dirs: Vec<_> = fs::read_dir(base)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.reverse();
    dirs.into_iter()
        .find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.starts_with("ffmpeg-9.0.2-"))
                && verify_installed(p)
        })
        .map(|p| p.join("tools"))
}
fn path_tool(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| safe_regular(p))
}
fn selected() -> Option<(PathBuf, PathBuf, &'static str)> {
    if let Some(d) = bundled() {
        return Some((d.join("ffmpeg.exe"), d.join("ffprobe.exe"), "bundled"));
    }
    if let Some(d) = installed() {
        return Some((d.join("ffmpeg.exe"), d.join("ffprobe.exe"), "installed"));
    }
    Some((path_tool("ffmpeg.exe")?, path_tool("ffprobe.exe")?, "path"))
}
pub fn status() -> Value {
    match selected() {
        Some((_, _, source)) => json!({"available":true,"source":source}),
        None => json!({"available":false,"source":"missing"}),
    }
}
pub fn command(name: &str) -> Result<Command, String> {
    if name != "ffmpeg" && name != "ffprobe" {
        return Err("Invalid media tool".into());
    }
    let (ffmpeg, ffprobe, _) =
        selected().ok_or("Video components are missing; install them in settings")?;
    Ok(Command::new(if name == "ffmpeg" {
        ffmpeg
    } else {
        ffprobe
    }))
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn unpack(bytes: &[u8], base: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let id = format!(
        "ffmpeg-9.0.2-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let stage = Stage(base.join(format!(".stage-{id}")));
    fs::create_dir(&stage.0).map_err(|e| e.to_string())?;
    let tools = stage.0.join("tools");
    fs::create_dir(&tools).map_err(|e| e.to_string())?;
    let licenses = stage.0.join("licenses");
    fs::create_dir(&licenses).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Invalid media archive: {e}"))?;
    if archive.len() > 4096 {
        return Err("Media archive has too many entries".into());
    }
    let mut hashes = BTreeMap::<String, String>::new();
    let mut total = 0u64;
    let mut license = false;
    for i in 0..archive.len() {
        let mut item = archive.by_index(i).map_err(|e| e.to_string())?;
        if item.is_dir() {
            continue;
        }
        let Some(path) = item.enclosed_name() else {
            return Err("Unsafe media archive path".into());
        };
        if item.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("Media archive contains a symbolic link".into());
        }
        let comps: Vec<_> = path
            .components()
            .filter_map(|c| {
                if let Component::Normal(n) = c {
                    n.to_str()
                } else {
                    None
                }
            })
            .collect();
        let Some(last) = comps.last() else {
            continue;
        };
        let name = last.to_ascii_lowercase();
        let is_tool = comps.len() >= 3
            && comps[comps.len() - 2].eq_ignore_ascii_case("bin")
            && tool_name(&name);
        let is_license = comps.len() == 2 && name == "license.txt";
        if !is_tool && !is_license {
            continue;
        }
        if item.size() == 0 || item.size() > MAX_EXTRACTED {
            return Err("Media archive file exceeds limit".into());
        }
        total = total
            .checked_add(item.size())
            .ok_or("Media archive is too large")?;
        if total > MAX_EXTRACTED {
            return Err("Media archive is too large".into());
        }
        let dest = if is_tool {
            tools.join(&name)
        } else {
            licenses.join("LICENSE.txt")
        };
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut item.by_ref().take(MAX_EXTRACTED + 1), &mut out)
            .map_err(|e| e.to_string())?;
        if copied != item.size() {
            return Err("Invalid media archive entry".into());
        }
        drop(out);
        if is_tool {
            hashes.insert(name, digest_file(&dest)?);
        } else {
            license = true;
        }
    }
    if !hashes.contains_key("ffmpeg.exe")
        || !hashes.contains_key("ffprobe.exe")
        || !license
        || hashes.len() > 128
    {
        return Err("Incomplete media runtime archive".into());
    }
    let mut receipt = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.0.join("receipt.json"))
        .map_err(|e| e.to_string())?;
    receipt
        .write_all(
            serde_json::to_string(&hashes)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
    drop(receipt);
    let target = base.join(id);
    fs::rename(&stage.0, &target).map_err(|e| e.to_string())?;
    Ok(target)
}
pub fn install() -> Result<Value, String> {
    if let Some(dir) = installed() {
        return Ok(json!({"available":true,"source":"installed","path":dir}));
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("Katran/3.1 media runtime")
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(URL)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Media runtime download failed: {e}"))?;
    if response.content_length().is_some_and(|n| n > MAX_ARCHIVE) {
        return Err("Media runtime download exceeds limit".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_ARCHIVE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    verify_archive(&bytes, DIGEST)?;
    let target = unpack(&bytes, &runtime_base()?)?;
    Ok(json!({"available":true,"source":"installed","path":target.join("tools")}))
}
fn verify_archive(bytes: &[u8], expected: &str) -> Result<(), String> {
    if bytes.len() as u64 > MAX_ARCHIVE || format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err("Media runtime checksum mismatch".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{ZipWriter, write::SimpleFileOptions};
    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn extraction_requires_complete_runtime_and_verifies_existing_contents() {
        let dir = tempfile::tempdir().unwrap();
        let data = archive(&[
            ("bundle/LICENSE.txt", b"license"),
            ("bundle/bin/ffmpeg.exe", b"ffmpeg"),
            ("bundle/bin/ffprobe.exe", b"ffprobe"),
            ("bundle/bin/avcodec-63.dll", b"dll"),
        ]);
        let installed = unpack(&data, dir.path()).unwrap();
        assert!(verify_installed(&installed));
        fs::write(installed.join("tools/ffmpeg.exe"), b"changed").unwrap();
        assert!(!verify_installed(&installed));
        assert!(unpack(&archive(&[("bundle/LICENSE.txt", b"license")]), dir.path()).is_err());
    }
    #[test]
    fn checksum_must_match_before_extraction() {
        let bytes = archive(&[("bundle/LICENSE.txt", b"license")]);
        assert!(verify_archive(&bytes, DIGEST).is_err());
        let expected = format!("{:x}", Sha256::digest(&bytes));
        assert!(verify_archive(&bytes, &expected).is_ok());
    }
    #[test]
    fn real_pinned_archive_extracts_and_tools_start() {
        let Some(path) = std::env::var_os("KATRAN_TEST_MEDIA_ARCHIVE") else {
            return;
        };
        let bytes = fs::read(path).unwrap();
        verify_archive(&bytes, DIGEST).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let result = unpack(&bytes, dir.path()).unwrap();
        assert!(verify_installed(&result));
        for exe in ["ffmpeg.exe", "ffprobe.exe"] {
            assert!(
                Command::new(result.join("tools").join(exe))
                    .arg("-version")
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
    }
    #[test]
    fn rejects_archive_traversal_and_duplicate_tools() {
        let dir = tempfile::tempdir().unwrap();
        let bad = archive(&[("../escape", b"bad")]);
        assert!(unpack(&bad, dir.path()).is_err());
        let dup = archive(&[("a/bin/ffmpeg.exe", b"one"), ("b/bin/ffmpeg.exe", b"two")]);
        assert!(unpack(&dup, dir.path()).is_err());
        assert!(!dir.path().parent().unwrap().join("escape").exists());
    }
}

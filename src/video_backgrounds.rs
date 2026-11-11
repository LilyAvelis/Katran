//! Bounded, silent managed video backgrounds. User files are never modified.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 128 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn temp(suffix: &str) -> Result<(Temp, File), String> {
    for _ in 0..100 {
        let p = std::env::temp_dir().join(format!(
            "katran-video-{}-{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            suffix
        ));
        match OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(f) => return Ok((Temp(p), f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Cannot create temporary video file".into())
}
fn run(mut cmd: Command, timeout: Duration) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Video tool unavailable: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("Video tool failed: {status}"))
            };
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Video tool timed out".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
}
fn probe(path: &Path) -> Result<(u32, u32, f64), String> {
    let (output, file) = temp("json")?;
    // Output is redirected to a file, so neither pipe can fill and deadlock.
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=codec_name,width,height:format=duration",
        "-of",
        "json",
    ])
    .arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffprobe unavailable: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            if !s.success() {
                return Err("Invalid or unsupported video".into());
            }
            break;
        }
        if start.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Video probe timed out".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
    let mut bytes = Vec::new();
    File::open(&output.0)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Video metadata too large".into());
    }
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid video metadata")?;
    let stream = v["streams"]
        .as_array()
        .and_then(|s| s.first())
        .ok_or("Video stream missing")?;
    if stream["codec_name"].as_str() != Some("h264") {
        return Err("Only H.264 video backgrounds are supported".into());
    }
    let w = stream["width"].as_u64().unwrap_or(0);
    let h = stream["height"].as_u64().unwrap_or(0);
    let duration = v["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    if w == 0
        || h == 0
        || w > 3840
        || h > 2160
        || !duration.is_finite()
        || duration <= 0.0
        || duration > 120.0
    {
        return Err("Video must be at most 120 seconds and 3840×2160".into());
    }
    Ok((w as u32, h as u32, duration))
}
pub struct PreparedVideo {
    file: Temp,
    width: u32,
    height: u32,
    duration: f64,
    name: String,
}
pub fn prepare(source: &Path) -> Result<PreparedVideo, String> {
    prepare_inner(source, None)
}
pub fn prepare_event(event: &crate::Event) -> Result<PreparedVideo, String> {
    let digest = event
        .revision
        .strip_prefix("ref:")
        .or_else(|| event.revision.strip_prefix("blob:"))
        .ok_or("Unsupported video revision")?;
    if !valid_id(digest) {
        return Err("Invalid video revision".into());
    }
    prepare_inner(&event.path, Some((event.size, digest)))
}
fn prepare_inner(source: &Path, expected: Option<(u64, &str)>) -> Result<PreparedVideo, String> {
    let meta = fs::metadata(source).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_BYTES {
        return Err("Video must be a file no larger than 128 MiB".into());
    }
    let (input, mut file) = temp("input")?;
    let copied = std::io::copy(
        &mut File::open(source)
            .map_err(|e| e.to_string())?
            .take(MAX_BYTES + 1),
        &mut file,
    )
    .map_err(|e| e.to_string())?;
    if copied == 0 || copied > MAX_BYTES {
        return Err("Video exceeds 128 MiB".into());
    }
    drop(file);
    if let Some((size, digest)) = expected {
        let mut hash = Sha256::new();
        std::io::copy(
            &mut File::open(&input.0).map_err(|e| e.to_string())?,
            &mut hash,
        )
        .map_err(|e| e.to_string())?;
        if copied != size || format!("{:x}", hash.finalize()) != digest {
            return Err("Video changed since it was selected".into());
        }
    }
    let (width, height, duration) = probe(&input.0)?;
    let (output, file) = temp("mp4")?;
    drop(file);
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(&input.0)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-c:v",
            "copy",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
        ])
        .arg(&output.0);
    run(cmd, Duration::from_secs(90))?;
    let size = fs::metadata(&output.0).map_err(|e| e.to_string())?.len();
    if size == 0 || size > MAX_BYTES {
        return Err("Prepared video exceeds size limit".into());
    }
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Video")
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect();
    Ok(PreparedVideo {
        file: output,
        width,
        height,
        duration,
        name,
    })
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn safe(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
fn dir(root: &Path) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let p = root.join("video-backgrounds");
    match fs::create_dir(&p) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    let m = fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
    if !m.is_dir()
        || !safe(&m)
        || p.canonicalize().map_err(|e| e.to_string())?.parent() != Some(root.as_path())
    {
        return Err("Unsafe video background directory".into());
    }
    Ok(p)
}
pub fn commit(root: &Path, prepared: PreparedVideo) -> Result<Value, String> {
    let d = dir(root)?;
    let mut f = File::open(&prepared.file.0).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    std::io::copy(&mut f, &mut hash).map_err(|e| e.to_string())?;
    let id = format!("{:x}", hash.finalize());
    let path = d.join(format!("{id}.mp4"));
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut out) => {
            if let Err(e) = std::io::copy(
                &mut File::open(&prepared.file.0).map_err(|e| e.to_string())?,
                &mut out,
            ) {
                drop(out);
                let _ = fs::remove_file(&path);
                return Err(e.to_string());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let m = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if !m.is_file() || !safe(&m) || m.len() == 0 || m.len() > MAX_BYTES {
                return Err("Unsafe existing video background".into());
            }
            let mut hash = Sha256::new();
            let count = std::io::copy(
                &mut File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(MAX_BYTES + 1),
                &mut hash,
            )
            .map_err(|e| e.to_string())?;
            if count != m.len() || format!("{:x}", hash.finalize()) != id {
                return Err("Existing video background does not match its ID".into());
            }
        }
        Err(e) => return Err(e.to_string()),
    }
    let metadata = json!({"id":id,"name":prepared.name,"codec":"h264","width":prepared.width,"height":prepared.height,"duration":prepared.duration});
    let meta_path = d.join(format!("{id}.json"));
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(meta_path)
    {
        Ok(mut f) => f
            .write_all(metadata.to_string().as_bytes())
            .map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    get(root, &id)
}
pub fn get(root: &Path, id: &str) -> Result<Value, String> {
    if !valid_id(id) {
        return Err("Invalid video background ID".into());
    }
    let d = dir(root)?;
    let path = d.join(format!("{id}.mp4"));
    for p in [&path, &d.join(format!("{id}.json"))] {
        let m = fs::symlink_metadata(p).map_err(|e| e.to_string())?;
        if !m.is_file() || !safe(&m) || m.len() == 0 || m.len() > MAX_BYTES {
            return Err("Unsafe video background file".into());
        }
    }
    let mut bytes = Vec::new();
    File::open(d.join(format!("{id}.json")))
        .map_err(|e| e.to_string())?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("Invalid video metadata".into());
    }
    let mut v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let width = v["width"].as_u64().unwrap_or(0);
    let height = v["height"].as_u64().unwrap_or(0);
    let duration = v["duration"].as_f64().unwrap_or(0.0);
    if v["id"].as_str() != Some(id)
        || v["codec"].as_str() != Some("h264")
        || width == 0
        || width > 3840
        || height == 0
        || height > 2160
        || !duration.is_finite()
        || duration <= 0.0
        || duration > 120.0
    {
        return Err("Invalid video metadata".into());
    }
    v["path"] = json!(path);
    Ok(v)
}
pub fn catalogue(root: &Path) -> Result<Value, String> {
    let d = dir(root)?;
    let mut ids = Vec::new();
    for item in fs::read_dir(d).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        if let Some(id) = item
            .file_name()
            .to_str()
            .and_then(|s| s.strip_suffix(".json"))
            && valid_id(id)
        {
            ids.push(id.to_owned());
        }
    }
    ids.sort();
    let mut items = Vec::new();
    for id in ids {
        items.push(get(root, &id)?);
    }
    Ok(json!(items))
}

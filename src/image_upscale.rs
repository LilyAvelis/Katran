//! Bounded one-shot image upscaling through the locally supplied Python runtime.
use crate::{ApiError, Result, domain_jobs::Cancellation};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const WORKER: &str = include_str!("../scripts/image-upscale/worker.py");
const MAX_INPUT: usize = 64 * 1024 * 1024;
const MAX_OUTPUT: u64 = 256 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct StagedUpscale {
    dir: PathBuf,
    path: PathBuf,
    pub(crate) input_width: u32,
    pub(crate) input_height: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}
impl StagedUpscale {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for StagedUpscale {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn reserve(root: &Path) -> Result<PathBuf> {
    let m = fs::symlink_metadata(root)?;
    let mut safe = m.is_dir() && !m.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        safe &= m.file_attributes() & 0x400 == 0;
    }
    if !safe {
        return Err(ApiError::new(
            "upscale_error",
            "Unsafe upscale staging directory",
        ));
    }
    let root = root.canonicalize()?;
    for _ in 0..128 {
        let dir = root.join(format!(
            "upscale-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(ApiError::new(
        "upscale_error",
        "Could not reserve upscale staging",
    ))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}

fn read_receipt(path: &Path) -> Result<serde_json::Value> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(ApiError::new("upscale_error", "Invalid upscale receipt"));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new("upscale_error", "Invalid upscale receipt"))
}

/// `source` is the caller's immutable validated snapshot; staged PNG is deleted on drop.
pub(crate) fn upscale(
    source: &[u8],
    staging_root: &Path,
    python: &Path,
    model: &Path,
    factor: u32,
    cancel: &Cancellation,
) -> Result<StagedUpscale> {
    cancel.check()?;
    if source.is_empty() || source.len() > MAX_INPUT {
        return Err(ApiError::new(
            "upscale_error",
            "Image input exceeds the size limit",
        ));
    }
    if !matches!(factor, 2 | 4) {
        return Err(ApiError::new(
            "upscale_error",
            "Upscale factor must be 2 or 4",
        ));
    }
    if !python.is_absolute() || !python.is_file() || !model.is_absolute() || !model.is_file() {
        return Err(ApiError::new(
            "upscale_error",
            "Image model or Python runtime is missing",
        ));
    }
    let dir = reserve(staging_root)?;
    let mut staged = StagedUpscale {
        path: dir.join("output.png"),
        dir,
        input_width: 0,
        input_height: 0,
        width: 0,
        height: 0,
    };
    let input = staged.dir.join("input");
    let script = staged.dir.join("worker.py");
    let receipt = staged.dir.join("result.json");
    write_new(&input, source)?;
    write_new(&script, WORKER.as_bytes())?;
    cancel.check()?;
    let mut command = Command::new(python);
    command
        .arg(&script)
        .arg(&input)
        .arg(&staged.path)
        .arg(&receipt)
        .arg(model)
        .arg(factor.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| ApiError::new("upscale_error", format!("Cannot start image worker: {e}")))?;
    let start = Instant::now();
    loop {
        if let Err(e) = cancel.check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        if fs::metadata(&staged.path)
            .ok()
            .is_some_and(|m| m.len() > MAX_OUTPUT)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ApiError::new(
                "upscale_error",
                "Upscaled image exceeds size limit",
            ));
        }
        match child.try_wait() {
            Ok(Some(s)) if s.success() => break,
            Ok(Some(_)) => {
                let detail = read_receipt(&receipt)
                    .ok()
                    .and_then(|value| value["error"].as_str().map(str::to_owned));
                return Err(ApiError::new(
                    "upscale_error",
                    detail.unwrap_or_else(|| {
                        "Image worker failed; check image, model, and local runtime".into()
                    }),
                ));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ApiError::new("upscale_error", e.to_string()));
            }
            Ok(None) if start.elapsed() >= Duration::from_secs(1800) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ApiError::new("upscale_error", "Image upscale timed out"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    cancel.check()?;
    let value = read_receipt(&receipt)?;
    let dimension = |name: &str| -> Result<u32> {
        value[name]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or_else(|| ApiError::new("upscale_error", "Invalid upscale dimensions"))
    };
    staged.input_width = dimension("input_width")?;
    staged.input_height = dimension("input_height")?;
    staged.width = dimension("width")?;
    staged.height = dimension("height")?;
    if staged.input_width.checked_mul(factor) != Some(staged.width)
        || staged.input_height.checked_mul(factor) != Some(staged.height)
        || staged.width > 8192
        || staged.height > 8192
        || u64::from(staged.width) * u64::from(staged.height) > 32_000_000
    {
        return Err(ApiError::new("upscale_error", "Invalid upscale dimensions"));
    }
    let m = fs::metadata(&staged.path)?;
    if m.len() < 24 || m.len() > MAX_OUTPUT {
        return Err(ApiError::new(
            "upscale_error",
            "Invalid upscale output size",
        ));
    }
    let mut header = [0u8; 24];
    fs::File::open(&staged.path)?.read_exact(&mut header)?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n"
        || &header[12..16] != b"IHDR"
        || u32::from_be_bytes(header[16..20].try_into().unwrap()) != staged.width
        || u32::from_be_bytes(header[20..24].try_into().unwrap()) != staged.height
    {
        return Err(ApiError::new("upscale_error", "Invalid upscale PNG"));
    }
    Ok(staged)
}

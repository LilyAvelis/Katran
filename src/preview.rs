//! Versioned, bounded persistent thumbnail cache; source bytes are never modified.
use crate::{ApiError, Event, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
pub fn content_hash(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
pub fn revision(path: &Path, hash: Option<&str>) -> String {
    if let Some(hash) = hash {
        return format!("blob:{hash}");
    }
    let metadata = fs::metadata(path).ok();
    let stamp = metadata
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos());
    format!(
        "file:{:x}",
        Sha256::digest(format!(
            "{}:{:?}:{:?}",
            path.display(),
            metadata.as_ref().map(|m| m.len()),
            stamp
        ))
    )
}
pub fn render(root: &Path, event: &Event, edge: u32) -> Result<serde_json::Value> {
    if !(32..=2048).contains(&edge) {
        return Err(ApiError::new(
            "invalid_argument",
            "max_edge must be 32..2048",
        ));
    }
    if !event.path.is_file() {
        return Err(ApiError::new(
            "preview_unavailable",
            "Source file unavailable",
        ));
    }
    if let Some(expected) = event.revision.strip_prefix("ref:")
        && content_hash(&event.path)? != expected
    {
        return Err(ApiError::new(
            "source_changed",
            "Source contents changed since this arrival; copy the current file again",
        ));
    }
    let directory = root.join("previews");
    fs::create_dir_all(&directory)?;
    if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        return Err(ApiError::new(
            "integrity_error",
            "Preview cache directory is a symbolic link",
        ));
    }
    let key = format!(
        "{:x}",
        Sha256::digest(format!("v1:{}:{edge}", event.revision))
    );
    let cache = directory.join(format!("{key}.png"));
    let mut hit = false;
    let image = match image::ImageReader::open(&cache)
        .ok()
        .and_then(|r| r.decode().ok())
    {
        Some(image) => {
            hit = true;
            image.to_rgba8()
        }
        None => {
            let mut reader = image::ImageReader::open(&event.path)?.with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(256 * 1024 * 1024);
            reader.limits(limits);
            let decoded = reader
                .decode()
                .map_err(|e| ApiError::new("preview_unavailable", e))?;
            let thumb = decoded.thumbnail(edge, edge).to_rgba8();
            let current = if event.revision.starts_with("ref:") {
                format!("ref:{}", content_hash(&event.path)?)
            } else {
                revision(
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
                    "Source changed during preview",
                ));
            }
            let staging = directory.join(format!(
                "{key}-{}-{}.tmp",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            if thumb
                .save_with_format(&staging, image::ImageFormat::Png)
                .is_ok()
                && fs::rename(&staging, &cache).is_err()
            {
                let _ = fs::remove_file(&staging);
            }
            thumb
        }
    };
    // Keep a fixed 128 MiB on-disk cache. The newest response remains in memory.
    let mut files: Vec<_> = fs::read_dir(&directory)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            if e.path().extension().and_then(|s| s.to_str()) != Some("png") {
                return None;
            }
            let m = e.metadata().ok()?;
            if !m.is_file() {
                return None;
            }
            Some((e.path(), m.len(), m.modified().ok()))
        })
        .collect();
    let mut size: u64 = files.iter().map(|e| e.1).sum();
    files.sort_by_key(|e| e.2);
    for (path, len, _) in files {
        if size <= 128 * 1024 * 1024 {
            break;
        }
        if fs::remove_file(path).is_ok() {
            size = size.saturating_sub(len);
        }
    }
    Ok(
        serde_json::json!({"width":image.width(),"height":image.height(),"revision":event.revision,"cache_hit":hit,"rgba_base64":base64::engine::general_purpose::STANDARD.encode(image.into_raw())}),
    )
}

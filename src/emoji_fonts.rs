//! Profile-local emoji fonts. Never installs or replaces Windows fonts.
use crate::{ApiError, Result};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[path = "bundled_emoji.rs"]
mod bundled_emoji;

const MAX_BYTES: u64 = 384 * 1024 * 1024;
pub fn valid_id(id: &str) -> bool {
    matches!(id, "apple" | "system")
        || (id.len() == 64
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
fn directory(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let dir = root.join("emoji-fonts");
    fs::create_dir_all(&dir)?;
    let meta = fs::symlink_metadata(&dir)?;
    if !meta.is_dir() || is_link(&meta) || dir.canonicalize()?.parent() != Some(root.as_path()) {
        return Err(ApiError::new(
            "unsafe_font",
            "Emoji font directory is not profile-local",
        ));
    }
    Ok(dir)
}
fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}
pub fn resolve(root: &Path, id: &str) -> Result<Value> {
    if !valid_id(id) {
        return Err(ApiError::new("invalid_font", "Unknown emoji font"));
    }
    if id == "system" {
        return Ok(
            json!({"id":id,"label":"Windows · Segoe UI Emoji","available":true,"path":null}),
        );
    }
    let dir = directory(root)?;
    let mut path = dir.join(format!("{id}.ttf"));
    let mut available = fs::symlink_metadata(&path)
        .is_ok_and(|m| m.is_file() && !is_link(&m) && m.len() > 0 && m.len() <= MAX_BYTES);
    if id == "apple"
        && !available
        && let Some(bundled) = bundled_emoji::materialize(&dir)?
    {
        path = bundled;
        available = true;
    }
    let label = if id == "apple" {
        "Apple · macOS".into()
    } else {
        format!("Свой шрифт · {}", &id[..8])
    };
    Ok(
        json!({"id":id,"label":label,"available":available,"path":if available {Some(path)} else {None}}),
    )
}
pub fn catalogue(root: &Path, selected: &str) -> Result<Value> {
    let mut fonts = vec![resolve(root, "apple")?, resolve(root, "system")?];
    let mut ids = Vec::new();
    for entry in fs::read_dir(directory(root)?)?.take(256) {
        let entry = entry?;
        if let Some(id) = entry
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            && id.len() == 64
            && valid_id(&id)
            && entry.path().extension().is_some_and(|e| e == "ttf")
        {
            ids.push(id);
        }
    }
    ids.sort();
    ids.dedup();
    for id in ids.into_iter().take(32) {
        fonts.push(resolve(root, &id)?);
    }
    Ok(json!({"fonts":fonts,"selected":selected}))
}
pub(crate) struct PreparedFont {
    bytes: Vec<u8>,
    id: String,
    family: String,
}
pub(crate) fn prepare(path: &Path) -> Result<PreparedFont> {
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size == 0 || size > MAX_BYTES || !file.metadata()?.is_file() {
        return Err(ApiError::new(
            "invalid_font",
            "Font must be a regular file up to 384 MiB",
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err(ApiError::new(
            "source_changed",
            "Font changed while reading",
        ));
    }
    // Validate the exact bytes that will be committed, not a subsequently replaced source.
    let tmp = tempfile_for_validation(&bytes)?;
    let family =
        crate::emoji_render::probe_font(&tmp.0).map_err(|e| ApiError::new("invalid_font", e))?;
    let id = format!("{:x}", Sha256::digest(&bytes));
    Ok(PreparedFont { bytes, id, family })
}
struct ValidationFile(PathBuf);
impl Drop for ValidationFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn tempfile_for_validation(bytes: &[u8]) -> Result<ValidationFile> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "katran-font-{}-{}.ttf",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let guard = ValidationFile(path);
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(guard)
}
pub(crate) fn commit(root: &Path, font: PreparedFont) -> Result<Value> {
    let path = directory(root)?.join(format!("{}.ttf", font.id));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(&font.bytes).and_then(|_| file.sync_all()) {
                let _ = fs::remove_file(&path);
                return Err(e.into());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() || is_link(&meta) || meta.len() != font.bytes.len() as u64 {
                return Err(ApiError::new("unsafe_font", "Existing font differs"));
            }
            let mut input = fs::File::open(&path)?;
            let mut digest = Sha256::new();
            std::io::copy(&mut input, &mut digest)?;
            if format!("{:x}", digest.finalize()) != font.id {
                return Err(ApiError::new("unsafe_font", "Existing font differs"));
            }
        }
        Err(e) => return Err(e.into()),
    }
    Ok(json!({"id":font.id,"label":font.family,"path":path}))
}

//! Optional embedded private font, extracted only inside the profile.
use crate::Result;
use std::path::{Path, PathBuf};

#[cfg(not(katran_bundled_apple))]
pub(super) fn materialize(_: &Path) -> Result<Option<PathBuf>> {
    Ok(None)
}

#[cfg(katran_bundled_apple)]
pub(super) fn materialize(dir: &Path) -> Result<Option<PathBuf>> {
    use crate::ApiError;
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, OpenOptions},
        io::{Read, Write},
        sync::atomic::{AtomicU64, Ordering},
    };
    const EXPECTED: &str = "88cb60b1b6eb49946d80059c1e109e204b5824d2b10fd18fa393423ce4e85fb7";
    const SIZE: u64 = 66_360_392;
    const BUNDLE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/emoji-bundle/apple.zip"
    ));
    fn error(msg: impl ToString) -> ApiError {
        ApiError::new("invalid_font", msg.to_string())
    }
    fn valid(path: &Path) -> Result<bool> {
        let meta = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        if !meta.is_file() || super::is_link(&meta) || meta.len() != SIZE {
            return Err(error("Bundled emoji font path is occupied or invalid"));
        }
        let mut f = fs::File::open(path)?;
        let mut hash = Sha256::new();
        std::io::copy(&mut f, &mut hash)?;
        if format!("{:x}", hash.finalize()) != EXPECTED {
            return Err(error("Bundled emoji font failed integrity check"));
        }
        Ok(true)
    }
    let target = dir.join("apple-bundled96.ttf");
    if valid(&target)? {
        return Ok(Some(target));
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(BUNDLE)).map_err(error)?;
    if archive.len() != 1 {
        return Err(error("Invalid bundled emoji archive"));
    }
    let mut entry = archive.by_index(0).map_err(error)?;
    if entry.name().map_err(error)? != "apple-bundled96.ttf" || entry.size() != SIZE {
        return Err(error("Invalid bundled emoji resource"));
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = dir.join(format!(
        ".apple-bundled96-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(temp.clone());
    let mut digest = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = entry.read(&mut buffer).map_err(error)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .ok_or_else(|| error("Bundled font too large"))?;
        if count > SIZE {
            return Err(error("Bundled font too large"));
        }
        digest.update(&buffer[..n]);
        out.write_all(&buffer[..n])?;
    }
    out.sync_all()?;
    drop(out);
    if count != SIZE || format!("{:x}", digest.finalize()) != EXPECTED {
        return Err(error("Bundled font integrity check failed"));
    }
    crate::emoji_render::probe_font(&temp).map_err(error)?;
    match fs::hard_link(&temp, &target) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            valid(&target)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(Some(target))
}

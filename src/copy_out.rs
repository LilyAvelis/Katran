//! Windows file clipboard plus full-size pixels for a single supported image.
use image::ImageDecoder;
use std::cell::Cell;
use std::io::Read;
use std::path::{Path, PathBuf};

fn shell_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        PathBuf::from(format!("\\\\{unc}"))
    } else if let Some(path) = text.strip_prefix("\\\\?\\") {
        PathBuf::from(path)
    } else {
        path.to_path_buf()
    }
}

#[cfg(windows)]
fn file_drop(paths: &[PathBuf]) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    let mut bytes = vec![0u8; 20];
    bytes[..4].copy_from_slice(&20u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    for path in paths {
        for unit in shell_path(path).as_os_str().encode_wide().chain(Some(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn image_dib(path: &Path) -> Result<Vec<u8>, String> {
    let mut decoder = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if width as u64 * height as u64 > 32_000_000 {
        return Err("Image exceeds clipboard pixel budget; files copied instead".into());
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    decoder.set_limits(limits).map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    let rgba = image.into_rgba8();
    Ok(dib(rgba.width(), rgba.height(), rgba.as_raw()))
}

fn dib(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; 124]; // BITMAPV5HEADER, top-down BGRA, explicit alpha.
    for (offset, value) in [
        (0, 124),
        (4, width),
        (8, (-(height as i32)) as u32),
        (16, 3),
        (20, rgba.len() as u32),
        (40, 0x00ff0000),
        (44, 0x0000ff00),
        (48, 0x000000ff),
        (52, 0xff000000),
        (56, 0x73524742),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&32u16.to_le_bytes());
    bytes.reserve(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        bytes.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    bytes
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ItemFailure {
    pub id: i64,
    pub error: String,
}
fn active_plans() -> &'static std::sync::Mutex<std::collections::HashSet<PathBuf>> {
    static ACTIVE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
        std::sync::OnceLock::new();
    ACTIVE.get_or_init(Default::default)
}
#[derive(Debug)]
pub struct Plan {
    pub paths: Vec<PathBuf>,
    pub failures: Vec<ItemFailure>,
    directory: PathBuf,
    previous: Vec<PathBuf>,
    published: Cell<bool>,
    attempted: Cell<bool>,
}
impl Plan {
    /// Replace a fully verified link-only snapshot batch with one owned UTF-8 file.
    pub(crate) fn group_links(&mut self, quota: u64) -> Result<(), String> {
        self.group_text(quota, "links.txt", "\n", |text| {
            crate::link_fragments::recognize(text).is_some()
        })
    }
    /// Replace a fully verified Pods-only batch with one exact UTF-8 sequence.
    pub(crate) fn group_pods(&mut self, quota: u64) -> Result<(), String> {
        self.group_text(quota, "pods.txt", "", crate::pods::is_pod)
    }
    fn group_text(
        &mut self,
        quota: u64,
        filename: &str,
        separator: &str,
        valid: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        use std::io::Write;
        if !self.failures.is_empty() || self.paths.is_empty() {
            return Err(format!(
                "Cannot group text: {}",
                serde_json::to_string(&self.failures).unwrap_or_default()
            ));
        }
        let mut urls = Vec::with_capacity(self.paths.len());
        let mut total = 0usize;
        for path in &self.paths {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .map_err(|e| e.to_string())?
                .take((crate::text_fragments::MAX_TEXT_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > crate::text_fragments::MAX_TEXT_BYTES {
                return Err("Text snapshot exceeds 1 MiB".into());
            }
            let url = String::from_utf8(bytes).map_err(|e| e.to_string())?;
            if !valid(&url) {
                return Err("Invalid grouped text snapshot".into());
            }
            total = total
                .checked_add(url.len())
                .and_then(|n| n.checked_add(if urls.is_empty() { 0 } else { separator.len() }))
                .ok_or("Text export overflow")?;
            if total > crate::text_fragments::MAX_MERGED_BYTES {
                return Err("Text export exceeds 8 MiB".into());
            }
            urls.push(url);
        }
        let mut other_bytes = 0u64;
        let exports = self.directory.parent().ok_or("Invalid export directory")?;
        for batch in std::fs::read_dir(exports).map_err(|e| e.to_string())? {
            let batch = batch.map_err(|e| e.to_string())?;
            if batch.path() == self.directory
                || !batch.file_name().to_string_lossy().starts_with("copy-")
            {
                continue;
            }
            if !batch.file_type().map_err(|e| e.to_string())?.is_dir() {
                return Err("Unexpected entry in owned export batches".into());
            }
            for file in std::fs::read_dir(batch.path()).map_err(|e| e.to_string())? {
                let file = file.map_err(|e| e.to_string())?;
                if !file.file_type().map_err(|e| e.to_string())?.is_file() {
                    return Err("Unexpected entry in export batch".into());
                }
                other_bytes = other_bytes
                    .checked_add(file.metadata().map_err(|e| e.to_string())?.len())
                    .ok_or("Export size overflow")?;
            }
        }
        if other_bytes.saturating_add(total as u64) > quota {
            return Err("Export quota exceeded".into());
        }
        let destination = self.directory.join(filename);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|e| e.to_string())?;
        file.write_all(urls.join(separator).as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        for path in &self.paths {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        self.paths = vec![destination.canonicalize().map_err(|e| e.to_string())?];
        Ok(())
    }
    /// An external application may read the path asynchronously or keep editing
    /// it. Open-file snapshots have their own bounded cache and never retire
    /// previously handed-off files on a later open.
    pub(crate) fn retain_without_retiring(&self) {
        self.published.set(true);
    }
    /// Legacy references may point into an older export batch. Their lifetime is
    /// governed by shelf records, so clipboard rotation must retain that batch.
    /// Call with all stored external reference paths before publishing. Retained
    /// batches remain included in subsequent export quota accounting.
    pub fn protect_references(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        if self.attempted.get() {
            return Err("Copy plan already consumed".into());
        }
        let mut references = Vec::with_capacity(paths.len());
        for path in paths {
            match path.canonicalize() {
                Ok(path) => references.push(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("Cannot verify reference protection: {error}")),
            }
        }
        let mut disposable = Vec::with_capacity(self.previous.len());
        for batch in &self.previous {
            let canonical = batch
                .canonicalize()
                .map_err(|error| format!("Cannot verify export batch protection: {error}"))?;
            if !references.iter().any(|path| path.starts_with(&canonical)) {
                disposable.push(batch.clone());
            }
        }
        self.previous = disposable;
        Ok(())
    }
}
impl Drop for Plan {
    fn drop(&mut self) {
        active_plans()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&self.directory);
        if !self.published.get() {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
#[derive(Debug, serde::Serialize)]
pub struct CopyReceipt {
    pub count: usize,
    pub failures: Vec<ItemFailure>,
    pub warnings: Vec<String>,
    pub sequence: u32,
}
/// Separate owned-export budget, including the still-pasteable prior batch.
pub const DEFAULT_EXPORT_QUOTA: u64 = 2 * 1024 * 1024 * 1024;

/// Preparation never touches the clipboard. Startup deliberately preserves exports:
/// clipboard history is unknown. Only a later successful Katran copy retires prior
/// batches. All payload paths are independent owned snapshots, including references.
/// One preparation writer per root is required; live plans are protected from
/// cleanup while other serialized preparation results await dispatcher commit.
pub fn prepare(root: &Path, events: &[crate::Event]) -> Result<Plan, String> {
    prepare_with_budget(root, events, DEFAULT_EXPORT_QUOTA)
}

pub fn prepare_with_budget(
    root: &Path,
    events: &[crate::Event],
    quota: u64,
) -> Result<Plan, String> {
    if events.is_empty() {
        return Err("No files selected".into());
    }
    let exports = root.join("exports");
    std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
    if std::fs::symlink_metadata(&exports)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Export directory is a symbolic link".into());
    }
    let mut previous = vec![];
    let mut used = 0u64;
    for entry in std::fs::read_dir(&exports).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        // Never traverse links or remove unrelated files/directories.
        if !entry.file_name().to_string_lossy().starts_with("copy-") {
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            return Err("Unexpected entry in owned export batches".into());
        }
        for file in std::fs::read_dir(entry.path()).map_err(|e| e.to_string())? {
            let file = file.map_err(|e| e.to_string())?;
            if !file.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("Unexpected entry in owned export batch".into());
            }
            used = used
                .checked_add(file.metadata().map_err(|e| e.to_string())?.len())
                .ok_or("Export size overflow")?;
        }
        previous.push(entry.path());
    }
    if used > quota {
        return Err("Export quota exceeded; prior clipboard exports retained".into());
    }
    let mut n = 0u64;
    let directory = loop {
        let p = exports.join(format!(
            "copy-{}-{}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        match std::fs::create_dir(&p) {
            Ok(()) => break p,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e.to_string()),
        }
    };
    active_plans()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(directory.clone());
    let mut plan = Plan {
        paths: vec![],
        failures: vec![],
        directory: directory.clone(),
        previous,
        published: Cell::new(false),
        attempted: Cell::new(false),
    };
    for (index, event) in events.iter().enumerate() {
        let mut attempted_path = None;
        let result = (|| -> Result<PathBuf, String> {
            if !event.path.is_file() {
                return Err("File unavailable".into());
            }
            let folder_file =
                event.id < 0 && event.source == "work_folder" && event.storage == "reference";
            let folder_digest = if folder_file {
                if std::fs::metadata(&event.path)
                    .map_err(|e| e.to_string())?
                    .len()
                    > quota - used
                {
                    return Err("Export quota exceeded".into());
                }
                if !event.revision.starts_with("file:") {
                    return Err("source_changed: folder file has no metadata revision".into());
                }
                crate::open_file::validate(event)
                    .map_err(|e| format!("source_changed: {}", e.message))?;
                Some(bounded_content_digest(&event.path, quota - used)?)
            } else {
                None
            };
            let expected = if event.storage == "cached" {
                event
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|n| n.len() == 64 && n.bytes().all(|b| b.is_ascii_hexdigit()))
            } else if folder_file {
                folder_digest.as_deref()
            } else {
                Some(event.revision.strip_prefix("ref:").ok_or(
                    "source_changed: reference has no verified content identity; ingest it again",
                )?)
            };
            let name: String = event
                .name
                .chars()
                .map(|c| {
                    if c.is_control() || "<>:\"/\\|?*".contains(c) {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            let name = name.trim_end_matches([' ', '.']);
            let name = if name.is_empty() { "file" } else { name };
            // Independent bytes: a hard link would let paste recipients mutate
            // the content-addressed blob. Bound the actual read, not event.size.
            let source = std::fs::File::open(&event.path).map_err(|e| e.to_string())?;
            let remaining = quota - used;
            let length = source.metadata().map_err(|e| e.to_string())?.len();
            if length > remaining {
                return Err("Export quota exceeded".into());
            }
            let base = if folder_file {
                name.to_owned()
            } else {
                format!("{index}-{name}")
            };
            let (destination, mut output) = {
                let path = Path::new(&base);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| format!(".{s}"))
                    .unwrap_or_default();
                let mut found = None;
                for suffix in 0..10_000 {
                    let filename = if suffix == 0 {
                        base.clone()
                    } else {
                        format!("{stem} ({suffix}){ext}")
                    };
                    let candidate = directory.join(filename);
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&candidate)
                    {
                        Ok(file) => {
                            found = Some((candidate, file));
                            break;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(error) => return Err(error.to_string()),
                    }
                }
                found.ok_or("No available snapshot filename")?
            };
            attempted_path = Some(destination.clone());
            let copied = std::io::copy(&mut source.take(remaining.saturating_add(1)), &mut output)
                .map_err(|e| e.to_string())?;
            if copied > remaining {
                return Err("Export quota exceeded".into());
            }
            output.sync_all().map_err(|e| e.to_string())?;
            if let Some(expected) = expected
                && content_digest(&destination)? != expected
            {
                return Err(if event.storage == "cached" {
                    "Cached content failed integrity check; nothing was copied for this item".into()
                } else {
                    "source_changed: reference content differs from the shelf identity; ingest it again".into()
                });
            }
            if folder_file {
                crate::open_file::validate(event)
                    .map_err(|e| format!("source_changed: {}", e.message))?;
            }
            used += copied;
            destination.canonicalize().map_err(|e| e.to_string())
        })();
        if result.is_err()
            && let Some(path) = attempted_path
            && let Err(error) = std::fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!("Failed export cleanup: {error}"));
        }
        match result {
            Ok(path) => plan.paths.push(path),
            Err(error) if error == "Export quota exceeded" => return Err(error),
            Err(error) => plan.failures.push(ItemFailure {
                id: event.id,
                error,
            }),
        }
    }
    Ok(plan)
}
fn content_digest(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut input = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = input.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn bounded_content_digest(path: &Path, limit: u64) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut input = std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit.saturating_add(1));
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut bytes = [0u8; 65536];
    loop {
        let count = input.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > limit {
            return Err("Export quota exceeded".into());
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A sink must return success only after the primary file payload is accepted.
pub trait ClipboardSink {
    fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String>;
}
pub fn commit(plan: &Plan, sink: &mut impl ClipboardSink) -> Result<CopyReceipt, String> {
    if plan.attempted.replace(true) {
        return Err("Copy plan already consumed".into());
    }
    if plan.paths.is_empty() {
        let _ = std::fs::remove_dir_all(&plan.directory);
        return Err(format!(
            "No available files; {} item failures",
            plan.failures.len()
        ));
    }
    let (sequence, mut warnings) = match sink.write(&plan.paths) {
        Ok(result) => result,
        Err(error) => {
            return match std::fs::remove_dir_all(&plan.directory) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; export cleanup pending: {cleanup}")),
            };
        }
    };
    // The primary clipboard payload is now accepted. Shelf deletion and Plan's
    // destructor must preserve these paths until the next successful copy.
    plan.published.set(true);
    for previous in &plan.previous {
        if active_plans()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .contains(previous)
        {
            continue;
        }
        if let Err(error) = std::fs::remove_dir_all(previous) {
            warnings.push(format!("Files copied; old export cleanup pending: {error}"));
        }
    }
    Ok(CopyReceipt {
        count: plan.paths.len(),
        failures: plan.failures.clone(),
        warnings,
        sequence,
    })
}
/// Image decoding and HDROP encoding are preparation work, not publication work.
pub(crate) struct PreparedNativeClipboard {
    paths: Vec<PathBuf>,
    #[cfg(windows)]
    files: Vec<u8>,
    pixels: Option<Vec<u8>>,
    warnings: Vec<String>,
}
impl PreparedNativeClipboard {
    pub(crate) fn prepare(paths: &[PathBuf]) -> Self {
        let mut warnings = Vec::new();
        let pixels = if paths.len() == 1 {
            match image_dib(&paths[0]) {
                Ok(pixels) => Some(pixels),
                Err(error) => {
                    warnings.push(format!("Files copied; pixels unavailable: {error}"));
                    None
                }
            }
        } else {
            None
        };
        Self {
            paths: paths.to_vec(),
            #[cfg(windows)]
            files: file_drop(paths),
            pixels,
            warnings,
        }
    }
}
impl ClipboardSink for PreparedNativeClipboard {
    fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
        if paths != self.paths {
            return Err("Prepared clipboard payload does not match export plan".into());
        }
        #[cfg(windows)]
        {
            let mut warnings = std::mem::take(&mut self.warnings);
            let _clipboard =
                clipboard_win::Clipboard::new_attempts(20).map_err(|error| error.to_string())?;
            clipboard_win::raw::set(15, &self.files).map_err(|error| error.to_string())?;
            if let Some(pixels) = &self.pixels
                && let Err(error) = clipboard_win::raw::set_without_clear(17, pixels)
            {
                warnings.push(format!("Files copied; image clipboard failed: {error}"));
            }
            if let Some(format) = clipboard_win::raw::register_format("Preferred DropEffect")
                && let Err(error) =
                    clipboard_win::raw::set_without_clear(format.get(), &1u32.to_le_bytes())
            {
                warnings.push(format!("Copy effect unavailable: {error}"));
            }
            Ok((
                unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() },
                warnings,
            ))
        }
        #[cfg(not(windows))]
        {
            let _ = (&self.pixels, &self.warnings);
            Err("Native clipboard requires Windows".into())
        }
    }
}
pub struct NativeClipboard;
impl ClipboardSink for NativeClipboard {
    fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
        PreparedNativeClipboard::prepare(paths).write(paths)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(bool);
    impl ClipboardSink for Fake {
        fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
            assert_eq!(paths.len(), 1);
            if self.0 {
                Ok((42, vec![]))
            } else {
                Err("busy".into())
            }
        }
    }
    #[test]
    fn preparation_preserves_snapshot_and_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("blob");
        std::fs::write(&source, b"synthetic").unwrap();
        let event = |id, path| crate::Event {
            kind: Default::default(),
            text_preview: None,
            delete_policy: Default::default(),
            origin: Default::default(),
            revision: "synthetic".into(),
            id,
            detected_at_ms: 0,
            modified_at_ms: None,
            source: "test".into(),
            name: "photo.png".into(),
            storage: "cached".into(),
            path,
            size: 9,
            available: true,
        };
        let plan = prepare(
            dir.path(),
            &[
                event(1, source.clone()),
                event(2, dir.path().join("missing")),
            ],
        )
        .unwrap();
        assert_eq!(plan.failures.len(), 1);
        assert_eq!(plan.paths[0].extension().unwrap(), "png");
        std::fs::write(&plan.paths[0], b"external mutation").unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), b"synthetic");
        std::fs::write(&plan.paths[0], b"synthetic").unwrap();
        std::fs::remove_file(&source).unwrap();
        assert_eq!(std::fs::read(&plan.paths[0]).unwrap(), b"synthetic");
        assert!(commit(&plan, &mut Fake(false)).is_err());
        assert!(!plan.paths[0].exists());
        std::fs::write(&source, b"synthetic").unwrap();
        let plan = prepare(
            dir.path(),
            &[
                event(1, source.clone()),
                event(2, dir.path().join("missing")),
            ],
        )
        .unwrap();
        let receipt = commit(&plan, &mut Fake(true)).unwrap();
        assert_eq!(receipt.count, 1);
        assert_eq!(receipt.sequence, 42);
        assert_eq!(receipt.failures[0].id, 2);
    }
    fn cached_event(path: PathBuf) -> crate::Event {
        crate::Event {
            kind: Default::default(),
            text_preview: None,
            delete_policy: Default::default(),
            origin: Default::default(),
            revision: "synthetic".into(),
            id: 1,
            detected_at_ms: 0,
            modified_at_ms: None,
            source: "test".into(),
            name: "sample.bin".into(),
            storage: "cached".into(),
            path,
            size: 1,
            available: true,
        }
    }

    #[test]
    fn publication_never_reclaims_another_live_preparation() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.bin");
        std::fs::write(&source, b"fixture").unwrap();
        let event = cached_event(source);
        let first = prepare_with_budget(root.path(), std::slice::from_ref(&event), 100).unwrap();
        let first_path = first.paths[0].clone();
        let second = prepare_with_budget(root.path(), std::slice::from_ref(&event), 100).unwrap();
        let second_path = second.paths[0].clone();
        commit(&second, &mut Fake(true)).unwrap();
        assert_eq!(std::fs::read(&first_path).unwrap(), b"fixture");
        commit(&first, &mut Fake(true)).unwrap();
        assert_eq!(std::fs::read(&second_path).unwrap(), b"fixture");
        drop(first);
        drop(second);
        let third = prepare_with_budget(root.path(), &[event], 100).unwrap();
        commit(&third, &mut Fake(true)).unwrap();
        assert!(!first_path.exists());
        assert!(!second_path.exists());
    }
    #[test]
    fn budget_and_failed_publication_preserve_pending_paste() {
        let dir = tempfile::tempdir().unwrap();
        let blob = dir.path().join("blob");
        std::fs::write(&blob, b"12345").unwrap();
        let event = cached_event(blob.clone());
        let first = prepare_with_budget(dir.path(), std::slice::from_ref(&event), 10).unwrap();
        commit(&first, &mut Fake(true)).unwrap();
        let old = first.paths[0].clone();
        drop(first);
        // Event.size is deliberately inaccurate: actual bytes determine quota.
        assert!(prepare_with_budget(dir.path(), std::slice::from_ref(&event), 9).is_err());
        assert_eq!(std::fs::read(&old).unwrap(), b"12345");
        assert_eq!(
            std::fs::read_dir(dir.path().join("exports"))
                .unwrap()
                .count(),
            1
        );
        let failed = prepare_with_budget(dir.path(), std::slice::from_ref(&event), 10).unwrap();
        let failed_path = failed.paths[0].clone();
        assert!(commit(&failed, &mut Fake(false)).is_err());
        assert!(!failed_path.exists());
        assert_eq!(std::fs::read(&old).unwrap(), b"12345");
        let next = prepare_with_budget(dir.path(), &[event], 10).unwrap();
        struct PendingSink(PathBuf);
        impl ClipboardSink for PendingSink {
            fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
                // Prior paste remains possible until the new payload is accepted.
                assert_eq!(std::fs::read(&self.0).unwrap(), b"12345");
                assert_eq!(std::fs::read(&paths[0]).unwrap(), b"12345");
                Ok((7, vec![]))
            }
        }
        commit(&next, &mut PendingSink(old.clone())).unwrap();
        assert!(!old.exists());
        assert_eq!(std::fs::read(&next.paths[0]).unwrap(), b"12345");
        assert_eq!(std::fs::read(&blob).unwrap(), b"12345");
    }

    #[test]
    fn abandoned_plans_cleanup_and_reference_snapshots_are_budgeted() {
        let dir = tempfile::tempdir().unwrap();
        let blob = dir.path().join("source");
        std::fs::write(&blob, b"12345").unwrap();
        let mut event = cached_event(blob.clone());
        let plan = prepare_with_budget(dir.path(), std::slice::from_ref(&event), 5).unwrap();
        let abandoned = plan.paths[0].clone();
        drop(plan);
        assert!(!abandoned.exists());
        event.storage = "reference".into();
        event.revision = format!("ref:{}", content_digest(&blob).unwrap());
        assert!(prepare_with_budget(dir.path(), std::slice::from_ref(&event), 0).is_err());
        let plan = prepare_with_budget(dir.path(), &[event], 5).unwrap();
        assert_ne!(plan.paths, vec![blob.canonicalize().unwrap()]);
        commit(&plan, &mut Fake(true)).unwrap();
        assert_eq!(std::fs::read(&blob).unwrap(), b"12345");
    }

    #[test]
    fn legacy_reference_batches_survive_unrelated_copy_rotation_and_stay_budgeted() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_batch = dir.path().join("exports").join("copy-legacy");
        std::fs::create_dir_all(&legacy_batch).unwrap();
        let legacy_reference = legacy_batch.join("shelf-reference.bin");
        std::fs::write(&legacy_reference, b"legacy").unwrap();
        let source = dir.path().join("new.bin");
        std::fs::write(&source, b"new").unwrap();
        let event = cached_event(source);
        let mut plan = prepare_with_budget(dir.path(), std::slice::from_ref(&event), 9).unwrap();
        plan.protect_references(std::slice::from_ref(&legacy_reference))
            .unwrap();
        commit(&plan, &mut Fake(true)).unwrap();
        assert_eq!(std::fs::read(&legacy_reference).unwrap(), b"legacy");
        assert_eq!(std::fs::read(&plan.paths[0]).unwrap(), b"new");
        // The protected batch is still charged alongside the newest clipboard batch.
        assert!(prepare_with_budget(dir.path(), &[event], 11).is_err());
        assert_eq!(std::fs::read(&legacy_reference).unwrap(), b"legacy");
    }

    #[test]
    fn references_verify_content_and_pending_snapshots_ignore_later_changes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("external.bin");
        std::fs::write(&source, b"first").unwrap();
        let modified = std::fs::metadata(&source).unwrap().modified().unwrap();
        let mut event = cached_event(source.clone());
        event.storage = "reference".into();
        event.revision = format!("ref:{}", content_digest(&source).unwrap());
        let ready = prepare_with_budget(dir.path(), std::slice::from_ref(&event), 10).unwrap();
        commit(&ready, &mut Fake(true)).unwrap();
        std::fs::write(&source, b"other").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let changed = prepare_with_budget(dir.path(), &[event], 10).unwrap();
        assert!(changed.paths.is_empty());
        assert!(changed.failures[0].error.starts_with("source_changed:"));
        struct NeverCalled;
        impl ClipboardSink for NeverCalled {
            fn write(&mut self, _: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
                panic!("all-failed reference batch must never publish")
            }
        }
        assert!(commit(&changed, &mut NeverCalled).is_err());
        assert_eq!(std::fs::read(&ready.paths[0]).unwrap(), b"first");
        assert_eq!(std::fs::read(&source).unwrap(), b"other");
        std::fs::write(&ready.paths[0], b"paste").unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), b"other");
    }

    #[test]
    fn group_files_and_single_pixels_are_compatible_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.png");
        image::DynamicImage::new_rgb8(2, 1).save(&original).unwrap();
        let events: Vec<_> = (1..=3)
            .map(|id| crate::Event {
                kind: Default::default(),
                text_preview: None,
                delete_policy: Default::default(),
                origin: Default::default(),
                revision: format!("ref:{}", content_digest(&original).unwrap()),
                id,
                detected_at_ms: 0,
                modified_at_ms: None,
                source: "test".into(),
                name: "original.png".into(),
                storage: if id == 1 { "reference" } else { "cached" }.into(),
                path: original.clone(),
                size: 1,
                available: true,
            })
            .collect();
        let plan = prepare(dir.path(), &events).unwrap();
        assert_eq!(plan.paths.len(), 3);
        assert!(plan.failures.is_empty());
        struct GroupSink;
        impl ClipboardSink for GroupSink {
            fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
                assert_eq!(paths.len(), 3);
                for path in paths {
                    assert!(path.is_file());
                }
                Ok((10, vec![]))
            }
        }
        assert_eq!(commit(&plan, &mut GroupSink).unwrap().count, 3);
        let pixels = image_dib(&original).unwrap();
        assert_eq!(u32::from_le_bytes(pixels[4..8].try_into().unwrap()), 2);
        assert_eq!(pixels.len(), 124 + 8);
        #[cfg(windows)]
        {
            let files = file_drop(&plan.paths);
            assert_eq!(&files[files.len() - 4..], &[0, 0, 0, 0]);
            assert_eq!(u32::from_le_bytes(files[..4].try_into().unwrap()), 20);
        }
        assert!(original.is_file());
    }
}

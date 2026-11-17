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
    pub(crate) text_payload: Option<String>,
    directory: PathBuf,
    previous: Vec<PathBuf>,
    published: Cell<bool>,
    attempted: Cell<bool>,
}
impl Plan {
    /// Replace a fully verified link-only snapshot batch with one owned UTF-8 file.
    pub(crate) fn group_links(&mut self, quota: u64) -> Result<(), String> {
        self.group_text(quota, "links.txt", "\n", false, |text| {
            crate::link_fragments::recognize(text).is_some()
        })
    }
    /// Replace a fully verified Pods-only batch with one exact UTF-8 sequence.
    pub(crate) fn group_pods(&mut self, quota: u64) -> Result<(), String> {
        self.group_text(quota, "pods.txt", "", true, crate::pods::is_pod)
    }
    fn group_text(
        &mut self,
        quota: u64,
        filename: &str,
        separator: &str,
        offer_text: bool,
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
        let text = urls.join(separator);
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        for path in &self.paths {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        self.paths = vec![destination.canonicalize().map_err(|e| e.to_string())?];
        if offer_text {
            self.text_payload = Some(text);
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication: Option<PublicationFacts>,
}
/// Facts about accepted clipboard formats, never file names or content.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct PublicationFacts {
    pub formats: Vec<&'static str>,
    pub item_count: usize,
    pub optional_pixel_warning: bool,
    pub sequence: u32,
    pub sequence_verification: &'static str,
}
pub struct ClipboardWrite {
    pub sequence: u32,
    pub warnings: Vec<String>,
    pub publication: Option<PublicationFacts>,
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
        text_payload: None,
        directory: directory.clone(),
        previous,
        published: Cell::new(false),
        attempted: Cell::new(false),
    };
    for (index, event) in events.iter().enumerate() {
        let mut attempted_path = None;
        let result = (|| -> Result<PathBuf, String> {
            if crate::pods::is_emoji_id(event.id) {
                use std::io::Write;
                let text = crate::text_fragments::read_text_event(event).map_err(|e| e.message)?;
                if text.len() as u64 > quota - used {
                    return Err("Export quota exceeded".into());
                }
                let destination = directory.join(format!("emoji-{index}.txt"));
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)
                    .map_err(|e| e.to_string())?;
                attempted_path = Some(destination.clone());
                file.write_all(text.as_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|e| e.to_string())?;
                used += text.len() as u64;
                return destination.canonicalize().map_err(|e| e.to_string());
            }
            if !event.path.is_file() {
                return Err("File unavailable".into());
            }
            let folder_file = event.id < 0
                && matches!(event.source.as_str(), "work_folder" | "desktop")
                && event.storage == "reference";
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
            let display_name = if event.source == "desktop"
                && std::path::Path::new(&event.name).extension().is_none()
            {
                format!(
                    "{}.{}",
                    event.name,
                    event
                        .path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("bin")
                )
            } else {
                event.name.clone()
            };
            let name: String = display_name
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
    fn write_with_facts(&mut self, paths: &[PathBuf]) -> Result<ClipboardWrite, String> {
        let (sequence, warnings) = self.write(paths)?;
        Ok(ClipboardWrite {
            sequence,
            warnings,
            publication: None,
        })
    }
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
    let ClipboardWrite {
        sequence,
        mut warnings,
        publication,
    } = match sink.write_with_facts(&plan.paths) {
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
        // Without a settled sequence the current clipboard may belong to another
        // writer; preserve older batches as well as the just-published files.
        if sequence == 0 {
            continue;
        }
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
        publication,
    })
}
/// Image decoding and HDROP encoding are preparation work, not publication work.
#[cfg(any(windows, test))]
static FILE_OWNER_TOKENS: std::sync::Mutex<std::collections::VecDeque<String>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

#[cfg(any(windows, test))]
pub(crate) fn owns_file_token(token: &str) -> bool {
    FILE_OWNER_TOKENS
        .lock()
        .is_ok_and(|tokens| tokens.iter().any(|known| known == token))
}

#[cfg(test)]
pub(crate) fn remember_file_token_for_test(token: &str) {
    remember_file_token(&mut FILE_OWNER_TOKENS.lock().unwrap(), token.to_owned());
}

#[cfg(any(windows, test))]
fn remember_file_token(tokens: &mut std::collections::VecDeque<String>, token: String) {
    tokens.push_back(token);
    if tokens.len() > 64 {
        tokens.pop_front();
    }
}

#[cfg(any(windows, test))]
struct SettlementProof {
    owner_none: bool,
    formats_valid: bool,
    marker_valid: bool,
    payload_valid: bool,
    sequence_before: u32,
    sequence_after: u32,
    sequence_closed: u32,
}
#[cfg(any(windows, test))]
fn verified_settlement(proof: SettlementProof) -> Result<u32, &'static str> {
    if !proof.owner_none {
        return Err("owner_live");
    }
    if !proof.formats_valid {
        return Err("formats_changed");
    }
    if !proof.marker_valid {
        return Err("marker_missing");
    }
    if !proof.payload_valid {
        return Err("payload_changed");
    }
    if proof.sequence_before == 0
        || proof.sequence_before != proof.sequence_after
        || proof.sequence_before != proof.sequence_closed
    {
        return Err("sequence_unstable");
    }
    Ok(proof.sequence_before)
}
#[cfg(any(windows, test))]
fn accepted_formats(formats: &[u32], marker: u32, effect: Option<u32>, pixels: bool) -> bool {
    if marker == 0 || !formats.contains(&15) || !formats.contains(&marker) {
        return false;
    }
    if pixels && !formats.contains(&17) {
        return false;
    }
    if effect.is_some_and(|id| !formats.contains(&id)) {
        return false;
    }
    formats.iter().all(|id| {
        *id == 15
            || *id == marker
            || effect == Some(*id)
            || (pixels && matches!(*id, 2 | 8 | 9 | 17))
    })
}

#[cfg(any(windows, test))]
fn same_bounded_payload(expected: &[u8], observed: &[u8], allocation: usize) -> bool {
    allocation >= expected.len()
        && allocation <= expected.len().saturating_add(16)
        && observed == expected
}

#[cfg(any(windows, test))]
trait FilePublication {
    fn owner_valid(&self) -> bool;
    fn primary(&mut self) -> Result<(), String>;
    fn pixels(&mut self) -> Result<(), String>;
    fn effect(&mut self) -> Result<bool, String>;
    fn marker(&mut self) -> Result<Option<String>, String>;
    fn sequence(&self) -> u32;
    /// Complete publication and establish ownership of the settled sequence.
    fn finish(
        self,
        marker: Option<String>,
        pixels: bool,
        effect: bool,
    ) -> Result<u32, &'static str>;
}

#[cfg(any(windows, test))]
fn publish_file_formats<B: FilePublication>(
    mut backend: B,
    item_count: usize,
    has_pixels: bool,
    mut warnings: Vec<String>,
) -> Result<ClipboardWrite, String> {
    if !backend.owner_valid() {
        return Err("Clipboard owner window unavailable".into());
    }
    backend.primary()?;
    let mut formats = vec!["CF_HDROP"];
    let mut optional_pixel_warning = !warnings.is_empty();
    if has_pixels {
        match backend.pixels() {
            Ok(()) => formats.push("CF_DIBV5"),
            Err(error) => {
                optional_pixel_warning = true;
                warnings.push(format!("Files copied; image clipboard failed: {error}"));
            }
        }
    }
    let effect = match backend.effect() {
        Ok(true) => {
            formats.push("Preferred DropEffect");
            true
        }
        Ok(false) => false,
        Err(error) => {
            warnings.push(format!("Copy effect unavailable: {error}"));
            false
        }
    };
    let marker = match backend.marker() {
        Ok(Some(token)) => {
            formats.push("Katran owner marker");
            Some(token)
        }
        Ok(None) => None,
        Err(error) => {
            warnings.push(format!("Clipboard ownership marker failed: {error}"));
            None
        }
    };
    let _before_close = backend.sequence();
    let (sequence, sequence_verification) =
        match backend.finish(marker, formats.contains(&"CF_DIBV5"), effect) {
            Ok(sequence) if sequence != 0 => (sequence, "verified"),
            Ok(_) => (0, "sequence_unstable"),
            Err(category) => (0, category),
        };
    if sequence == 0 {
        warnings.push(
            "Files copied; clipboard ownership could not be verified for automatic paste".into(),
        );
    }
    Ok(ClipboardWrite {
        sequence,
        warnings,
        publication: Some(PublicationFacts {
            formats,
            item_count,
            optional_pixel_warning,
            sequence,
            sequence_verification,
        }),
    })
}

#[cfg(test)]
#[test]
fn file_owner_tokens_are_bounded() {
    let mut tokens = std::collections::VecDeque::new();
    for id in 0..65 {
        remember_file_token(&mut tokens, format!("{id:032x}"));
    }
    assert_eq!(tokens.len(), 64);
    assert!(!tokens.contains(&format!("{:032x}", 0)));
    assert!(tokens.contains(&format!("{:032x}", 64)));
}

pub(crate) struct PreparedNativeClipboard {
    paths: Vec<PathBuf>,
    #[cfg(windows)]
    files: Vec<u8>,
    pixels: Option<Vec<u8>>,
    warnings: Vec<String>,
}

#[cfg(windows)]
mod native_publication {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows::{
        Win32::{
            Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, RegisterClassW,
                UnregisterClassW, WNDCLASSW,
            },
        },
        core::{PCWSTR, w},
    };

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wp, lp) }
    }
    struct Window {
        hwnd: HWND,
        class: Vec<u16>,
        module: HINSTANCE,
    }
    impl Window {
        fn new() -> Result<Self, String> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let class: Vec<u16> = format!(
                "Katran.FileClipboard.{}.{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            let module = HINSTANCE(
                unsafe { GetModuleHandleW(None) }
                    .map_err(|e| e.to_string())?
                    .0,
            );
            let definition = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            if unsafe { RegisterClassW(&definition) } == 0 {
                return Err(windows::core::Error::from_thread().to_string());
            }
            let hwnd = match unsafe {
                CreateWindowExW(
                    Default::default(),
                    PCWSTR(class.as_ptr()),
                    w!(""),
                    Default::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(module),
                    None,
                )
            } {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    unsafe {
                        let _ = UnregisterClassW(PCWSTR(class.as_ptr()), Some(module));
                    }
                    return Err(error.to_string());
                }
            };
            Ok(Self {
                hwnd,
                class,
                module,
            })
        }
    }
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
                let _ = UnregisterClassW(PCWSTR(self.class.as_ptr()), Some(self.module));
            }
        }
    }
    /// Verify only the exact payload just published, with no delayed owner.
    /// OpenClipboard prevents other writers throughout the byte comparison.
    fn settle(
        files: &[u8],
        pixels: Option<&[u8]>,
        marker: Option<&str>,
        effect: bool,
    ) -> Result<u32, &'static str> {
        use windows_sys::Win32::{
            Foundation::{GetLastError, SetLastError},
            System::DataExchange::{
                CloseClipboard, EnumClipboardFormats, GetClipboardOwner,
                GetClipboardSequenceNumber, OpenClipboard,
            },
        };
        let expected_marker = marker.ok_or("marker_missing")?;
        if files.is_empty()
            || files.len() > 8 * 1024 * 1024
            || pixels.is_some_and(|p| p.len() > 128_000_124)
        {
            return Err("payload_changed");
        }
        if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
            return Err("busy");
        }
        struct Open(bool);
        impl Drop for Open {
            fn drop(&mut self) {
                if self.0 {
                    unsafe {
                        CloseClipboard();
                    }
                }
            }
        }
        let mut open = Open(true);
        let before = unsafe { GetClipboardSequenceNumber() };
        unsafe {
            SetLastError(0);
        }
        let owner = unsafe { GetClipboardOwner() };
        if owner.is_null() && unsafe { GetLastError() } != 0 {
            return Err("owner_unavailable");
        }
        let owner_none = owner.is_null();
        if !owner_none {
            return Err("owner_live");
        }
        let marker_format =
            clipboard_win::raw::register_format(crate::table_clipboard::OWNER_FORMAT)
                .ok_or("marker_missing")?
                .get();
        let effect_format = if effect {
            Some(
                clipboard_win::raw::register_format("Preferred DropEffect")
                    .ok_or("formats_changed")?
                    .get(),
            )
        } else {
            None
        };
        let mut formats = Vec::new();
        let mut id = 0;
        loop {
            unsafe {
                SetLastError(0);
            }
            id = unsafe { EnumClipboardFormats(id) };
            if id == 0 {
                if unsafe { GetLastError() } != 0 {
                    return Err("formats_changed");
                }
                break;
            }
            formats.push(id);
            if formats.len() > 64 {
                return Err("formats_changed");
            }
        }
        let formats_valid =
            accepted_formats(&formats, marker_format, effect_format, pixels.is_some());
        if !formats_valid {
            return Err("formats_changed");
        }
        let marker_valid = crate::table_clipboard::read_owner_token_locked()
            .ok()
            .flatten()
            .as_deref()
            == Some(expected_marker);
        if !marker_valid {
            return Err("marker_missing");
        }
        fn equal_bytes(format: u32, expected: &[u8]) -> bool {
            use windows_sys::Win32::{
                Foundation::{GetLastError, SetLastError},
                System::{
                    DataExchange::GetClipboardData,
                    Memory::{GlobalLock, GlobalSize, GlobalUnlock},
                },
            };
            let handle = unsafe { GetClipboardData(format) };
            if handle.is_null() {
                return false;
            }
            let size = unsafe { GlobalSize(handle) };
            if size < expected.len() || size > expected.len().saturating_add(16) {
                return false;
            }
            let ptr = unsafe { GlobalLock(handle) };
            if ptr.is_null() {
                return false;
            }
            let observed = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), expected.len()) };
            let equal = same_bounded_payload(expected, observed, size);
            unsafe {
                SetLastError(0);
            }
            let unlocked = unsafe { GlobalUnlock(handle) };
            equal && (unlocked != 0 || unsafe { GetLastError() } == 0)
        }
        let payload_valid = equal_bytes(15, files)
            && pixels.is_none_or(|bytes| equal_bytes(17, bytes))
            && effect_format.is_none_or(|id| equal_bytes(id, &1u32.to_le_bytes()));
        let after = unsafe { GetClipboardSequenceNumber() };
        if unsafe { CloseClipboard() } == 0 {
            return Err("close_failed");
        }
        open.0 = false;
        let closed = unsafe { GetClipboardSequenceNumber() };
        verified_settlement(SettlementProof {
            owner_none,
            formats_valid,
            marker_valid,
            payload_valid,
            sequence_before: before,
            sequence_after: after,
            sequence_closed: closed,
        })
    }
    pub(super) struct Backend<'a> {
        // Fields drop in declaration order: CloseClipboard before DestroyWindow.
        _clipboard: clipboard_win::Clipboard,
        window: Window,
        files: &'a [u8],
        pixels: Option<&'a [u8]>,
    }
    impl<'a> Backend<'a> {
        pub(super) fn open(files: &'a [u8], pixels: Option<&'a [u8]>) -> Result<Self, String> {
            let window = Window::new()?;
            if window.hwnd.0.is_null() {
                return Err("Clipboard owner window unavailable".into());
            }
            let clipboard = clipboard_win::Clipboard::new_attempts_for(window.hwnd.0, 20)
                .map_err(|e| e.to_string())?;
            Ok(Self {
                _clipboard: clipboard,
                window,
                files,
                pixels,
            })
        }
    }
    impl FilePublication for Backend<'_> {
        fn owner_valid(&self) -> bool {
            !self.window.hwnd.0.is_null()
        }
        fn primary(&mut self) -> Result<(), String> {
            clipboard_win::raw::set(15, self.files).map_err(|e| e.to_string())
        }
        fn pixels(&mut self) -> Result<(), String> {
            clipboard_win::raw::set_without_clear(17, self.pixels.ok_or("Pixels unavailable")?)
                .map_err(|e| e.to_string())
        }
        fn effect(&mut self) -> Result<bool, String> {
            let Some(format) = clipboard_win::raw::register_format("Preferred DropEffect") else {
                return Ok(false);
            };
            clipboard_win::raw::set_without_clear(format.get(), &1u32.to_le_bytes())
                .map_err(|e| e.to_string())?;
            Ok(true)
        }
        fn marker(&mut self) -> Result<Option<String>, String> {
            let mut tokens = FILE_OWNER_TOKENS
                .lock()
                .map_err(|_| "Clipboard ownership registry unavailable".to_owned())?;
            let token = unsafe { windows::Win32::System::Com::CoCreateGuid() }
                .map(|guid| format!("{:032x}", guid.to_u128()))
                .map_err(|e| e.to_string())?;
            let format = clipboard_win::raw::register_format(crate::table_clipboard::OWNER_FORMAT)
                .ok_or("Cannot register ownership marker")?;
            let bytes = [token.as_bytes(), &[0]].concat();
            clipboard_win::raw::set_without_clear(format.get(), &bytes)
                .map_err(|e| e.to_string())?;
            remember_file_token(&mut tokens, token.clone());
            Ok(Some(token))
        }
        fn sequence(&self) -> u32 {
            unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
        }
        fn finish(
            self,
            marker: Option<String>,
            pixels: bool,
            effect: bool,
        ) -> Result<u32, &'static str> {
            let files = self.files;
            let expected_pixels = if pixels { self.pixels } else { None };
            drop(self);
            settle(files, expected_pixels, marker.as_deref(), effect)
        }
    }
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
        let result = self.write_with_facts(paths)?;
        Ok((result.sequence, result.warnings))
    }
    fn write_with_facts(&mut self, paths: &[PathBuf]) -> Result<ClipboardWrite, String> {
        if paths != self.paths {
            return Err("Prepared clipboard payload does not match export plan".into());
        }
        #[cfg(windows)]
        {
            let backend = native_publication::Backend::open(&self.files, self.pixels.as_deref())?;
            publish_file_formats(
                backend,
                paths.len(),
                self.pixels.is_some(),
                std::mem::take(&mut self.warnings),
            )
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
    fn write_with_facts(&mut self, paths: &[PathBuf]) -> Result<ClipboardWrite, String> {
        PreparedNativeClipboard::prepare(paths).write_with_facts(paths)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct FakePublication {
        events: Arc<Mutex<Vec<&'static str>>>,
        valid_owner: bool,
        fail_primary: bool,
        fail_pixels: bool,
        settled: Result<u32, &'static str>,
    }
    impl Drop for FakePublication {
        fn drop(&mut self) {
            let mut events = self.events.lock().unwrap();
            events.push("close_clipboard");
            events.push("destroy_owner");
        }
    }
    impl FilePublication for FakePublication {
        fn owner_valid(&self) -> bool {
            self.events.lock().unwrap().push("check_owner");
            self.valid_owner
        }
        fn primary(&mut self) -> Result<(), String> {
            self.events.lock().unwrap().push("hdrop");
            if self.fail_primary {
                Err("primary failed".into())
            } else {
                Ok(())
            }
        }
        fn pixels(&mut self) -> Result<(), String> {
            self.events.lock().unwrap().push("dibv5");
            if self.fail_pixels {
                Err("pixel failed".into())
            } else {
                Ok(())
            }
        }
        fn effect(&mut self) -> Result<bool, String> {
            self.events.lock().unwrap().push("effect");
            Ok(true)
        }
        fn marker(&mut self) -> Result<Option<String>, String> {
            self.events.lock().unwrap().push("marker");
            Ok(Some("synthetic-token".into()))
        }
        fn sequence(&self) -> u32 {
            self.events.lock().unwrap().push("sequence");
            42
        }
        fn finish(
            self,
            marker: Option<String>,
            _pixels: bool,
            _effect: bool,
        ) -> Result<u32, &'static str> {
            let events = self.events.clone();
            let result = self.settled;
            drop(self);
            events.lock().unwrap().push("verify_settled");
            if marker.is_none() {
                return Err("marker_missing");
            }
            result
        }
    }
    fn fake_publication(
        valid_owner: bool,
        fail_primary: bool,
        fail_pixels: bool,
    ) -> (FakePublication, Arc<Mutex<Vec<&'static str>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        (
            FakePublication {
                events: events.clone(),
                valid_owner,
                fail_primary,
                fail_pixels,
                settled: Ok(44),
            },
            events,
        )
    }
    fn proof() -> SettlementProof {
        SettlementProof {
            owner_none: true,
            formats_valid: true,
            marker_valid: true,
            payload_valid: true,
            sequence_before: 44,
            sequence_after: 44,
            sequence_closed: 44,
        }
    }
    #[test]
    fn settled_proof_rejects_foreign_payload_formats_owner_and_races() {
        assert_eq!(verified_settlement(proof()), Ok(44));
        let mut p = proof();
        p.payload_valid = false;
        assert_eq!(verified_settlement(p), Err("payload_changed"));
        let mut p = proof();
        p.marker_valid = false;
        assert_eq!(verified_settlement(p), Err("marker_missing"));
        let mut p = proof();
        p.formats_valid = false;
        assert_eq!(verified_settlement(p), Err("formats_changed"));
        let mut p = proof();
        p.owner_none = false;
        assert_eq!(verified_settlement(p), Err("owner_live"));
        let mut p = proof();
        p.sequence_after = 45;
        assert_eq!(verified_settlement(p), Err("sequence_unstable"));
        let mut p = proof();
        p.sequence_closed = 45;
        assert_eq!(verified_settlement(p), Err("sequence_unstable"));
        assert!(accepted_formats(
            &[15, 17, 8, 2, 9, 50000, 50001],
            50000,
            Some(50001),
            true
        ));
        assert!(!accepted_formats(
            &[15, 17, 50000, 50001, 50002],
            50000,
            Some(50001),
            true
        ));
        assert!(!accepted_formats(
            &[15, 17, 50001],
            50000,
            Some(50001),
            true
        ));
        // A foreign replacement that copied our marker still fails when it changes HDROP or pixels.
        assert!(same_bounded_payload(b"hdrop", b"hdrop", 8));
        assert!(!same_bounded_payload(b"hdrop", b"hdr0p", 8));
        assert!(!same_bounded_payload(b"pixels", b"pixe1s", 8));
        assert!(!same_bounded_payload(b"hdrop", b"hdrop", 99));
    }
    #[test]
    fn failed_postpublication_proof_preserves_export() {
        struct Unverified;
        impl ClipboardSink for Unverified {
            fn write(&mut self, _: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
                unreachable!()
            }
            fn write_with_facts(&mut self, paths: &[PathBuf]) -> Result<ClipboardWrite, String> {
                let (mut backend, _) = fake_publication(true, false, false);
                backend.settled = Err("payload_changed");
                publish_file_formats(backend, paths.len(), true, vec![])
            }
        }
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("synthetic.bin");
        std::fs::write(&source, b"fixture").unwrap();
        let plan = prepare(root.path(), &[cached_event(source)]).unwrap();
        let exported = plan.paths[0].clone();
        let receipt = commit(&plan, &mut Unverified).unwrap();
        assert_eq!(receipt.sequence, 0);
        assert_eq!(
            receipt.publication.unwrap().sequence_verification,
            "payload_changed"
        );
        drop(plan);
        assert!(exported.exists());
    }
    #[test]
    fn file_publication_requires_owner_and_closes_before_destroying_it() {
        let (backend, events) = fake_publication(true, false, false);
        let result = publish_file_formats(backend, 2, true, vec![]).unwrap();
        assert_eq!(
            *events.lock().unwrap(),
            [
                "check_owner",
                "hdrop",
                "dibv5",
                "effect",
                "marker",
                "sequence",
                "close_clipboard",
                "destroy_owner",
                "verify_settled"
            ]
        );
        let facts = result.publication.unwrap();
        assert_eq!(
            facts.formats,
            [
                "CF_HDROP",
                "CF_DIBV5",
                "Preferred DropEffect",
                "Katran owner marker"
            ]
        );
        assert_eq!(facts.item_count, 2);
        assert_eq!(facts.sequence, 44);
        assert_eq!(facts.sequence_verification, "verified");
        assert!(!facts.optional_pixel_warning);
        let encoded = serde_json::to_string(&facts).unwrap();
        assert!(!encoded.contains("path") && !encoded.contains("content"));
        let (backend, events) = fake_publication(false, false, false);
        assert!(publish_file_formats(backend, 1, false, vec![]).is_err());
        assert_eq!(
            *events.lock().unwrap(),
            ["check_owner", "close_clipboard", "destroy_owner"]
        );
    }
    #[test]
    fn hdrop_failure_is_fatal_but_optional_pixels_are_reported() {
        let (backend, events) = fake_publication(true, true, false);
        assert!(publish_file_formats(backend, 1, true, vec![]).is_err());
        assert_eq!(
            *events.lock().unwrap(),
            ["check_owner", "hdrop", "close_clipboard", "destroy_owner"]
        );
        let (backend, events) = fake_publication(true, false, true);
        let result = publish_file_formats(backend, 1, true, vec![]).unwrap();
        let facts = result.publication.unwrap();
        assert_eq!(facts.sequence, 44);
        assert_eq!(facts.sequence_verification, "verified");
        assert!(facts.optional_pixel_warning);
        assert!(!facts.formats.contains(&"CF_DIBV5"));
        assert!(facts.formats.contains(&"CF_HDROP"));
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(events.lock().unwrap().last(), Some(&"verify_settled"));
    }
    #[test]
    fn commit_exposes_publication_facts_without_changing_legacy_receipt() {
        struct FactSink;
        impl ClipboardSink for FactSink {
            fn write(&mut self, _: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
                unreachable!("commit must use extended sink boundary")
            }
            fn write_with_facts(&mut self, paths: &[PathBuf]) -> Result<ClipboardWrite, String> {
                let (backend, _) = fake_publication(true, false, true);
                publish_file_formats(backend, paths.len(), true, vec![])
            }
        }
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("synthetic.bin");
        std::fs::write(&source, b"fixture").unwrap();
        let plan = prepare(root.path(), &[cached_event(source)]).unwrap();
        let receipt = commit(&plan, &mut FactSink).unwrap();
        assert_eq!(receipt.count, 1);
        assert_eq!(receipt.sequence, 44);
        assert!(receipt.failures.is_empty());
        assert_eq!(receipt.warnings.len(), 1);
        let facts = receipt.publication.unwrap();
        assert_eq!(facts.item_count, 1);
        assert_eq!(facts.sequence, receipt.sequence);
        assert!(facts.optional_pixel_warning);
    }
    #[cfg(windows)]
    #[test]
    fn unicode_group_hdrop_encodes_all_names_and_double_terminator() {
        use std::os::windows::ffi::OsStrExt;
        let paths = [
            PathBuf::from(r"C:\test\папка\猫.txt"),
            PathBuf::from(r"C:\test\🐄.png"),
        ];
        let bytes = file_drop(&paths);
        let units: Vec<u16> = bytes[20..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let expected: Vec<u16> = paths
            .iter()
            .flat_map(|path| path.as_os_str().encode_wide().chain(Some(0)))
            .chain(Some(0))
            .collect();
        assert_eq!(units, expected);
        assert_eq!(&units[units.len() - 2..], &[0, 0]);
    }
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

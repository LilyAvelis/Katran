//! Bounded, per-profile handoff for explicit folder launch requests.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};
const SLOTS: usize = 32;
const MAX_BYTES: u64 = 64 * 1024;
const MAX_UNITS: usize = 16_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    path: String,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn reparse(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
fn directory(root: &Path) -> io::Result<PathBuf> {
    let meta = fs::symlink_metadata(root)?;
    if reparse(&meta) || !meta.is_dir() {
        return Err(invalid("Invalid profile directory"));
    }
    let parent = fs::canonicalize(root)?;
    let queue = parent.join("launch-requests");
    match fs::create_dir(&queue) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let meta = fs::symlink_metadata(&queue)?;
    if reparse(&meta)
        || !meta.is_dir()
        || fs::canonicalize(&queue)?.parent() != Some(parent.as_path())
    {
        return Err(invalid("Invalid request directory"));
    }
    Ok(queue)
}
fn checked_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid("Folder must be absolute"));
    }
    let canonical = fs::canonicalize(path)?;
    if !fs::metadata(&canonical)?.is_dir() {
        return Err(invalid("Folder must be a directory"));
    }
    let string = canonical
        .to_str()
        .ok_or_else(|| invalid("Folder path is not Unicode"))?;
    if string.contains('\0') || string.encode_utf16().count() > MAX_UNITS {
        return Err(invalid("Folder path exceeds limit"));
    }
    Ok(canonical)
}
static NONCE: AtomicU64 = AtomicU64::new(0);
fn lock_name(slot: usize) -> String {
    format!("katran-{slot:02}.lock")
}
fn request_name(slot: usize) -> String {
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "katran-{slot:02}-{time:032x}-{:08x}-{:016x}",
        std::process::id(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    )
}
fn parse_name(name: &str) -> Option<(usize, &str)> {
    let (stem, ext) = name.rsplit_once('.')?;
    if !matches!(ext, "json" | "tmp" | "claim") {
        return None;
    }
    let mut parts = stem.split('-');
    if parts.next()? != "katran" {
        return None;
    }
    let slot_str = parts.next()?;
    let stamp = parts.next()?;
    let pid = parts.next()?;
    let nonce = parts.next()?;
    if parts.next().is_some()
        || slot_str.len() != 2
        || stamp.len() != 32
        || pid.len() != 8
        || nonce.len() != 16
        || !slot_str.bytes().all(|b| b.is_ascii_digit())
        || ![stamp, pid, nonce].iter().all(|s| {
            s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
    {
        return None;
    }
    let slot = slot_str.parse::<usize>().ok()?;
    (slot < SLOTS).then_some((slot, ext))
}
fn slot_occupied(queue: &Path, slot: usize) -> io::Result<bool> {
    for item in fs::read_dir(queue)? {
        let item = item?;
        if item
            .file_name()
            .to_str()
            .and_then(parse_name)
            .is_some_and(|(n, _)| n == slot)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn open_new(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// Queue one existing folder without changing the folder itself.
pub fn enqueue(root: &Path, path: &Path) -> io::Result<()> {
    let path = checked_path(path)?;
    let request = Request {
        version: 1,
        path: path
            .to_str()
            .ok_or_else(|| invalid("Folder path is not Unicode"))?
            .to_owned(),
    };
    let data = serde_json::to_vec(&request).map_err(|e| invalid(&e.to_string()))?;
    if data.len() as u64 > MAX_BYTES {
        return Err(invalid("Folder request exceeds limit"));
    }
    let queue = directory(root)?;
    for slot in 0..SLOTS {
        let lock = queue.join(lock_name(slot));
        let guard = match open_new(&lock) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let occupied = match slot_occupied(&queue, slot) {
            Ok(value) => value,
            Err(e) => {
                drop(guard);
                let _ = fs::remove_file(&lock);
                return Err(e);
            }
        };
        if occupied {
            drop(guard);
            fs::remove_file(&lock)?;
            continue;
        }
        let stem = request_name(slot);
        let temp = queue.join(format!("{stem}.tmp"));
        let ready = queue.join(format!("{stem}.json"));
        let result = (|| {
            let mut file = open_new(&temp)?;
            if let Err(e) = file.write_all(&data).and_then(|_| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&temp);
                return Err(e);
            }
            drop(file);
            if let Err(e) = fs::rename(&temp, &ready) {
                let _ = fs::remove_file(&temp);
                return Err(e);
            }
            Ok(())
        })();
        drop(guard);
        fs::remove_file(&lock)?;
        return result;
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "Folder request queue is full",
    ))
}

fn read_claim(path: &Path) -> io::Result<PathBuf> {
    let meta = fs::symlink_metadata(path)?;
    if reparse(&meta) || !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(invalid("Invalid folder request file"));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path)?;
    let opened = file.metadata()?;
    if reparse(&opened) || !opened.is_file() || opened.len() > MAX_BYTES {
        return Err(invalid("Invalid folder request file"));
    }
    let mut data = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_BYTES {
        return Err(invalid("Folder request exceeds limit"));
    }
    let request: Request =
        serde_json::from_slice(&data).map_err(|_| invalid("Malformed folder request"))?;
    if request.version != 1 {
        return Err(invalid("Unknown folder request version"));
    }
    checked_path(Path::new(&request.path))
}

/// Atomically claim each pending request. Individual failures do not block others.
pub fn take(root: &Path) -> io::Result<Vec<Result<PathBuf, String>>> {
    let queue = directory(root)?;
    let mut pending = Vec::new();
    for entry in fs::read_dir(&queue)? {
        let entry = entry?;
        let filename = entry.file_name();
        if filename
            .to_str()
            .and_then(parse_name)
            .is_some_and(|(_, ext)| ext == "json")
        {
            let meta = fs::symlink_metadata(entry.path())?;
            pending.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), filename));
        }
    }
    pending.sort();
    let mut results = Vec::with_capacity(pending.len());
    for (_, filename) in pending.into_iter().take(SLOTS) {
        let ready = queue.join(filename);
        let claim = ready.with_extension("claim");
        match fs::symlink_metadata(&ready) {
            Ok(meta) if reparse(&meta) || !meta.is_file() => {
                results.push(Err("Invalid folder request file".into()));
                continue;
            }
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                results.push(Err(format!("Cannot inspect folder request: {e}")));
                continue;
            }
        }
        if fs::symlink_metadata(&claim).is_ok() {
            continue;
        }
        match fs::rename(&ready, &claim) {
            Ok(()) => (),
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    || e.kind() == io::ErrorKind::AlreadyExists =>
            {
                continue;
            }
            Err(e) => {
                results.push(Err(format!("Cannot claim folder request: {e}")));
                continue;
            }
        }
        let result = read_claim(&claim).map_err(|e| format!("Invalid folder request: {e}"));
        // Only our strictly named claimed entry is removed, never its target.
        match fs::remove_file(&claim) {
            Ok(()) => results.push(result),
            Err(e) => results.push(Err(format!("Cannot remove claimed folder request: {e}"))),
        }
    }
    Ok(results)
}

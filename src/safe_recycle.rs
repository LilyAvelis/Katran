//! Recoverable, handle-bound staging before an injected recycle operation.
//! This module is not a guarantee that an arbitrary Trash implementation is
//! recycle-only. Production must establish that contract before enabling it.
use crate::{ApiError, Result, downloads::Trash};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct RecycleReceipt {
    pub operation_id: String,
    pub original_path: PathBuf,
    pub status: &'static str,
    pub original_removed: bool,
    pub recycled: bool,
    pub recovery_path: Option<PathBuf>,
    pub error: Option<String>,
}

#[cfg(not(windows))]
pub fn recycle_verified(_: &Path, _: &Path, _: &str, _: &dyn Trash) -> Result<RecycleReceipt> {
    Err(ApiError::new(
        "unsupported_platform",
        "Verified staging requires Windows",
    ))
}

#[cfg(not(windows))]
pub(crate) fn recycle_locked(
    _: crate::delete_selection::LockedOriginal,
    _: &Path,
    _: &Path,
    _: &str,
    _: &dyn Trash,
) -> Result<RecycleReceipt> {
    Err(ApiError::new(
        "unsupported_platform",
        "Verified staging requires Windows",
    ))
}

#[cfg(windows)]
pub fn recycle_verified(
    path: &Path,
    root: &Path,
    hash: &str,
    trash: &dyn Trash,
) -> Result<RecycleReceipt> {
    let root = root.canonicalize()?;
    let original = path.canonicalize()?;
    let locked = crate::delete_selection::prepare_locked_original(&original, &root, hash)?;
    recycle_locked(locked, &original, &root, hash, trash)
}

/// Continue from the same authorized handle; no pathname reopen may replace it.
#[cfg(windows)]
pub(crate) fn recycle_locked(
    mut locked: crate::delete_selection::LockedOriginal,
    path: &Path,
    root: &Path,
    hash: &str,
    trash: &dyn Trash,
) -> Result<RecycleReceipt> {
    use crate::delete_selection::{
        locked_identity, prepare_locked_original, rename_locked_original,
    };
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = root.canonicalize()?;
    let original = path.canonicalize()?;
    if original == root
        || !original.starts_with(&root)
        || !fs::symlink_metadata(path)?.file_type().is_file()
    {
        return Err(ApiError::new(
            "unsafe_source",
            "Original is not a regular file inside the configured folder",
        ));
    }
    if crate::delete_selection::locked_path(&locked)? != original {
        return Err(ApiError::new(
            "stale_selection",
            "Authorized handle no longer names the selected original",
        ));
    }
    let identity = locked_identity(&locked)?;
    let (operation_id, stage) = loop {
        let id = format!(
            "{:x}-{:x}-{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let stage = root.join(format!(".katran-recovery-{id}"));
        match fs::create_dir(&stage) {
            Ok(()) => break (id, stage),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    };
    // A fresh directory beneath the canonical root, not a reparse point. The
    // volume check also catches nested mounted volumes under a configured root.
    let stage_metadata = fs::symlink_metadata(&stage)?;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    if !stage_metadata.is_dir()
        || stage_metadata.file_attributes() & 0x400 != 0
        || stage.canonicalize()?.parent() != Some(root.as_path())
    {
        return Err(ApiError::new(
            "unsafe_staging",
            "Staging directory changed or is a reparse point",
        ));
    }
    let directory = fs::OpenOptions::new()
        .access_mode(0x80)
        // Keep this operation's own stage from being renamed or replaced.
        .share_mode(3)
        .custom_flags(0x02000000 | 0x00200000)
        .open(&stage)?;
    let stage_identity = directory_identity(&directory)?;
    if stage_identity.0 != identity.volume {
        return Err(ApiError::new(
            "unsafe_staging",
            "Source and staging directory are on different volumes",
        ));
    }
    set_hidden(&stage)?;
    let data = stage.join("data");
    fs::create_dir(&data)?;
    let staged = data.join(
        original
            .file_name()
            .ok_or_else(|| ApiError::new("unsafe_source", "Original has no filename"))?,
    );
    let record = serde_json::json!({"operation_id":operation_id,"original_path":original,"staged_path":staged,"hash":hash,"identity":identity});
    write_record(&stage.join("prepared.json"), &record)?;
    rename_locked_original(&mut locked, &staged)?;
    if let Err(error) = write_record(&stage.join("staged.json"), &record) {
        let receipt = restore_locked(
            locked,
            &original,
            &staged,
            &stage,
            &operation_id,
            format!("Journal stage failed: {}", error.message),
        );
        if receipt.status == "restored" {
            cleanup_completed(&stage, directory, stage_identity, &record, &receipt);
        }
        return Ok(receipt);
    }
    drop(locked);
    // Reopen, verify content and file identity directly before handing the
    // staged path to Trash. The remaining Shell pathname race is bounded to
    // the private staging path, never the original path.
    let staged_handle = match prepare_locked_original(&staged, &stage, hash) {
        Ok(handle) => handle,
        Err(e) => {
            return Ok(recovery(
                &operation_id,
                &original,
                &staged,
                format!("Cannot verify staged file: {}", e.message),
            ));
        }
    };
    match locked_identity(&staged_handle) {
        Ok(found) if found == identity => {}
        _ => {
            return Ok(recovery(
                &operation_id,
                &original,
                &staged,
                "Staged file identity changed".into(),
            ));
        }
    }
    drop(staged_handle);
    match trash.recycle(&staged) {
        Ok(()) if !staged.exists() => {
            let receipt = RecycleReceipt {
                operation_id,
                original_path: original,
                status: "recycled",
                original_removed: true,
                recycled: true,
                recovery_path: None,
                error: None,
            };
            // A failure to record completion cannot turn an already recycled
            // source into a retryable deletion. Return the confirmed result.
            if write_record(
                &stage.join("outcome.json"),
                &serde_json::json!({"receipt":receipt}),
            )
            .is_ok()
            {
                cleanup_completed(&stage, directory, stage_identity, &record, &receipt);
            }
            Ok(receipt)
        }
        Ok(()) => Ok(recovery(
            &operation_id,
            &original,
            &staged,
            "Trash reported success but staged file remains".into(),
        )),
        Err(error) => {
            let mut handle = match prepare_locked_original(&staged, &stage, hash) {
                Ok(handle) => handle,
                Err(e) => {
                    return Ok(recovery(
                        &operation_id,
                        &original,
                        &staged,
                        format!(
                            "Recycle failed: {}; staged file cannot be verified: {}",
                            error.message, e.message
                        ),
                    ));
                }
            };
            if locked_identity(&handle).ok() != Some(identity) {
                return Ok(recovery(
                    &operation_id,
                    &original,
                    &staged,
                    "Recycle failed and staged identity changed".into(),
                ));
            }
            let receipt = restore_locked_handle(
                &mut handle,
                &original,
                &staged,
                &stage,
                &operation_id,
                error.message,
            );
            if receipt.status == "restored" {
                cleanup_completed(&stage, directory, stage_identity, &record, &receipt);
            }
            Ok(receipt)
        }
    }
}

#[cfg(windows)]
fn write_record(path: &Path, value: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec(value).map_err(|e| ApiError::new("journal_failed", e))?;
    let temporary = path.with_extension("json.pending");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    // Only journal files are renamed by pathname. Original data is always moved
    // through its verified handle. Destination is a new, unique phase record.
    if path.exists() {
        return Err(ApiError::new(
            "journal_failed",
            "Journal phase already exists",
        ));
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}

/// Retire only this operation's exact completed journal and empty stage. This
/// never enumerates older recovery directories and never recursively deletes.
/// Any mismatch or I/O error leaves remaining material for inspection.
#[cfg(windows)]
fn cleanup_completed(
    stage: &Path,
    stage_guard: std::fs::File,
    stage_identity: (u32, u64),
    record: &serde_json::Value,
    receipt: &RecycleReceipt,
) {
    use std::{
        fs,
        io::Read,
        os::windows::fs::{MetadataExt, OpenOptionsExt},
        os::windows::io::AsRawHandle,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };
    fn remove_open(file: &fs::File) -> std::io::Result<()> {
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    let try_cleanup = || -> std::io::Result<()> {
        // A directory handle with DELETE access blocks the journal's Win32
        // renames. Upgrade after the final receipt and verify the same directory.
        drop(stage_guard);
        let stage_handle = fs::OpenOptions::new()
            .access_mode(0x80000000 | 0x00010000)
            .share_mode(3)
            .custom_flags(0x02000000 | 0x00200000)
            .open(stage)?;
        if directory_identity(&stage_handle).ok() != Some(stage_identity) {
            return Err(std::io::Error::other("recovery stage changed"));
        }
        let mut names = fs::read_dir(stage)?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        names.sort();
        let mut expected: Vec<std::ffi::OsString> =
            ["data", "prepared.json", "staged.json", "outcome.json"]
                .into_iter()
                .map(Into::into)
                .collect();
        expected.sort();
        if names != expected {
            return Err(std::io::Error::other("unexpected recovery entries"));
        }
        let data_path = stage.join("data");
        let data_meta = fs::symlink_metadata(&data_path)?;
        if !data_meta.is_dir()
            || data_meta.file_attributes() & 0x400 != 0
            || data_path.canonicalize()?.parent() != Some(stage)
        {
            return Err(std::io::Error::other("unsafe recovery data directory"));
        }
        let data = fs::OpenOptions::new()
            .access_mode(0x80000000 | 0x00010000)
            .share_mode(3)
            .custom_flags(0x02000000 | 0x00200000)
            .open(&data_path)?;
        let opened_data = data.metadata()?;
        if !opened_data.is_dir()
            || opened_data.file_attributes() & 0x400 != 0
            || data_path.canonicalize()?.parent() != Some(stage)
        {
            return Err(std::io::Error::other("recovery data changed"));
        }
        if fs::read_dir(&data_path)?.next().is_some() {
            return Err(std::io::Error::other("recovery data is not empty"));
        }
        let expected_record = serde_json::to_vec(record)?;
        let expected_outcome = serde_json::to_vec(&serde_json::json!({"receipt":receipt}))?;
        let mut files = Vec::new();
        for (name, expected) in [
            ("staged.json", &expected_record),
            ("prepared.json", &expected_record),
            ("outcome.json", &expected_outcome),
        ] {
            let mut file = fs::OpenOptions::new()
                .access_mode(0x80000000 | 0x00010000)
                .share_mode(1)
                .custom_flags(0x00200000)
                .open(stage.join(name))?;
            let meta = file.metadata()?;
            if !meta.is_file()
                || meta.file_attributes() & 0x400 != 0
                || meta.len() != expected.len() as u64
                || meta.len() > 65536
            {
                return Err(std::io::Error::other("unexpected recovery journal"));
            }
            let mut actual = Vec::new();
            file.read_to_end(&mut actual)?;
            if &actual != expected {
                return Err(std::io::Error::other("changed recovery journal"));
            }
            files.push(file);
        }
        remove_open(&data)?;
        drop(data);
        for file in files {
            remove_open(&file)?;
        }
        remove_open(&stage_handle)?;
        Ok(())
    };
    let _ = try_cleanup();
}

#[cfg(windows)]
fn restore_locked(
    mut handle: crate::delete_selection::LockedOriginal,
    original: &Path,
    staged: &Path,
    stage: &Path,
    id: &str,
    error: String,
) -> RecycleReceipt {
    restore_locked_handle(&mut handle, original, staged, stage, id, error)
}
#[cfg(windows)]
fn restore_locked_handle(
    handle: &mut crate::delete_selection::LockedOriginal,
    original: &Path,
    staged: &Path,
    stage: &Path,
    id: &str,
    error: String,
) -> RecycleReceipt {
    match crate::delete_selection::rename_locked_original(handle, original) {
        Ok(()) => {
            let receipt = RecycleReceipt {
                operation_id: id.into(),
                original_path: original.into(),
                status: "restored",
                original_removed: false,
                recycled: false,
                recovery_path: None,
                error: Some(error),
            };
            let _ = write_record(
                &stage.join("outcome.json"),
                &serde_json::json!({"receipt":receipt}),
            );
            receipt
        }
        Err(e) => recovery(
            id,
            original,
            staged,
            format!("{error}; restore failed: {}", e.message),
        ),
    }
}
#[cfg(windows)]
fn recovery(id: &str, original: &Path, staged: &Path, error: String) -> RecycleReceipt {
    RecycleReceipt {
        operation_id: id.into(),
        original_path: original.into(),
        status: "recovery_required",
        original_removed: true,
        recycled: false,
        recovery_path: Some(staged.into()),
        error: Some(error),
    }
}

#[cfg(windows)]
fn directory_identity(file: &std::fs::File) -> Result<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if info.dwFileAttributes & 0x400 != 0 || info.dwFileAttributes & 0x10 == 0 {
        return Err(ApiError::new(
            "unsafe_staging",
            "Staging directory is a reparse point",
        ));
    }
    Ok((
        info.dwVolumeSerialNumber,
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}
#[cfg(windows)]
fn set_hidden(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW};
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { SetFileAttributesW(name.as_ptr(), FILE_ATTRIBUTE_HIDDEN) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

/// Read only immediate Katran journals. Never restores, removes, or retries.
#[cfg(windows)]
pub fn list_recovery(root: &Path) -> Result<serde_json::Value> {
    use std::{fs, io::Read, os::windows::fs::MetadataExt};
    let root = root.canonicalize()?;
    let mut items = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".katran-recovery-")
        {
            continue;
        }
        let stage = entry.path();
        let inspected = (|| -> Result<serde_json::Value> {
            let metadata = fs::symlink_metadata(&stage)?;
            if !metadata.is_dir()
                || metadata.file_attributes() & 0x400 != 0
                || stage.canonicalize()?.parent() != Some(root.as_path())
            {
                return Err(ApiError::new(
                    "unsafe_staging",
                    "Recovery directory is not a direct regular directory",
                ));
            }
            let read = |name: &str| -> Result<serde_json::Value> {
                let path = stage.join(name);
                let meta = fs::symlink_metadata(&path)?;
                if !meta.is_file() || meta.file_attributes() & 0x400 != 0 || meta.len() > 65536 {
                    return Err(ApiError::new("invalid_journal", "Invalid recovery journal"));
                }
                let mut bytes = Vec::new();
                fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
                if bytes.len() > 65536 {
                    return Err(ApiError::new(
                        "invalid_journal",
                        "Recovery journal exceeds limit",
                    ));
                }
                serde_json::from_slice(&bytes).map_err(|e| ApiError::new("invalid_journal", e))
            };
            let prepared = read("prepared.json")?;
            let operation_id = prepared["operation_id"]
                .as_str()
                .ok_or_else(|| ApiError::new("invalid_journal", "Missing operation ID"))?;
            if stage.file_name().and_then(|v| v.to_str())
                != Some(format!(".katran-recovery-{operation_id}").as_str())
            {
                return Err(ApiError::new("invalid_journal", "Operation ID mismatch"));
            }
            let original = PathBuf::from(
                prepared["original_path"]
                    .as_str()
                    .ok_or_else(|| ApiError::new("invalid_journal", "Missing original path"))?,
            );
            let staged = PathBuf::from(
                prepared["staged_path"]
                    .as_str()
                    .ok_or_else(|| ApiError::new("invalid_journal", "Missing staged path"))?,
            );
            let expected = prepared["hash"]
                .as_str()
                .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or_else(|| ApiError::new("invalid_journal", "Invalid source hash"))?;
            let identity: crate::delete_selection::LockedIdentity =
                serde_json::from_value(prepared["identity"].clone())
                    .map_err(|e| ApiError::new("invalid_journal", e))?;
            if !original.starts_with(&root)
                || original == root
                || original.file_name().is_none()
                || staged != stage.join("data").join(original.file_name().unwrap())
            {
                return Err(ApiError::new(
                    "invalid_journal",
                    "Paths escape recorded staging layout",
                ));
            }
            let outcome = match read("outcome.json") {
                Ok(value) => Some(value),
                Err(error) if error.code == "io_error" && !stage.join("outcome.json").exists() => {
                    None
                }
                Err(error) => return Err(error),
            };
            // Skip only a verified, internally consistent completed receipt when
            // the private data directory is still a real child and the staged
            // pathname is absent. Uncertain, malformed and unfinished entries
            // remain visible and count toward the bound.
            let receipt = outcome.as_ref().and_then(|value| value.get("receipt"));
            let complete = receipt.is_some_and(|receipt| {
                let same_id = receipt["operation_id"].as_str() == Some(operation_id);
                let same_path = receipt["original_path"]
                    .as_str()
                    .is_some_and(|path| Path::new(path) == original);
                let no_recovery = receipt
                    .get("recovery_path")
                    .is_some_and(serde_json::Value::is_null);
                let valid_status = match receipt["status"].as_str() {
                    Some("recycled") => {
                        receipt["recycled"] == true
                            && receipt["original_removed"] == true
                            && receipt.get("error").is_some_and(serde_json::Value::is_null)
                    }
                    Some("restored") => {
                        receipt["recycled"] == false && receipt["original_removed"] == false
                    }
                    _ => false,
                };
                same_id && same_path && no_recovery && valid_status
            });
            let data = stage.join("data");
            let safe_data = fs::symlink_metadata(&data).is_ok_and(|metadata| {
                metadata.is_dir()
                    && metadata.file_attributes() & 0x400 == 0
                    && data.canonicalize().ok().as_deref().and_then(Path::parent)
                        == Some(stage.as_path())
            });
            let staged_absent = fs::symlink_metadata(&staged)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
            if complete && safe_data && staged_absent {
                return Ok(serde_json::Value::Null);
            }
            let mut recovery_path = None;
            let mut staged_state = "missing";
            if staged.exists() {
                let data = fs::symlink_metadata(stage.join("data"))?;
                if !data.is_dir() || data.file_attributes() & 0x400 != 0 {
                    return Err(ApiError::new(
                        "unsafe_staging",
                        "Recovery data directory is a reparse point",
                    ));
                }
                match crate::delete_selection::prepare_locked_original(&staged, &stage, expected) {
                    Ok(handle)
                        if crate::delete_selection::locked_identity(&handle).ok()
                            == Some(identity) =>
                    {
                        staged_state = "verified";
                        recovery_path = Some(staged.clone());
                    }
                    _ => {
                        staged_state = "identity_conflict";
                    }
                }
            }
            Ok(
                serde_json::json!({"operation_id":operation_id,"original_path":original,"staged_state":staged_state,"recovery_path":recovery_path,"outcome":outcome,"original_exists":original.exists()}),
            )
        })();
        let item = match inspected {
            Ok(item) if item.is_null() => continue,
            Ok(item) => item,
            Err(e) => serde_json::json!({"journal_dir":stage,"error":e}),
        };
        if items.len() == 1000 {
            return Err(ApiError::new(
                "recovery_limit",
                "More than 1000 unresolved recovery journals",
            ));
        }
        items.push(item);
    }
    Ok(serde_json::json!({"items":items}))
}

#[cfg(not(windows))]
pub fn list_recovery(_: &Path) -> Result<serde_json::Value> {
    Err(ApiError::new(
        "unsupported_platform",
        "Recovery inspection requires Windows",
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    struct Fake {
        original: PathBuf,
        replace: bool,
        success: bool,
    }
    impl Trash for Fake {
        fn recycle(&self, staged: &Path) -> Result<()> {
            assert_eq!(fs::read(staged).unwrap(), b"synthetic original");
            if self.replace {
                fs::write(&self.original, b"independent replacement").unwrap();
            }
            if self.success {
                fs::remove_file(staged)?;
                Ok(())
            } else {
                Err(ApiError::new("recycle_failed", "synthetic failure"))
            }
        }
    }
    #[test]
    fn failed_trash_restores_when_original_name_is_free() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("sample.txt");
        fs::write(&original, b"synthetic original").unwrap();
        let hash = crate::preview::content_hash(&original).unwrap();
        let fake = Fake {
            original: original.clone(),
            replace: false,
            success: false,
        };
        let receipt = recycle_verified(&original, root.path(), &hash, &fake).unwrap();
        assert_eq!(receipt.status, "restored");
        assert!(!receipt.recycled);
        assert_eq!(fs::read(&original).unwrap(), b"synthetic original");
        assert!(
            !root
                .path()
                .join(format!(".katran-recovery-{}", receipt.operation_id))
                .exists()
        );
    }
    #[test]
    fn occupied_original_is_preserved_with_recovery_file() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("sample.txt");
        fs::write(&original, b"synthetic original").unwrap();
        let hash = crate::preview::content_hash(&original).unwrap();
        let fake = Fake {
            original: original.clone(),
            replace: true,
            success: false,
        };
        let receipt = recycle_verified(&original, root.path(), &hash, &fake).unwrap();
        assert_eq!(receipt.status, "recovery_required");
        assert_eq!(fs::read(&original).unwrap(), b"independent replacement");
        assert_eq!(
            fs::read(receipt.recovery_path.as_ref().unwrap()).unwrap(),
            b"synthetic original"
        );
        let listing = list_recovery(root.path()).unwrap();
        assert_eq!(listing["items"][0]["staged_state"], "verified");
        assert_eq!(
            listing["items"][0]["recovery_path"].as_str().unwrap(),
            receipt.recovery_path.unwrap().to_string_lossy()
        );
    }
    #[test]
    fn completed_journals_do_not_hide_verified_unresolved_recovery() {
        let root = tempfile::tempdir().unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let original = canonical.join("unresolved.png");
        fs::write(&original, b"synthetic original").unwrap();
        let hash = crate::preview::content_hash(&original).unwrap();
        let fake = Fake {
            original: original.clone(),
            replace: true,
            success: false,
        };
        let receipt = recycle_verified(&original, &canonical, &hash, &fake).unwrap();
        let recovery = receipt.recovery_path.unwrap();
        for n in 0..1000 {
            let id = format!("completed-{n}");
            let stage = canonical.join(format!(".katran-recovery-{id}"));
            fs::create_dir(&stage).unwrap();
            fs::create_dir(stage.join("data")).unwrap();
            let source = canonical.join(format!("completed-{n}.png"));
            let staged = stage.join("data").join(source.file_name().unwrap());
            let prepared = serde_json::json!({"operation_id":id,"original_path":source,"staged_path":staged,"hash":"00".repeat(32),"identity":{"volume":1,"index":1}});
            let outcome = serde_json::json!({"receipt":{"operation_id":id,"original_path":source,"status":"recycled","original_removed":true,"recycled":true,"recovery_path":null,"error":null}});
            fs::write(
                stage.join("prepared.json"),
                serde_json::to_vec(&prepared).unwrap(),
            )
            .unwrap();
            fs::write(
                stage.join("outcome.json"),
                serde_json::to_vec(&outcome).unwrap(),
            )
            .unwrap();
        }
        let listing = list_recovery(&canonical).unwrap();
        let items = listing["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0]["recovery_path"].as_str(),
            Some(recovery.to_str().unwrap())
        );
        assert_eq!(fs::read(&recovery).unwrap(), b"synthetic original");
        assert_eq!(fs::read(&original).unwrap(), b"independent replacement");
        assert!(canonical.join(".katran-recovery-completed-0").exists());
    }
    #[test]
    fn mismatched_completed_receipt_remains_visible() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let stage = root.join(".katran-recovery-example");
        fs::create_dir(&stage).unwrap();
        fs::create_dir(stage.join("data")).unwrap();
        let original = root.join("example.png");
        let staged = stage.join("data").join("example.png");
        let prepared = serde_json::json!({"operation_id":"example","original_path":original,"staged_path":staged,"hash":"00".repeat(32),"identity":{"volume":1,"index":1}});
        let outcome = serde_json::json!({"receipt":{"operation_id":"other","original_path":original,"status":"recycled","original_removed":true,"recycled":true,"recovery_path":null,"error":null}});
        fs::write(
            stage.join("prepared.json"),
            serde_json::to_vec(&prepared).unwrap(),
        )
        .unwrap();
        fs::write(
            stage.join("outcome.json"),
            serde_json::to_vec(&outcome).unwrap(),
        )
        .unwrap();
        let listing = list_recovery(&root).unwrap();
        assert_eq!(listing["items"].as_array().unwrap().len(), 1);
        assert_eq!(listing["items"][0]["operation_id"], "example");
    }
    #[test]
    fn fake_success_receipt_requires_staged_file_to_be_consumed() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("sample.txt");
        fs::write(&original, b"synthetic original").unwrap();
        let hash = crate::preview::content_hash(&original).unwrap();
        let fake = Fake {
            original: original.clone(),
            replace: false,
            success: true,
        };
        let receipt = recycle_verified(&original, root.path(), &hash, &fake).unwrap();
        assert_eq!(receipt.status, "recycled");
        assert!(!original.exists());
        assert!(receipt.recycled);
        assert!(
            !root
                .path()
                .join(format!(".katran-recovery-{}", receipt.operation_id))
                .exists()
        );
    }
    #[test]
    fn unknown_stage_contents_are_never_removed() {
        struct Extra;
        impl Trash for Extra {
            fn recycle(&self, staged: &Path) -> Result<()> {
                let stage = staged.parent().unwrap().parent().unwrap();
                fs::write(stage.join("unknown.bin"), b"preserve")?;
                fs::remove_file(staged)?;
                Ok(())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("sample.txt");
        fs::write(&original, b"synthetic original").unwrap();
        let hash = crate::preview::content_hash(&original).unwrap();
        let receipt = recycle_verified(&original, root.path(), &hash, &Extra).unwrap();
        assert_eq!(receipt.status, "recycled");
        let stage = root
            .path()
            .join(format!(".katran-recovery-{}", receipt.operation_id));
        assert_eq!(fs::read(stage.join("unknown.bin")).unwrap(), b"preserve");
        assert!(stage.join("prepared.json").exists());
        assert!(stage.join("outcome.json").exists());
    }
}

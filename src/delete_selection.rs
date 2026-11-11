//! Explicit original-file deletion for ShareX references, through the dispatcher.
use crate::*;
use serde_json::Value;

impl Service {
    pub(crate) fn delete_selection(&mut self, ids: Vec<i64>, confirmed: bool) -> Result<Value> {
        match self.begin_delete_selection(ids, confirmed)? {
            crate::delete_jobs::DeleteStart::Immediate(value) => Ok(value),
            crate::delete_jobs::DeleteStart::Prepare(request) => {
                let prepared = crate::delete_jobs::prepare(request);
                let authorized = self.authorize_delete(prepared);
                let deleted =
                    crate::delete_jobs::apply(authorized, std::sync::Arc::clone(&self.trash));
                self.finish_delete(deleted)
            }
        }
    }
}

/// Hash and delete the same opened Windows file. Replacement/writes are denied
/// until FileDispositionInfo marks this handle, closing the pathname TOCTOU gap.
pub(crate) struct LockedOriginal {
    file: fs::File,
    #[cfg(not(windows))]
    path: PathBuf,
}

/// Windows volume/file identity captured from the opened handle, not the pathname.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LockedIdentity {
    pub volume: u32,
    pub index: u64,
}

#[cfg(windows)]
pub(crate) fn locked_identity(original: &LockedOriginal) -> Result<LockedIdentity> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(original.file.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if info.dwFileAttributes & (0x10 | 0x400) != 0 {
        return Err(ApiError::new(
            "unsafe_source",
            "Source is a directory or reparse point",
        ));
    }
    Ok(LockedIdentity {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

#[cfg(windows)]
pub(crate) fn locked_path(original: &LockedOriginal) -> Result<PathBuf> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
    let mut name = vec![0u16; 32768];
    let count = unsafe {
        GetFinalPathNameByHandleW(
            original.file.as_raw_handle(),
            name.as_mut_ptr(),
            name.len() as u32,
            0,
        )
    } as usize;
    if count == 0 || count >= name.len() {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(PathBuf::from(
        String::from_utf16(&name[..count]).map_err(|e| ApiError::new("unsafe_source", e))?,
    ))
}

/// Rename the already validated handle without replacement. The caller must
/// validate and secure the destination parent on the same volume. The handle is
/// retained on both success and failure, allowing safe recovery decisions.
#[cfg(windows)]
pub(crate) fn rename_locked_original(
    original: &mut LockedOriginal,
    destination: &Path,
) -> Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
    };
    if !destination.is_absolute() {
        return Err(ApiError::new(
            "invalid_argument",
            "Rename destination must be absolute",
        ));
    }
    let mut name: Vec<u16> = destination.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err(ApiError::new(
            "invalid_argument",
            "Rename destination contains NUL",
        ));
    }
    let name_bytes = name
        .len()
        .checked_mul(2)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| ApiError::new("invalid_argument", "Rename path too long"))?;
    name.push(0);
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let size = offset
        .checked_add(name.len() * 2)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| ApiError::new("invalid_argument", "Rename buffer too large"))?;
    let mut storage = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = name_bytes;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            name.len(),
        );
        if SetFileInformationByHandle(
            original.file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            size,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
/// Retain the same deny-write/deny-replacement handle through authorization.
pub(crate) fn prepare_locked_original(
    path: &Path,
    folder: &Path,
    expected: &str,
) -> Result<LockedOriginal> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .access_mode(0x80000000 | 0x00010000)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(path)?
    };
    #[cfg(not(windows))]
    let mut file = fs::File::open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(ApiError::new(
            "unsafe_source",
            "Opened original is not a regular file",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
        let mut name = vec![0u16; 32768];
        let count = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                name.as_mut_ptr(),
                name.len() as u32,
                0,
            )
        } as usize;
        if count == 0 || count >= name.len() {
            return Err(std::io::Error::last_os_error().into());
        }
        let resolved = PathBuf::from(
            String::from_utf16(&name[..count])
                .map_err(|error| ApiError::new("unsafe_source", error))?,
        );
        if !resolved.starts_with(folder) || resolved == folder {
            return Err(ApiError::new(
                "unsafe_source",
                "Opened original escaped configured folder",
            ));
        }
    }
    #[cfg(not(windows))]
    if !path.canonicalize()?.starts_with(folder) {
        return Err(ApiError::new(
            "unsafe_source",
            "Opened original escaped configured folder",
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != expected {
        return Err(ApiError::new(
            "stale_selection",
            "Source contents changed; refresh before deleting",
        ));
    }
    Ok(LockedOriginal {
        file,
        #[cfg(not(windows))]
        path: path.into(),
    })
}
pub(crate) fn dispose_locked_original(original: LockedOriginal) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
        };
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        if unsafe {
            SetFileInformationByHandle(
                original.file.as_raw_handle(),
                FileDispositionInfo,
                (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    {
        drop(original.file);
        fs::remove_file(original.path)?;
    }
    Ok(())
}
#[cfg(all(test, windows))]
fn remove_locked_original_with_hook(
    path: &Path,
    folder: &Path,
    expected: &str,
    before_disposition: impl FnOnce(),
) -> Result<()> {
    let original = prepare_locked_original(path, folder, expected)?;
    before_disposition();
    dispose_locked_original(original)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn opened_original_cannot_be_replaced_or_written_between_hash_and_disposition() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("original.png");
        fs::write(&path, b"verified synthetic original").unwrap();
        let replacement = temp.path().join("replacement.png");
        fs::write(&replacement, b"preserve replacement").unwrap();
        let hash = preview::content_hash(&path).unwrap();
        let folder = temp.path().canonicalize().unwrap();
        remove_locked_original_with_hook(&path, &folder, &hash, || {
            assert!(fs::write(&path, b"changed").is_err());
            assert!(fs::remove_file(&path).is_err());
            assert!(fs::rename(&replacement, &path).is_err());
        })
        .unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read(replacement).unwrap(), b"preserve replacement");
    }

    #[test]
    fn handle_rename_preserves_identity_and_never_replaces_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source 🐄.txt");
        let staged = root.join("staged 🐄.txt");
        let occupied = root.join("occupied.txt");
        fs::write(&source, b"synthetic original").unwrap();
        fs::write(&occupied, b"do not replace").unwrap();
        let hash = preview::content_hash(&source).unwrap();
        let mut handle = prepare_locked_original(&source, &root, &hash).unwrap();
        let identity = locked_identity(&handle).unwrap();
        assert!(rename_locked_original(&mut handle, &occupied).is_err());
        assert_eq!(fs::read(&occupied).unwrap(), b"do not replace");
        rename_locked_original(&mut handle, &staged).unwrap();
        assert_eq!(locked_identity(&handle).unwrap(), identity);
        assert!(!source.exists());
        assert!(fs::write(&staged, b"replace").is_err());
        rename_locked_original(&mut handle, &source).unwrap();
        drop(handle);
        assert_eq!(fs::read(&source).unwrap(), b"synthetic original");
    }
}

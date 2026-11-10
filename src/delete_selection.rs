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
}

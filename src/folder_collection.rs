//! Handle-bound, no-replace publication into an explicitly selected Folder.
//! Windows only: staging is created relative to held directory handles and
//! cleanup marks only handles owned by this operation for deletion.
use crate::{ApiError, Result};
use baloh_collection::bs::FileEntry;
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
pub(crate) struct PreparedOutput;
#[cfg(not(windows))]
impl PreparedOutput {
    pub(crate) fn publish(self) -> Result<PathBuf> {
        Err(ApiError::new(
            "unsupported_platform",
            "Folder publication requires Windows",
        ))
    }
}
#[cfg(not(windows))]
pub(crate) fn prepare(_: &Path, _: &str, _: &[u8]) -> Result<PreparedOutput> {
    Err(ApiError::new(
        "unsupported_platform",
        "Folder publication requires Windows",
    ))
}
#[cfg(not(windows))]
pub(crate) fn prepare_extraction(_: &Path, _: &str, _: Vec<FileEntry>) -> Result<PreparedOutput> {
    Err(ApiError::new(
        "unsupported_platform",
        "Folder publication requires Windows",
    ))
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::{
        collections::HashSet,
        ffi::c_void,
        fs::{self, File, OpenOptions},
        io::{self, Write},
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::{AsRawHandle, FromRawHandle},
        },
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_DISPOSITION_INFO, FILE_RENAME_INFO, FileDispositionInfo,
        FileRenameInfo, GetFileInformationByHandle, SetFileInformationByHandle,
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    const OPEN_REPARSE: u32 = 0x00200000;
    const BACKUP_SEMANTICS: u32 = 0x02000000;
    const DELETE: u32 = 0x00010000;
    const SYNCHRONIZE: u32 = 0x00100000;
    const GENERIC_READ: u32 = 0x80000000;
    const GENERIC_WRITE: u32 = 0x40000000;

    fn fail(message: &str) -> ApiError {
        ApiError::new("folder_collection_error", message)
    }
    fn valid_leaf(name: &str) -> bool {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.ends_with(['.', ' '])
            || name.encode_utf16().count() > 255
            || name.chars().any(|c| {
                c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            })
        {
            return false;
        }
        let upper = name
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        if matches!(
            upper.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) {
            return false;
        }
        for prefix in ["COM", "LPT"] {
            if let Some(suffix) = upper.strip_prefix(prefix)
                && matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            {
                return false;
            }
        }
        true
    }
    fn identity(file: &File, directory: bool) -> Result<(u32, u64)> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        if info.dwFileAttributes & 0x400 != 0 || (info.dwFileAttributes & 0x10 != 0) != directory {
            return Err(fail(
                "Folder or staging object is a reparse point or wrong type",
            ));
        }
        Ok((
            info.dwVolumeSerialNumber,
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        ))
    }
    fn open_root(folder: &Path) -> Result<(PathBuf, File, (u32, u64))> {
        if !folder.is_absolute() {
            return Err(fail("Folder path must be absolute"));
        }
        let meta = fs::symlink_metadata(folder)?;
        if !meta.is_dir() || meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0 {
            return Err(fail("Folder is not a real directory"));
        }
        let path = folder.canonicalize()?;
        let dir = OpenOptions::new()
            .access_mode(0x80)
            .share_mode(3)
            .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE)
            .open(&path)?;
        let id = identity(&dir, true)?;
        Ok((path, dir, id))
    }
    fn verify_root(path: &Path, expected: (u32, u64)) -> Result<()> {
        let (canonical, _, current) = open_root(path)?;
        if canonical != path || current != expected {
            return Err(ApiError::new(
                "source_changed",
                "Folder changed before publication",
            ));
        }
        Ok(())
    }

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *mut u16,
    }
    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root: *mut c_void,
        name: *mut UnicodeString,
        attributes: u32,
        security: *mut c_void,
        qos: *mut c_void,
    }
    #[repr(C)]
    struct IoStatus {
        status: usize,
        information: usize,
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtCreateFile(
            handle: *mut *mut c_void,
            access: u32,
            attributes: *mut ObjectAttributes,
            status: *mut IoStatus,
            allocation: *mut i64,
            file_attributes: u32,
            share: u32,
            disposition: u32,
            options: u32,
            ea: *mut c_void,
            ea_length: u32,
        ) -> i32;
        fn RtlNtStatusToDosError(status: i32) -> u32;
    }
    /// Atomically create a leaf relative to the already opened directory.
    fn open_relative(parent: &File, name: &str, directory: bool, create: bool) -> io::Result<File> {
        if !valid_leaf(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsafe staging filename",
            ));
        }
        let mut wide: Vec<u16> = std::ffi::OsStr::new(name).encode_wide().collect();
        let len = u16::try_from(wide.len() * 2)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Filename too long"))?;
        let mut name = UnicodeString {
            length: len,
            maximum_length: len,
            buffer: wide.as_mut_ptr(),
        };
        let mut attrs = ObjectAttributes {
            length: std::mem::size_of::<ObjectAttributes>() as u32,
            root: parent.as_raw_handle(),
            name: &mut name,
            attributes: 0x40,
            security: std::ptr::null_mut(),
            qos: std::ptr::null_mut(),
        };
        let mut handle = std::ptr::null_mut();
        let mut status = IoStatus {
            status: 0,
            information: 0,
        };
        let options = 0x20 | OPEN_REPARSE | if directory { 1 } else { 0x40 };
        let code = unsafe {
            NtCreateFile(
                &mut handle,
                GENERIC_READ | GENERIC_WRITE | DELETE | SYNCHRONIZE,
                &mut attrs,
                &mut status,
                std::ptr::null_mut(),
                if directory { 0x10 } else { 0x80 },
                if directory { 3 } else { 1 },
                if create { 2 } else { 1 },
                options,
                std::ptr::null_mut(),
                0,
            )
        };
        if code < 0 {
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(code) } as i32,
            ));
        }
        let file = unsafe { File::from_raw_handle(handle) };
        identity(&file, directory).map_err(|error| io::Error::other(error.message))?;
        Ok(file)
    }
    fn delete_owned(file: &File) {
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            );
        }
    }
    fn rename_into(file: &File, destination: &Path) -> io::Result<()> {
        let wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
        let bytes = wide
            .len()
            .checked_mul(2)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Filename too long"))?;
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let size = offset
            .checked_add(bytes as usize + 2)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Filename too long"))?;
        let mut storage = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = std::ptr::null_mut();
            (*info).FileNameLength = bytes;
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                storage.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
                wide.len(),
            );
            if SetFileInformationByHandle(file.as_raw_handle(), FileRenameInfo, info.cast(), size)
                == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    fn temporary_name() -> String {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!(
            ".katran-stage-{:x}-{time:x}-{:x}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }
    fn create_stage(parent: &File, directory: bool) -> Result<File> {
        for _ in 0..128 {
            match open_relative(parent, &temporary_name(), directory, true) {
                Ok(file) => return Ok(file),
                Err(e)
                    if e.kind() == io::ErrorKind::AlreadyExists
                        || matches!(e.raw_os_error(), Some(80 | 183)) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(fail("Cannot reserve a staging name"))
    }
    fn output_name(base: &str, ext: Option<&str>, index: usize) -> String {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!(" ({})", index + 1)
        };
        fn prefix(text: &str, limit: usize) -> String {
            let mut result = String::new();
            let mut used = 0;
            for ch in text.chars() {
                if used + ch.len_utf16() > limit {
                    break;
                }
                used += ch.len_utf16();
                result.push(ch);
            }
            result
        }
        let available = 255usize.saturating_sub(suffix.encode_utf16().count());
        let ext = ext.map(|value| prefix(value, available.saturating_sub(2)));
        let ext_units = ext
            .as_ref()
            .map_or(0, |value| value.encode_utf16().count() + 1);
        let base = prefix(base, available.saturating_sub(ext_units));
        match ext {
            Some(ext) => format!("{base}{suffix}.{ext}"),
            None => format!("{base}{suffix}"),
        }
    }

    struct Child {
        name: String,
        id: (u32, u64),
        file: Option<File>,
    }
    pub(crate) struct PreparedOutput {
        root_path: PathBuf,
        _root: File,
        root_id: (u32, u64),
        stage: Option<File>,
        children: Vec<Child>,
        base: String,
        extension: Option<String>,
    }
    impl PreparedOutput {
        pub(crate) fn publish(mut self) -> Result<PathBuf> {
            verify_root(&self.root_path, self.root_id)?;
            // Windows refuses renaming a nonempty directory while any of its
            // children are open, even when those handles share delete access.
            for child in &mut self.children {
                child.file.take();
            }
            let stage = self
                .stage
                .as_ref()
                .ok_or_else(|| fail("Staging is unavailable"))?;
            identity(stage, self.extension.is_none())?;
            for n in 0..10000 {
                let name = output_name(&self.base, self.extension.as_deref(), n);
                if !valid_leaf(&name) {
                    return Err(fail("Output filename is invalid"));
                }
                verify_root(&self.root_path, self.root_id)?;
                let destination = self.root_path.join(name);
                match rename_into(stage, &destination) {
                    Ok(()) => {
                        self.stage.take();
                        self.children.clear();
                        return Ok(destination);
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::AlreadyExists
                            || matches!(e.raw_os_error(), Some(80 | 183)) =>
                    {
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(fail("Cannot find a free output name"))
        }
    }
    impl Drop for PreparedOutput {
        fn drop(&mut self) {
            for child in &mut self.children {
                if let Some(file) = child.file.take() {
                    delete_owned(&file);
                } else if let Some(stage) = self.stage.as_ref()
                    && let Ok(file) = open_relative(stage, &child.name, false, false)
                    && identity(&file, false).ok() == Some(child.id)
                {
                    delete_owned(&file);
                }
            }
            self.children.clear();
            if let Some(stage) = self.stage.take() {
                delete_owned(&stage);
            }
        }
    }
    pub(crate) fn prepare(folder: &Path, extension: &str, bytes: &[u8]) -> Result<PreparedOutput> {
        if !matches!(extension, "bull" | "bs") || bytes.len() > 256 * 1024 * 1024 {
            return Err(fail("Invalid collection format or size"));
        }
        let (root_path, root, root_id) = open_root(folder)?;
        let stage = create_stage(&root, false)?;
        let mut result = PreparedOutput {
            root_path,
            _root: root,
            root_id,
            stage: Some(stage),
            children: Vec::new(),
            base: "Collection".into(),
            extension: Some(extension.into()),
        };
        result.stage.as_mut().unwrap().write_all(bytes)?;
        result.stage.as_ref().unwrap().sync_all()?;
        Ok(result)
    }
    pub(crate) fn prepare_extraction(
        folder: &Path,
        archive_stem: &str,
        entries: Vec<FileEntry>,
    ) -> Result<PreparedOutput> {
        if !valid_leaf(archive_stem)
            || archive_stem.encode_utf16().count() > 190
            || entries.is_empty()
            || entries.len() > baloh_collection::bs::MAX_FILES
        {
            return Err(fail("Invalid archive name or file count"));
        }
        let mut total = 0usize;
        for entry in &entries {
            if !valid_leaf(&entry.name) || entry.bytes.len() > baloh_collection::bs::MAX_FILE_BYTES
            {
                return Err(fail("Invalid extracted filename or size"));
            }
            total = total
                .checked_add(entry.bytes.len())
                .ok_or_else(|| fail("Extraction size overflow"))?;
            if total > baloh_collection::bs::MAX_TOTAL_BYTES {
                return Err(fail("Extraction exceeds size limit"));
            }
        }
        let (root_path, root, root_id) = open_root(folder)?;
        let stage = create_stage(&root, true)?;
        let mut result = PreparedOutput {
            root_path,
            _root: root,
            root_id,
            stage: Some(stage),
            children: Vec::new(),
            base: format!("{archive_stem} files"),
            extension: None,
        };
        let mut used = HashSet::new();
        for entry in entries {
            let path = Path::new(&entry.name);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| fail("Invalid file name"))?;
            let ext = path.extension().and_then(|s| s.to_str());
            let mut created = None;
            for n in 0..10000 {
                let name = if n == 0 {
                    entry.name.clone()
                } else {
                    output_name(stem, ext, n)
                };
                if !valid_leaf(&name) {
                    return Err(fail("Duplicate filename exceeds length limit"));
                }
                if !used.insert(name.to_lowercase()) {
                    continue;
                }
                match open_relative(result.stage.as_ref().unwrap(), &name, false, true) {
                    Ok(file) => {
                        created = Some((name, file));
                        break;
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::AlreadyExists
                            || matches!(e.raw_os_error(), Some(80 | 183)) =>
                    {
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            let (name, file) = created.ok_or_else(|| fail("Cannot resolve duplicate filename"))?;
            let id = identity(&file, false)?;
            result.children.push(Child {
                name,
                id,
                file: Some(file),
            });
            let file = result.children.last_mut().unwrap().file.as_mut().unwrap();
            file.write_all(&entry.bytes)?;
            file.sync_all()?;
        }
        Ok(result)
    }
}
#[cfg(windows)]
pub(crate) use windows_impl::{PreparedOutput, prepare, prepare_extraction};

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn file_publication_keeps_existing_and_dropped_stage_is_removed() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Collection.bull"), b"existing").unwrap();
        let staged = prepare(root.path(), "bull", b"new").unwrap();
        let result = staged.publish().unwrap();
        assert_eq!(result.file_name().unwrap(), "Collection (2).bull");
        assert_eq!(
            fs::read(root.path().join("Collection.bull")).unwrap(),
            b"existing"
        );
        assert_eq!(fs::read(result).unwrap(), b"new");
        let staged = prepare(root.path(), "bs", b"temporary").unwrap();
        drop(staged);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn extraction_keeps_archive_and_resolves_duplicate_names() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("archive.bs");
        fs::write(&original, b"original").unwrap();
        let items = vec![
            FileEntry {
                name: "a.txt".into(),
                bytes: b"one".to_vec(),
            },
            FileEntry {
                name: "A.txt".into(),
                bytes: b"two".to_vec(),
            },
        ];
        let folder = prepare_extraction(root.path(), "archive", items)
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(fs::read(folder.join("a.txt")).unwrap(), b"one");
        assert_eq!(fs::read(folder.join("A (2).txt")).unwrap(), b"two");
        assert_eq!(fs::read(original).unwrap(), b"original");
    }
}

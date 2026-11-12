//! Read-only preparation for Files pointers. No source or destination is modified.
//!
//! This module deliberately does not authorize moving, copying or deleting files.
//! The eventual effect must use a handle-bound, revision-checked transfer adapter.
use sha2::{Digest, Sha256};
use std::{
    fs,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    pub content_hash: String,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn regular(metadata: &fs::Metadata) -> bool {
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

/// Canonical path is a locator, not proof of stable file identity. Equal bytes
/// or equal locators are not used here to merge pointer records.
pub fn prepare_pointer(path: &Path) -> io::Result<Pointer> {
    if !path.is_absolute() {
        return Err(invalid("Expected an absolute file path"));
    }
    if !regular(&fs::symlink_metadata(path)?) {
        return Err(invalid("Expected a regular file, not a link or directory"));
    }
    let path = fs::canonicalize(path)?;
    let before = fs::symlink_metadata(&path)?;
    if !regular(&before) {
        return Err(invalid("Expected a regular file"));
    }
    let mut file = File::open(&path)?;
    let handle_before = file.metadata()?;
    if !regular(&handle_before)
        || before.len() != handle_before.len()
        || before.modified()? != handle_before.modified()?
    {
        return Err(invalid("Source changed during preparation"));
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
    let after = file.metadata()?;
    let path_after = fs::symlink_metadata(&path)?;
    if !regular(&path_after)
        || after.len() != before.len()
        || after.modified()? != before.modified()?
        || path_after.len() != before.len()
        || path_after.modified()? != before.modified()?
    {
        return Err(invalid("Source changed during preparation"));
    }
    Ok(Pointer {
        path,
        size: before.len(),
        modified: before.modified()?,
        content_hash: format!("{:x}", hash.finalize()),
    })
}

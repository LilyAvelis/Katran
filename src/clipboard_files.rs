//! Explicit paste reads file references; it never imports them into the shelf.
use crate::{ApiError, Result};
use std::path::PathBuf;
pub trait Source: Send {
    fn sequence(&self) -> u32;
    fn files(&self) -> Result<Vec<PathBuf>>;
}
pub struct Native;
impl Source for Native {
    fn sequence(&self) -> u32 {
        crate::capture::sequence_number()
    }
    fn files(&self) -> Result<Vec<PathBuf>> {
        #[cfg(windows)]
        {
            let sequence = self.sequence();
            let _lock = clipboard_win::Clipboard::new_attempts(3)
                .map_err(|e| ApiError::new("clipboard_busy", e))?;
            let paths: Vec<PathBuf> = clipboard_win::get(clipboard_win::formats::FileList)
                .map_err(|e| ApiError::new("clipboard_has_no_files", e))?;
            if self.sequence() != sequence {
                return Err(ApiError::new(
                    "clipboard_changed",
                    "Clipboard changed during paste",
                ));
            }
            Ok(paths)
        }
        #[cfg(not(windows))]
        {
            Err(ApiError::new(
                "unsupported_platform",
                "Clipboard file paste requires Windows",
            ))
        }
    }
}

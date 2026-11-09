//! Clipboard snapshot adapter; API dispatcher owns all ingestion.
use crate::{ApiError, Command, Result, Service, now_ms, origin};
use std::path::PathBuf;
impl Service {
    /// Passive snapshot adapter; it never writes to or clears the system clipboard.
    pub(crate) fn poll_clipboard(&mut self) -> Result<serde_json::Value> {
        if !self.capture_enabled {
            return Ok(serde_json::json!({"captured":0,"state":"paused"}));
        }
        #[cfg(windows)]
        {
            let sequence =
                unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };
            if sequence == 0 || sequence == self.clipboard_sequence {
                return Ok(serde_json::json!({"captured":0}));
            }
            // Leave sequence unacknowledged on contention so the next poll retries.
            let lock = clipboard_win::Clipboard::new_attempts(3)
                .map_err(|e| ApiError::new("clipboard_busy", e))?;
            let files: std::result::Result<Vec<PathBuf>, _> =
                clipboard_win::get(clipboard_win::formats::FileList);
            let Some(origin) = origin::snapshot_locked(sequence) else {
                return Ok(serde_json::json!({"captured":0,"state":"retry"}));
            };
            drop(lock);
            if let Ok(files) = files {
                let mut results = Vec::new();
                for path in files {
                    let display = path.clone();
                    let result = self.dispatch(Command::IngestWithOrigin {
                        origin: origin.clone(),
                        command: Box::new(Command::IngestPath {
                            path,
                            source: "clipboard_file".into(),
                        }),
                    });
                    results.push(match result {
                        Ok(event) => serde_json::json!({"ok":true,"path":display,"event":event}),
                        Err(error) => serde_json::json!({"ok":false,"path":display,"error":error}),
                    });
                }
                self.clipboard_sequence = sequence;
                return Ok(
                    serde_json::json!({"captured":results.iter().filter(|r| r["ok"]==true).count(),"items":results}),
                );
            }
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| ApiError::new("clipboard_error", e))?;
            match clipboard.get_image() {
                Ok(bitmap) => {
                    use image::ImageEncoder;
                    let width = u32::try_from(bitmap.width)
                        .map_err(|_| ApiError::new("invalid_image", "Image too wide"))?;
                    let height = u32::try_from(bitmap.height)
                        .map_err(|_| ApiError::new("invalid_image", "Image too tall"))?;
                    let mut png = Vec::new();
                    image::codecs::png::PngEncoder::new(&mut png)
                        .write_image(
                            &bitmap.bytes,
                            width,
                            height,
                            image::ExtendedColorType::Rgba8,
                        )
                        .map_err(|e| ApiError::new("image_error", e))?;
                    // A changing clipboard must not be acknowledged as this snapshot.
                    if unsafe {
                        windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber()
                    } != sequence
                    {
                        return Ok(serde_json::json!({"captured":0,"state":"retry"}));
                    }
                    let event = self.dispatch(Command::IngestWithOrigin {
                        origin,
                        command: Box::new(Command::IngestBytes {
                            name: format!("Clipboard-{}.png", now_ms()),
                            bytes: png,
                            source: "clipboard_image".into(),
                        }),
                    })?;
                    self.clipboard_sequence = sequence;
                    Ok(serde_json::json!({"captured":1,"event":event}))
                }
                Err(arboard::Error::ContentNotAvailable) => {
                    self.clipboard_sequence = sequence;
                    Ok(serde_json::json!({"captured":0,"state":"unsupported_format"}))
                }
                Err(e) => Err(ApiError::new("clipboard_error", e)),
            }
        }
        #[cfg(not(windows))]
        Err(ApiError::new(
            "unsupported_platform",
            "Windows clipboard adapter required",
        ))
    }
}

//! Passive native clipboard preparation. Ingestion remains a dispatcher commit.
use crate::{
    ApiError, Result, Service, now_ms,
    origin::{self, Origin},
};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Private owned worker receipt: never deserialize clipboard assertions from a client.
pub(crate) struct CaptureSnapshot {
    pub(crate) sequence: u32,
    pub(crate) payload: CapturePayload,
}
pub(crate) enum CapturePayload {
    Unchanged,
    Retry,
    ExcludedSharex,
    Unsupported,
    Files {
        origin: Origin,
        paths: Vec<PathBuf>,
    },
    PreparedFiles {
        items: Vec<(PathBuf, Result<crate::jobs_service::PreparedReference>)>,
    },
    PreparedImage(crate::storage::PreparedBlob),
    Image {
        origin: Origin,
        name: String,
        png: Vec<u8>,
        content_hash: String,
    },
}
pub(crate) fn sequence_number() -> u32 {
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
    }
    #[cfg(not(windows))]
    {
        0
    }
}
/// Reads the clipboard and encodes/hashes owned image bytes without a Service/DB.
pub(crate) fn prepare_snapshot(last_sequence: u32) -> Result<CaptureSnapshot> {
    #[cfg(windows)]
    {
        let sequence = sequence_number();
        let snapshot = |payload| CaptureSnapshot { sequence, payload };
        if sequence == 0 || sequence == last_sequence {
            return Ok(snapshot(CapturePayload::Unchanged));
        }
        let lock = clipboard_win::Clipboard::new_attempts(3)
            .map_err(|e| ApiError::new("clipboard_busy", e))?;
        let files: std::result::Result<Vec<PathBuf>, _> =
            clipboard_win::get(clipboard_win::formats::FileList);
        let Some(origin) = origin::snapshot_locked(sequence) else {
            return Ok(snapshot(CapturePayload::Retry));
        };
        drop(lock);
        if origin
            .source_app
            .as_deref()
            .is_some_and(|app| app.eq_ignore_ascii_case("sharex"))
        {
            return Ok(snapshot(CapturePayload::ExcludedSharex));
        }
        if let Ok(paths) = files {
            return Ok(snapshot(CapturePayload::Files { origin, paths }));
        }
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| ApiError::new("clipboard_error", e))?;
        match clipboard.get_image() {
            Ok(bitmap) => {
                use image::ImageEncoder;
                use sha2::{Digest, Sha256};
                let width =
                    u32::try_from(bitmap.width).map_err(|e| ApiError::new("invalid_image", e))?;
                let height =
                    u32::try_from(bitmap.height).map_err(|e| ApiError::new("invalid_image", e))?;
                let mut png = Vec::new();
                image::codecs::png::PngEncoder::new(&mut png)
                    .write_image(
                        &bitmap.bytes,
                        width,
                        height,
                        image::ExtendedColorType::Rgba8,
                    )
                    .map_err(|e| ApiError::new("image_error", e))?;
                if sequence_number() != sequence {
                    return Ok(snapshot(CapturePayload::Retry));
                }
                let content_hash = format!("{:x}", Sha256::digest(&png));
                Ok(snapshot(CapturePayload::Image {
                    origin,
                    name: format!("Clipboard-{}.png", now_ms()),
                    png,
                    content_hash,
                }))
            }
            Err(arboard::Error::ContentNotAvailable) => Ok(snapshot(CapturePayload::Unsupported)),
            Err(error) => Err(ApiError::new("clipboard_error", error)),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = last_sequence;
        Err(ApiError::new(
            "unsupported_platform",
            "Windows clipboard adapter required",
        ))
    }
}
/// Production jobs prepare storage/identity while the dispatcher stays available.
pub(crate) fn prepare_snapshot_for_ingest(
    last_sequence: u32,
    root: &std::path::Path,
    quota: u64,
    references: &[PathBuf],
) -> Result<CaptureSnapshot> {
    prepare_owned_snapshot(prepare_snapshot(last_sequence)?, root, quota, references)
}
fn prepare_owned_snapshot(
    mut snapshot: CaptureSnapshot,
    root: &std::path::Path,
    quota: u64,
    references: &[PathBuf],
) -> Result<CaptureSnapshot> {
    snapshot.payload = match snapshot.payload {
        CapturePayload::Files { origin, paths } => {
            let items = paths
                .into_iter()
                .map(|path| {
                    let prepared = crate::jobs_service::prepare_reference(
                        path.clone(),
                        "clipboard_file".into(),
                        origin.clone(),
                    );
                    (path, prepared)
                })
                .collect();
            CapturePayload::PreparedFiles { items }
        }
        CapturePayload::Image {
            origin,
            name,
            png,
            content_hash,
        } => {
            debug_assert_eq!(content_hash.len(), 64);
            CapturePayload::PreparedImage(crate::storage::prepare_bytes(
                root,
                quota,
                crate::storage::ByteIngest {
                    name: &name,
                    source: "clipboard_image",
                    bytes: &png,
                    origin: &origin,
                },
                references,
            )?)
        }
        payload => payload,
    };
    Ok(snapshot)
}
impl Service {
    /// Legacy explicit polling shares the native preparation/dispatcher commit path.
    pub(crate) fn poll_clipboard(&mut self) -> Result<Value> {
        if !self.capture_enabled {
            return Ok(json!({"captured":0,"state":"paused"}));
        }
        let references = self.reference_paths()?;
        let snapshot = prepare_snapshot_for_ingest(
            self.clipboard_sequence,
            &self.root,
            self.quota,
            &references,
        )?;
        self.commit_capture_snapshot(snapshot, sequence_number())
    }
    pub(crate) fn commit_capture_snapshot(
        &mut self,
        snapshot: CaptureSnapshot,
        current_sequence: u32,
    ) -> Result<Value> {
        if !self.capture_enabled {
            return Ok(json!({"captured":0,"state":"paused"}));
        }
        if snapshot.sequence != current_sequence {
            return Ok(json!({"captured":0,"state":"retry"}));
        }
        let sequence = snapshot.sequence;
        let result = match snapshot.payload {
            CapturePayload::Unchanged => return Ok(json!({"captured":0})),
            CapturePayload::Retry => return Ok(json!({"captured":0,"state":"retry"})),
            CapturePayload::ExcludedSharex => {
                json!({"captured":0,"state":"excluded_sharex_folder_source"})
            }
            CapturePayload::Unsupported => json!({"captured":0,"state":"unsupported_format"}),
            CapturePayload::PreparedFiles { items } => {
                let mut results = Vec::new();
                for (path, prepared) in items {
                    let result =
                        prepared.and_then(|prepared| self.commit_prepared_reference(prepared));
                    results.push(match result {
                        Ok(event) => json!({"ok":true,"path":path,"event":event}),
                        Err(error) => json!({"ok":false,"path":path,"error":error}),
                    });
                }
                json!({"captured":results.iter().filter(|result|result["ok"]==true).count(),"items":results})
            }
            CapturePayload::PreparedImage(prepared) => {
                let id = crate::storage::commit_prepared_bytes(
                    &mut self.db,
                    &self.root,
                    self.quota,
                    prepared,
                )?;
                json!({"captured":1,"event":self.get(id)?})
            }
            CapturePayload::Files { .. } | CapturePayload::Image { .. } => {
                return Err(ApiError::new(
                    "unprepared_capture",
                    "Capture requires a trusted prepared storage receipt",
                ));
            }
        };
        self.clipboard_sequence = sequence;
        Ok(result)
    }
}

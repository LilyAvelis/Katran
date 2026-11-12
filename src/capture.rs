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
    OwnTable {
        token: String,
    },
    OwnText {
        token: String,
    },
    OwnFile {
        token: String,
    },
    Retry,
    ExcludedSharex,
    Unsupported,
    Files {
        origin: Origin,
        paths: Vec<PathBuf>,
    },
    PreparedFiles {
        origin: Origin,
        items: Vec<(PathBuf, PreparedFile)>,
    },
    PreparedImage(crate::storage::PreparedBlob),
    PreparedText(crate::storage::PreparedBlob),
    PreparedLink(crate::storage::PreparedBlob),
    PreparedTable(crate::storage::PreparedBlob),
    HtmlTable {
        origin: Origin,
        html: String,
    },
    Text {
        origin: Origin,
        text: String,
    },
    Image {
        origin: Origin,
        name: String,
        png: Vec<u8>,
        content_hash: String,
    },
}
pub(crate) enum PreparedFile {
    Image(Result<crate::jobs_service::PreparedReference>),
    File(Result<crate::files_pointer::Pointer>),
}
fn owned_table_payload(token: Option<String>, owned: &[String]) -> Option<CapturePayload> {
    token
        .filter(|token| owned.contains(token))
        .map(|token| CapturePayload::OwnTable { token })
}
#[cfg(any(windows, test))]
fn owned_text_payload(token: Option<String>) -> Option<CapturePayload> {
    token
        .filter(|token| crate::text_fragments::owns_clipboard_token(token))
        .map(|token| CapturePayload::OwnText { token })
}
#[cfg(any(windows, test))]
fn owned_file_payload(token: Option<String>) -> Option<CapturePayload> {
    token
        .filter(|token| crate::copy_out::owns_file_token(token))
        .map(|token| CapturePayload::OwnFile { token })
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
pub(crate) fn prepare_snapshot(
    last_sequence: u32,
    images_enabled: bool,
    text_enabled: bool,
    links_enabled: bool,
    tables_enabled: bool,
    pods_enabled: bool,
    owned_table_tokens: &[String],
) -> Result<CaptureSnapshot> {
    #[cfg(windows)]
    {
        let sequence = sequence_number();
        let snapshot = |payload| CaptureSnapshot { sequence, payload };
        if sequence == 0 || sequence == last_sequence {
            return Ok(snapshot(CapturePayload::Unchanged));
        }
        let lock = clipboard_win::Clipboard::new_attempts(3)
            .map_err(|e| ApiError::new("clipboard_busy", e))?;
        if sequence_number() != sequence {
            return Ok(snapshot(CapturePayload::Retry));
        }
        let token = crate::table_clipboard::read_owner_token_locked()?;
        if sequence_number() != sequence {
            return Ok(snapshot(CapturePayload::Retry));
        }
        if let Some(payload) = owned_table_payload(token.clone(), owned_table_tokens) {
            return Ok(snapshot(payload));
        }
        if let Some(payload) = owned_text_payload(token.clone()) {
            return Ok(snapshot(payload));
        }
        if let Some(payload) = owned_file_payload(token) {
            return Ok(snapshot(payload));
        }
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
        if let Some(html) = read_html_snapshot(sequence)?
            && crate::table_fragments::has_html_table(&html)
        {
            return Ok(snapshot(if tables_enabled {
                CapturePayload::HtmlTable { origin, html }
            } else {
                CapturePayload::Unsupported
            }));
        }
        if !images_enabled {
            return read_text_snapshot(
                sequence,
                origin,
                text_enabled || links_enabled || pods_enabled,
            );
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
            Err(arboard::Error::ContentNotAvailable) => {
                drop(clipboard);
                read_text_snapshot(
                    sequence,
                    origin,
                    text_enabled || links_enabled || pods_enabled,
                )
            }
            Err(error) => Err(ApiError::new("clipboard_error", error)),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (
            last_sequence,
            images_enabled,
            text_enabled,
            links_enabled,
            tables_enabled,
            pods_enabled,
            owned_table_tokens,
        );
        Err(ApiError::new(
            "unsupported_platform",
            "Windows clipboard adapter required",
        ))
    }
}
// Limits are applied before any text allocation, including to oversized HGLOBAL blocks.
#[cfg(windows)]
fn read_html_snapshot(sequence: u32) -> Result<Option<String>> {
    use windows_sys::Win32::System::{
        DataExchange::{GetClipboardData, IsClipboardFormatAvailable, RegisterClipboardFormatW},
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    };
    let name: Vec<u16> = "HTML Format".encode_utf16().chain(Some(0)).collect();
    let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    if format == 0 {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot register HTML format",
        ));
    }
    let lock = clipboard_win::Clipboard::new_attempts(3)
        .map_err(|e| ApiError::new("clipboard_busy", e))?;
    if sequence_number() != sequence {
        return Err(ApiError::new(
            "clipboard_error",
            "Clipboard changed before HTML read",
        ));
    }
    if unsafe { IsClipboardFormatAvailable(format) } == 0 {
        return Ok(None);
    }
    let handle = unsafe { GetClipboardData(format) };
    if handle.is_null() {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot read HTML clipboard data",
        ));
    }
    let length = unsafe { GlobalSize(handle) };
    if length > 8 * 1024 * 1024 {
        return Err(ApiError::new(
            "unsupported_table",
            "Clipboard HTML exceeds 8 MiB",
        ));
    }
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot lock HTML clipboard data",
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) }.to_vec();
    unsafe { GlobalUnlock(handle) };
    drop(lock);
    if sequence_number() != sequence {
        return Err(ApiError::new(
            "clipboard_error",
            "Clipboard changed during HTML read",
        ));
    }
    crate::table_fragments::extract_cf_html(&bytes)
        .map(Some)
        .map_err(|e| ApiError::new("unsupported_table", e))
}
const MAX_TEXT_UTF8: usize = 1024 * 1024;
const MAX_TEXT_UTF16_BYTES: usize = 2 * (MAX_TEXT_UTF8 + 1);

/// CF_UNICODETEXT is UTF-16LE terminated at the first NUL; bytes after it are padding.
fn decode_unicode_text(bytes: &[u8]) -> Result<Option<String>> {
    if bytes.len() > MAX_TEXT_UTF16_BYTES {
        return Err(ApiError::new(
            "text_too_large",
            "Clipboard text exceeds the capture limit",
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]));
    let mut terminated = false;
    let mut text = String::new();
    for ch in std::char::decode_utf16(units.take_while(|&unit| {
        if unit == 0 {
            terminated = true;
            false
        } else {
            true
        }
    })) {
        let ch = ch
            .map_err(|_| ApiError::new("invalid_text", "Clipboard text contains invalid UTF-16"))?;
        if text.len() + ch.len_utf8() > MAX_TEXT_UTF8 {
            return Err(ApiError::new(
                "text_too_large",
                "Clipboard text exceeds the capture limit",
            ));
        }
        text.push(ch);
    }
    if !terminated {
        return Err(ApiError::new(
            "invalid_text",
            "Clipboard Unicode text is not terminated",
        ));
    }
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

#[cfg(windows)]
fn read_text_snapshot(sequence: u32, origin: Origin, enabled: bool) -> Result<CaptureSnapshot> {
    use windows_sys::Win32::System::{
        DataExchange::{GetClipboardData, IsClipboardFormatAvailable},
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    };
    const CF_UNICODETEXT: u32 = 13;
    let snapshot = |payload| CaptureSnapshot { sequence, payload };
    if !enabled {
        return Ok(snapshot(CapturePayload::Unsupported));
    }
    let lock = clipboard_win::Clipboard::new_attempts(3)
        .map_err(|e| ApiError::new("clipboard_busy", e))?;
    if sequence_number() != sequence {
        return Ok(snapshot(CapturePayload::Retry));
    }
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
        return Ok(snapshot(CapturePayload::Unsupported));
    }
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
    if handle.is_null() {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot read Unicode text from clipboard",
        ));
    }
    let size = unsafe { GlobalSize(handle) };
    if size > MAX_TEXT_UTF16_BYTES {
        return Err(ApiError::new(
            "text_too_large",
            "Clipboard text exceeds the capture limit",
        ));
    }
    if size == 0 {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot determine clipboard text size",
        ));
    }
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot lock clipboard text",
        ));
    }
    let decoded =
        decode_unicode_text(unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) });
    unsafe { GlobalUnlock(handle) };
    drop(lock);
    if sequence_number() != sequence {
        return Ok(snapshot(CapturePayload::Retry));
    }
    Ok(snapshot(match decoded? {
        Some(text) => CapturePayload::Text { origin, text },
        None => CapturePayload::Unsupported,
    }))
}

/// Production jobs prepare storage/identity while the dispatcher stays available.
pub(crate) fn prepare_snapshot_for_ingest(
    last_sequence: u32,
    root: &std::path::Path,
    quota: u64,
    references: &[PathBuf],
    features: (bool, bool, bool, bool, bool, bool),
    owned_table_tokens: &[String],
) -> Result<CaptureSnapshot> {
    let (images_enabled, text_enabled, links_enabled, tables_enabled, pods_enabled, files_enabled) =
        features;
    prepare_owned_snapshot_features(
        prepare_snapshot(
            last_sequence,
            images_enabled,
            text_enabled,
            links_enabled,
            tables_enabled,
            pods_enabled,
            owned_table_tokens,
        )?,
        root,
        quota,
        references,
        (
            images_enabled,
            text_enabled,
            links_enabled,
            tables_enabled,
            pods_enabled,
            files_enabled,
        ),
    )
}
fn prepare_owned_snapshot_features(
    mut snapshot: CaptureSnapshot,
    root: &std::path::Path,
    quota: u64,
    references: &[PathBuf],
    features: (bool, bool, bool, bool, bool, bool),
) -> Result<CaptureSnapshot> {
    let (images_enabled, text_enabled, links_enabled, tables_enabled, pods_enabled, files_enabled) =
        features;
    snapshot.payload = match snapshot.payload {
        CapturePayload::HtmlTable { origin, html } => {
            if tables_enabled {
                let table = crate::table_fragments::parse_html(&html)
                    .map_err(|e| ApiError::new("unsupported_table", e))?;
                CapturePayload::PreparedTable(crate::table_service::prepare(
                    root,
                    quota,
                    &table,
                    "clipboard_table",
                    &origin,
                    references,
                )?)
            } else {
                CapturePayload::Unsupported
            }
        }
        CapturePayload::Files { origin, paths } => {
            let items = paths
                .into_iter()
                .filter(|path| {
                    if crate::file_types::is_native_image(path) {
                        images_enabled
                    } else {
                        files_enabled
                    }
                })
                .enumerate()
                .map(|(index, path)| {
                    let too_many = || {
                        ApiError::new(
                            "invalid_argument",
                            "Clipboard file batch exceeds 1000 items",
                        )
                    };
                    let prepared = if crate::file_types::is_native_image(&path) {
                        PreparedFile::Image(if index >= 1000 {
                            Err(too_many())
                        } else {
                            crate::jobs_service::prepare_reference(
                                path.clone(),
                                "clipboard_file".into(),
                                origin.clone(),
                            )
                        })
                    } else {
                        PreparedFile::File(if index >= 1000 {
                            Err(too_many())
                        } else {
                            crate::files_pointer::prepare_pointer(&path).map_err(ApiError::from)
                        })
                    };
                    (path, prepared)
                })
                .collect();
            CapturePayload::PreparedFiles { origin, items }
        }
        CapturePayload::Text { origin, text }
            if crate::link_fragments::recognize(&text).is_some() =>
        {
            if links_enabled {
                CapturePayload::PreparedLink(crate::link_fragments::prepare(
                    root,
                    quota,
                    &text,
                    "clipboard_link",
                    &origin,
                    references,
                )?)
            } else {
                CapturePayload::Unsupported
            }
        }
        CapturePayload::Text { origin, text }
            if (crate::pods::is_pod(&text) && pods_enabled)
                || (!crate::pods::is_pod(&text) && text_enabled) =>
        {
            CapturePayload::PreparedText(crate::text_fragments::prepare(
                root,
                quota,
                &text,
                "clipboard_text",
                &origin,
                references,
            )?)
        }
        CapturePayload::Text { .. } => CapturePayload::Unsupported,
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
            (
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Images),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Text),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Links),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Tables),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Pods),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Files),
            ),
            &self.owned_table_tokens.iter().cloned().collect::<Vec<_>>(),
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
            CapturePayload::OwnTable { token } => {
                if !self.owned_table_tokens.contains(&token) {
                    return Ok(json!({"captured":0,"state":"retry"}));
                }
                json!({"captured":0,"state":"own_publication"})
            }
            CapturePayload::OwnText { token } => {
                #[cfg(any(windows, test))]
                if crate::text_fragments::owns_clipboard_token(&token) {
                    self.clipboard_sequence = sequence;
                    return Ok(json!({"captured":0,"state":"own_publication"}));
                }
                let _ = token;
                return Ok(json!({"captured":0,"state":"retry"}));
            }
            CapturePayload::OwnFile { token } => {
                #[cfg(any(windows, test))]
                if crate::copy_out::owns_file_token(&token) {
                    self.clipboard_sequence = sequence;
                    return Ok(json!({"captured":0,"state":"own_publication"}));
                }
                let _ = token;
                return Ok(json!({"captured":0,"state":"retry"}));
            }
            CapturePayload::Retry => return Ok(json!({"captured":0,"state":"retry"})),
            CapturePayload::ExcludedSharex => {
                json!({"captured":0,"state":"excluded_sharex_folder_source"})
            }
            CapturePayload::Unsupported => json!({"captured":0,"state":"unsupported_format"}),
            CapturePayload::PreparedFiles { origin, items } => {
                let mut results = Vec::new();
                for (path, prepared) in items {
                    let result = match prepared {
                        PreparedFile::Image(prepared) => {
                            if !self
                                .settings
                                .tab_enabled(crate::interaction::ShelfTab::Images)
                            {
                                continue;
                            }
                            prepared.and_then(|value| self.commit_prepared_reference(value))
                        }
                        PreparedFile::File(prepared) => {
                            if !self
                                .settings
                                .tab_enabled(crate::interaction::ShelfTab::Files)
                            {
                                continue;
                            }
                            prepared.and_then(|value| self.commit_file(value, &origin))
                        }
                    };
                    results.push(match result {
                        Ok(event) => json!({"ok":true,"path":path,"event":event}),
                        Err(error) => json!({"ok":false,"path":path,"error":error}),
                    });
                }
                json!({"captured":results.iter().filter(|result|result["ok"]==true).count(),"items":results})
            }
            CapturePayload::PreparedImage(prepared) => {
                if !self
                    .settings
                    .tab_enabled(crate::interaction::ShelfTab::Images)
                {
                    self.clipboard_sequence = sequence;
                    return Ok(json!({"captured":0,"state":"images_disabled"}));
                }
                let id = crate::storage::commit_prepared_bytes(
                    &mut self.db,
                    &self.root,
                    self.quota,
                    prepared,
                )?;
                json!({"captured":1,"event":self.get(id)?})
            }
            CapturePayload::PreparedTable(prepared) => {
                if !self
                    .settings
                    .tab_enabled(crate::interaction::ShelfTab::Tables)
                {
                    self.clipboard_sequence = sequence;
                    return Ok(json!({"captured":0,"state":"tables_disabled"}));
                }
                let id = crate::storage::commit_prepared_bytes(
                    &mut self.db,
                    &self.root,
                    self.quota,
                    prepared,
                )?;
                json!({"captured":1,"event":self.get(id)?})
            }
            CapturePayload::PreparedLink(prepared) => {
                if !self
                    .settings
                    .tab_enabled(crate::interaction::ShelfTab::Links)
                {
                    self.clipboard_sequence = sequence;
                    return Ok(json!({"captured":0,"state":"links_disabled"}));
                }
                let id = crate::storage::commit_prepared_bytes(
                    &mut self.db,
                    &self.root,
                    self.quota,
                    prepared,
                )?;
                json!({"captured":1,"event":self.get(id)?})
            }
            CapturePayload::PreparedText(prepared) => {
                let tab = if prepared.is_pod() {
                    crate::interaction::ShelfTab::Pods
                } else {
                    crate::interaction::ShelfTab::Text
                };
                if !self.settings.tab_enabled(tab) {
                    self.clipboard_sequence = sequence;
                    return Ok(json!({"captured":0,"state":"text_disabled"}));
                }
                let id = crate::storage::commit_prepared_bytes(
                    &mut self.db,
                    &self.root,
                    self.quota,
                    prepared,
                )?;
                json!({"captured":1,"event":self.get(id)?})
            }
            CapturePayload::Files { .. }
            | CapturePayload::Image { .. }
            | CapturePayload::Text { .. }
            | CapturePayload::HtmlTable { .. } => {
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
#[cfg(test)]
mod files_routing_tests {
    use super::*;

    #[test]
    fn mixed_clipboard_paths_use_images_and_files_with_commit_fences() {
        let data = tempfile::tempdir().unwrap();
        let input = tempfile::tempdir().unwrap();
        let image = input.path().join("image.png");
        let file = input.path().join("document.bin");
        let missing = input.path().join("missing.bin");
        std::fs::write(&image, b"synthetic image reference").unwrap();
        std::fs::write(&file, b"synthetic file reference").unwrap();
        let mut service = Service::open(data.path(), 4096).unwrap();
        service.capture_enabled = true;
        let prepare = |files_enabled| {
            prepare_owned_snapshot_features(
                CaptureSnapshot {
                    sequence: 42,
                    payload: CapturePayload::Files {
                        origin: Origin::default(),
                        paths: vec![image.clone(), file.clone(), missing.clone()],
                    },
                },
                &service.root,
                service.quota,
                &[],
                (true, false, false, false, false, files_enabled),
            )
            .unwrap()
        };
        let disabled = prepare(false);
        let snapshot = prepare(true);
        let stale = prepare(true);
        let result = service.commit_capture_snapshot(stale, 43).unwrap();
        assert_eq!(result["state"], "retry");
        let result = service.commit_capture_snapshot(snapshot, 42).unwrap();
        assert_eq!(result["captured"], 2, "{result}");
        assert_eq!(result["items"][0]["event"]["source"], "clipboard_file");
        assert_eq!(result["items"][1]["event"]["source"], "files");
        assert_eq!(result["items"][2]["ok"], false);
        let repeated = prepare_owned_snapshot_features(
            CaptureSnapshot {
                sequence: 43,
                payload: CapturePayload::Files {
                    origin: Origin::default(),
                    paths: vec![file.clone()],
                },
            },
            &service.root,
            service.quota,
            &[],
            (true, false, false, false, false, true),
        )
        .unwrap();
        let again = service.commit_capture_snapshot(repeated, 43).unwrap();
        assert_eq!(
            again["items"][0]["event"]["id"],
            result["items"][1]["event"]["id"]
        );
        let CapturePayload::PreparedFiles { items, .. } = disabled.payload else {
            panic!("expected files")
        };
        assert_eq!(items.len(), 1);
        let delayed = prepare_owned_snapshot_features(
            CaptureSnapshot {
                sequence: 44,
                payload: CapturePayload::Files {
                    origin: Origin::default(),
                    paths: vec![file.clone()],
                },
            },
            &service.root,
            service.quota,
            &[],
            (true, false, false, false, false, true),
        )
        .unwrap();
        service
            .settings
            .hidden_tabs
            .push(crate::interaction::ShelfTab::Files);
        assert_eq!(
            service.commit_capture_snapshot(delayed, 44).unwrap()["captured"],
            0
        );
        assert_eq!(std::fs::read(file).unwrap(), b"synthetic file reference");
    }

    #[test]
    fn own_file_marker_suppresses_later_sequences_only_if_known() {
        let data = tempfile::tempdir().unwrap();
        let mut service = Service::open(data.path(), 4096).unwrap();
        service.capture_enabled = true;
        let token = "cb9e6b67ddf24e259af52e7be5f802b1";
        crate::copy_out::remember_file_token_for_test(token);
        for sequence in [41, 42, 49] {
            let payload = owned_file_payload(Some(token.into())).unwrap();
            let result = service
                .commit_capture_snapshot(CaptureSnapshot { sequence, payload }, sequence)
                .unwrap();
            assert_eq!(result["state"], "own_publication");
        }
        assert!(owned_file_payload(None).is_none());
        assert!(owned_file_payload(Some("00000000000000000000000000000000".into())).is_none());
        let result = service
            .commit_capture_snapshot(
                CaptureSnapshot {
                    sequence: 50,
                    payload: CapturePayload::OwnFile {
                        token: "unknown".into(),
                    },
                },
                50,
            )
            .unwrap();
        assert_eq!(result["state"], "retry");
    }
}

#[cfg(test)]
fn prepare_owned_snapshot(
    snapshot: CaptureSnapshot,
    root: &std::path::Path,
    quota: u64,
    references: &[PathBuf],
    images: bool,
    text: bool,
    links: bool,
) -> Result<CaptureSnapshot> {
    prepare_owned_snapshot_features(
        snapshot,
        root,
        quota,
        references,
        (images, text, links, false, false, true),
    )
}

#[cfg(test)]
mod text_tests {
    use super::*;
    struct TextSink;
    impl crate::text_fragments::ClipboardSink for TextSink {
        fn write(&self, _: &str) -> Result<u32> {
            Ok(40)
        }
    }
    #[test]
    fn own_text_and_links_marker_suppresses_only_owned_publications() {
        for (value, link) in [
            ("a synthetic fragment", false),
            ("https://example.test/a", true),
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut api = Service::open(root.path(), 10000).unwrap();
            api.capture_enabled = true;
            api.settings.text_enabled = true;
            api.settings.links_enabled = true;
            api.set_text_clipboard_sink(Box::new(TextSink));
            let event = api
                .dispatch(if link {
                    crate::Command::IngestLink {
                        url: value.into(),
                        source: "test".into(),
                    }
                } else {
                    crate::Command::IngestText {
                        text: value.into(),
                        source: "test".into(),
                    }
                })
                .unwrap();
            let id = event["id"].as_i64().unwrap();
            let item = crate::SelectionRef {
                id,
                revision: event["revision"].as_str().unwrap().into(),
            };
            api.dispatch(crate::Command::CopySmart { items: vec![item] })
                .unwrap();
            assert_eq!(api.clipboard_sequence, 40);
            let token = if link {
                "000000000000000000000000000000a2"
            } else {
                "000000000000000000000000000000a1"
            };
            crate::text_fragments::remember_clipboard_token_for_test(token);
            let payload = owned_text_payload(Some(token.into())).unwrap();
            let result = api
                .commit_capture_snapshot(
                    CaptureSnapshot {
                        sequence: 41,
                        payload,
                    },
                    41,
                )
                .unwrap();
            assert_eq!(result["state"], "own_publication");
            assert_eq!(result["captured"], 0);
            for marker in [None, Some("000000000000000000000000000000ff".into())] {
                assert!(owned_text_payload(marker).is_none());
                let snapshot = prepare_owned_snapshot_features(
                    CaptureSnapshot {
                        sequence: 42,
                        payload: CapturePayload::Text {
                            origin: Origin::default(),
                            text: value.into(),
                        },
                    },
                    &api.root,
                    api.quota,
                    &[],
                    (false, true, true, false, false, true),
                )
                .unwrap();
                let result = api.commit_capture_snapshot(snapshot, 42).unwrap();
                assert_eq!(result["captured"], 1);
                assert_eq!(result["event"]["id"], id);
            }
            assert_eq!(
                api.db
                    .query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }
    #[test]
    fn pods_capture_is_independent_of_text_and_never_leaks_when_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let mut service = Service::open(temp.path(), 4096).unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        service.capture_enabled = true;
        service.settings.text_enabled = true;
        let make = |pods| {
            prepare_owned_snapshot_features(
                CaptureSnapshot {
                    sequence: 42,
                    payload: CapturePayload::Text {
                        origin: Origin::default(),
                        text: "🐄".into(),
                    },
                },
                &root,
                4096,
                &[],
                (true, true, false, false, pods, true),
            )
            .unwrap()
        };
        assert!(matches!(make(false).payload, CapturePayload::Unsupported));
        let prepared = make(true);
        assert_eq!(
            service.commit_capture_snapshot(prepared, 42).unwrap()["captured"],
            0
        );
        service.settings.pods_enabled = true;
        let saved = service.commit_capture_snapshot(make(true), 42).unwrap();
        assert_eq!(saved["captured"], 1);
        let again = service.commit_capture_snapshot(make(true), 42).unwrap();
        assert_eq!(saved["event"]["id"], again["event"]["id"]);
    }
    fn raw(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }
    #[test]
    fn decode_unicode_text_handles_crlf_emoji_empty_and_first_nul() {
        assert_eq!(
            decode_unicode_text(&raw("строка\r\n🐄"))
                .unwrap()
                .as_deref(),
            Some("строка\r\n🐄")
        );
        assert_eq!(decode_unicode_text(&raw("")).unwrap(), None);
        assert_eq!(
            decode_unicode_text(&raw("abc\0ignored"))
                .unwrap()
                .as_deref(),
            Some("abc")
        );
    }
    #[test]
    fn prepared_text_is_discarded_if_capture_is_paused_stale_or_tab_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let mut service = Service::open(temp.path(), 4096).unwrap();
        service.capture_enabled = true;
        service.settings.text_enabled = true;
        let make = || {
            prepare_owned_snapshot(
                CaptureSnapshot {
                    sequence: 42,
                    payload: CapturePayload::Text {
                        origin: Origin::default(),
                        text: "synthetic 🐄".into(),
                    },
                },
                temp.path(),
                4096,
                &[],
                true,
                true,
                false,
            )
            .unwrap()
        };
        let stale = make();
        assert_eq!(
            service.commit_capture_snapshot(stale, 43).unwrap()["state"],
            "retry"
        );
        let paused = make();
        service.capture_enabled = false;
        assert_eq!(
            service.commit_capture_snapshot(paused, 42).unwrap()["state"],
            "paused"
        );
        service.capture_enabled = true;
        let disabled = make();
        service.settings.text_enabled = false;
        assert_eq!(
            service.commit_capture_snapshot(disabled, 42).unwrap()["state"],
            "text_disabled"
        );
        let count: i64 = service
            .db
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn exact_urls_respect_link_toggle_without_text_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let _service = Service::open(temp.path(), 4096).unwrap();
        let make = |text: &str, text_enabled, links_enabled| {
            prepare_owned_snapshot(
                CaptureSnapshot {
                    sequence: 42,
                    payload: CapturePayload::Text {
                        origin: Origin::default(),
                        text: text.into(),
                    },
                },
                temp.path(),
                4096,
                &[],
                true,
                text_enabled,
                links_enabled,
            )
            .unwrap()
        };
        assert!(matches!(
            make("https://example.test/🐄", true, false).payload,
            CapturePayload::Unsupported
        ));
        assert!(matches!(
            make("https://example.test/🐄", false, true).payload,
            CapturePayload::PreparedLink(_)
        ));
        assert!(matches!(
            make("see https://example.test/🐄", true, false).payload,
            CapturePayload::PreparedText(_)
        ));
        assert!(matches!(
            make("see https://example.test/🐄", false, true).payload,
            CapturePayload::Unsupported
        ));
    }
    #[test]
    fn decode_unicode_text_rejects_oversize_malformed_and_unterminated() {
        assert_eq!(
            decode_unicode_text(&vec![0; MAX_TEXT_UTF16_BYTES + 1])
                .unwrap_err()
                .code,
            "text_too_large"
        );
        assert_eq!(
            decode_unicode_text(&raw(&"я".repeat(MAX_TEXT_UTF8 / 2 + 1)))
                .unwrap_err()
                .code,
            "text_too_large"
        );
        assert_eq!(
            decode_unicode_text(&[0x00, 0xd8, 0, 0]).unwrap_err().code,
            "invalid_text"
        );
        assert_eq!(
            decode_unicode_text(&[b'a', 0]).unwrap_err().code,
            "invalid_text"
        );
    }
}
#[cfg(test)]
mod table_capture_tests {
    use super::*;
    #[test]
    fn html_tables_do_not_widen_to_text_and_disabled_or_stale_receipts_never_commit() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 10000).unwrap();
        let prepared_root = api.root.clone();
        api.capture_enabled = true;
        api.settings.tables_enabled = true;
        api.settings.text_enabled = true;
        let make = |enabled| {
            prepare_owned_snapshot_features(
                CaptureSnapshot {
                    sequence: 44,
                    payload: CapturePayload::HtmlTable {
                        origin: Origin::default(),
                        html: "<table><tr><td>A</td><td>🐄</td></tr></table>".into(),
                    },
                },
                &prepared_root,
                10000,
                &[],
                (true, true, true, enabled, false, true),
            )
        };
        assert_eq!(
            api.commit_capture_snapshot(make(false).unwrap(), 44)
                .unwrap()["captured"],
            0
        );
        assert_eq!(
            api.commit_capture_snapshot(make(true).unwrap(), 45)
                .unwrap()["state"],
            "retry"
        );
        let prepared = make(true).unwrap();
        api.settings.tables_enabled = false;
        assert_eq!(
            api.commit_capture_snapshot(prepared, 44).unwrap()["state"],
            "tables_disabled"
        );
        api.settings.tables_enabled = true;
        let result = api
            .commit_capture_snapshot(make(true).unwrap(), 44)
            .unwrap();
        assert_eq!(result["event"]["kind"], "table_fragment");
        assert_eq!(result["captured"], 1);
        let invalid = prepare_owned_snapshot_features(
            CaptureSnapshot {
                sequence: 45,
                payload: CapturePayload::HtmlTable {
                    origin: Origin::default(),
                    html:
                        "<table><tr><td><table><tr><td>nested</td></tr></table></td></tr></table>"
                            .into(),
                },
            },
            &prepared_root,
            10000,
            &[],
            (true, true, true, true, false, true),
        );
        assert!(matches!(invalid,Err(e)if e.code=="unsupported_table"));
        let ordinary = prepare_owned_snapshot_features(
            CaptureSnapshot {
                sequence: 45,
                payload: CapturePayload::Text {
                    origin: Origin::default(),
                    text: "plain\ttabs".into(),
                },
            },
            &prepared_root,
            10000,
            &[],
            (true, true, true, true, false, true),
        )
        .unwrap();
        assert_eq!(
            api.commit_capture_snapshot(ordinary, 45).unwrap()["event"]["kind"],
            "text_fragment"
        );
    }
    struct OwnerSink {
        token: Option<String>,
        html: bool,
        text: bool,
    }
    impl crate::table_clipboard::Sink for OwnerSink {
        fn write(&self, _: &str, _: &[u8]) -> Result<crate::table_clipboard::Receipt> {
            Ok(crate::table_clipboard::Receipt {
                sequence: 100,
                owner_token: self.token.clone(),
                html_written: self.html,
                text_written: self.text,
                warnings: vec![],
            })
        }
    }
    #[test]
    fn table_owner_marker_suppresses_later_sequences_without_deduplicating_external_equal_content()
    {
        for (html, text) in [(true, true), (false, true), (true, false), (false, false)] {
            let root = tempfile::tempdir().unwrap();
            let mut api = Service::open(root.path(), 10000).unwrap();
            api.capture_enabled = true;
            api.settings.tables_enabled = true;
            let token = "00000000000000000000000000000064".to_string();
            api.set_table_clipboard_sink(Box::new(OwnerSink {
                token: Some(token.clone()),
                html,
                text,
            }));
            let event = api
                .ingest_table(
                    crate::table_fragments::Table {
                        rows: vec![vec!["same".into(), "content".into()]],
                    },
                    "synthetic",
                )
                .unwrap();
            api.copy_tables(vec![crate::SelectionRef {
                id: event["id"].as_i64().unwrap(),
                revision: event["revision"].as_str().unwrap().into(),
            }])
            .unwrap();
            for sequence in [101, 102, 109] {
                let known = api.owned_table_tokens.iter().cloned().collect::<Vec<_>>();
                let payload =
                    owned_table_payload(Some(token.clone()), &known).expect("owned proof");
                let result = api
                    .commit_capture_snapshot(CaptureSnapshot { sequence, payload }, sequence)
                    .unwrap();
                assert_eq!(result["state"], "own_publication");
                assert_eq!(result["captured"], 0);
                assert_eq!(api.clipboard_sequence, sequence);
            }
            assert_eq!(
                crate::storage::query_events(&api.db, "", 100, None)
                    .unwrap()
                    .len(),
                1
            );
            let result = api
                .commit_capture_snapshot(
                    CaptureSnapshot {
                        sequence: 110,
                        payload: CapturePayload::OwnTable {
                            token: token.clone(),
                        },
                    },
                    111,
                )
                .unwrap();
            assert_eq!(result["state"], "retry");
            assert_eq!(api.clipboard_sequence, 109);
            for (offset, marker) in [None, Some("000000000000000000000000000000ff".into())]
                .into_iter()
                .enumerate()
            {
                let known = api.owned_table_tokens.iter().cloned().collect::<Vec<_>>();
                assert!(owned_table_payload(marker, &known).is_none());
                let sequence = 112 + offset as u32;
                let snapshot = prepare_owned_snapshot_features(
                    CaptureSnapshot {
                        sequence,
                        payload: CapturePayload::HtmlTable {
                            origin: Origin::default(),
                            html: "<table><tr><td>same</td><td>content</td></tr></table>".into(),
                        },
                    },
                    &api.root,
                    10000,
                    &[],
                    (true, true, true, true, false, true),
                )
                .unwrap();
                let result = api.commit_capture_snapshot(snapshot, sequence).unwrap();
                assert_eq!(result["captured"], 1);
                assert_ne!(result["event"]["id"], event["id"]);
            }
            assert_eq!(
                crate::storage::query_events(&api.db, "", 100, None)
                    .unwrap()
                    .len(),
                3
            );
            // A private worker receipt is rechecked at commit if its token was
            // evicted while preparation was in flight. Do not swallow this sequence.
            api.owned_table_tokens.clear();
            let previous = api.clipboard_sequence;
            let result = api
                .commit_capture_snapshot(
                    CaptureSnapshot {
                        sequence: 120,
                        payload: CapturePayload::OwnTable {
                            token: token.clone(),
                        },
                    },
                    120,
                )
                .unwrap();
            assert_eq!(result["state"], "retry");
            assert_eq!(api.clipboard_sequence, previous);
        }
    }
}

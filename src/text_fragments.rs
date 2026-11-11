//! Exact, bounded text fragments backed by the same managed blob lifecycle as images.
use crate::{ApiError, Event, Result, SelectionRef, Service, origin::Origin, storage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_MERGED_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SEPARATOR_BYTES: usize = 4096;
pub const MAX_ITEMS: usize = 1000;
static NEXT_NAME: AtomicU64 = AtomicU64::new(0);

/// Explicit user-requested publication; tests can provide a synthetic sink.
pub trait ClipboardSink: Send {
    fn write(&self, text: &str) -> Result<u32>;
}
pub struct NativeClipboard;
#[path = "text_clipboard.rs"]
mod text_clipboard;
impl ClipboardSink for NativeClipboard {
    fn write(&self, text: &str) -> Result<u32> {
        text_clipboard::write(text)
    }
}

/// Presentation title only: the original content is never trimmed or changed.
pub fn title(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .chars()
        .take(64)
        .collect()
}

pub(crate) fn prepare(
    root: &Path,
    quota: u64,
    text: &str,
    source: &str,
    origin: &Origin,
    references: &[PathBuf],
) -> Result<storage::PreparedBlob> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(ApiError::new(
            "invalid_text",
            "Text must contain 1 byte to 1 MiB",
        ));
    }
    let name = format!(
        "Text-{}-{}.txt",
        crate::now_ms(),
        NEXT_NAME.fetch_add(1, Ordering::Relaxed)
    );
    let mut receipt = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: &name,
            source,
            bytes: text.as_bytes(),
            origin,
        },
        references,
    )?;
    receipt.mark_text(text.to_owned());
    Ok(receipt)
}

pub(crate) fn read_text_event(event: &Event) -> Result<String> {
    if event.kind != crate::EventKind::TextFragment {
        return Err(ApiError::new("not_text", "Item is not a text fragment"));
    }
    crate::open_file::validate(event)?;
    let mut bytes = Vec::new();
    std::fs::File::open(&event.path)?
        .take((MAX_TEXT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(ApiError::new("invalid_text", "Stored text exceeds 1 MiB"));
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if event.revision != format!("blob:{hash}") {
        return Err(ApiError::new(
            "source_changed",
            "Source contents changed since this arrival",
        ));
    }
    String::from_utf8(bytes).map_err(|_| ApiError::new("invalid_text", "Stored text is not UTF-8"))
}

pub(crate) fn merge_snapshot(
    items: Vec<(SelectionRef, Result<Event>)>,
    separator: &str,
) -> Result<Value> {
    merge_snapshot_kind(items, separator, false)
}
pub(crate) fn merge_snapshot_kind(
    items: Vec<(SelectionRef, Result<Event>)>,
    separator: &str,
    links: bool,
) -> Result<Value> {
    if items.is_empty() || items.len() > MAX_ITEMS {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected 1..1000 text items",
        ));
    }
    if separator.len() > MAX_SEPARATOR_BYTES {
        return Err(ApiError::new(
            "invalid_argument",
            "Separator exceeds 4096 bytes",
        ));
    }
    let mut seen: HashMap<i64, String> = HashMap::new();
    let mut results = Vec::new();
    let mut parts = Vec::new();
    let mut total = 0usize;
    let mut failed = false;
    for (item, event_result) in items {
        if let Some(previous) = seen.get(&item.id) {
            if *previous != item.revision {
                failed = true;
                results.push(json!({"id":item.id,"ok":false,"error":{"code":"source_changed","message":"Conflicting revisions for the same item"}}));
            }
            continue;
        }
        seen.insert(item.id, item.revision.clone());
        let result = event_result.and_then(|event| {
            if event.revision != item.revision {
                return Err(ApiError::new("source_changed", "Item revision changed"));
            }
            if links {
                crate::link_fragments::read_event(&event)
            } else {
                read_text_event(&event)
            }
        });
        match result {
            Ok(text) => {
                total = total
                    .saturating_add(text.len())
                    .saturating_add(if parts.is_empty() { 0 } else { separator.len() });
                if total > MAX_MERGED_BYTES {
                    failed = true;
                    results.push(json!({"id":item.id,"ok":false,"error":{"code":"text_too_large","message":"Merged text exceeds 8 MiB"}}));
                } else {
                    results.push(json!({"id":item.id,"ok":true}));
                    parts.push(text);
                }
            }
            Err(error) => {
                failed = true;
                results.push(json!({"id":item.id,"ok":false,"error":error}));
            }
        }
    }
    if failed {
        Ok(json!({"merged":false,"items":results}))
    } else {
        Ok(json!({"merged":true,"text":parts.join(separator),"items":results}))
    }
}

impl Service {
    pub(crate) fn get_text(&self, id: i64) -> Result<Value> {
        let event = self.get(id)?;
        let text = read_text_event(&event)?;
        Ok(json!({"id":id,"revision":event.revision,"title":title(&text),"text":text}))
    }
    pub(crate) fn merge_text(&self, items: &[SelectionRef], separator: &str) -> Result<Value> {
        if items.is_empty() || items.len() > MAX_ITEMS || separator.len() > MAX_SEPARATOR_BYTES {
            return Err(ApiError::new(
                "invalid_argument",
                "Invalid item count or separator length",
            ));
        }
        let snapshot = items
            .iter()
            .cloned()
            .map(|item| {
                let event = self.get(item.id);
                (item, event)
            })
            .collect();
        merge_snapshot(snapshot, separator)
    }
}

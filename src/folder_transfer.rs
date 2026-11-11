//! Materialize a selection into a folder. Copy only: this module never removes sources.
use crate::{ApiError, Event, EventKind, Result, open_file};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Serialize)]
pub struct TransferReceipt {
    pub items: Vec<TransferItem>,
}
#[derive(Debug, Serialize)]
pub struct TransferItem {
    pub id: i64,
    pub path: Option<PathBuf>,
    pub content_hash: Option<String>,
    pub status: &'static str,
    pub error: Option<ApiError>,
}

/// Destination is explicit and must exist. All item failures, including partially
/// written files, are returned with their path; no source or output is deleted.
pub fn materialize(events: &[Event], destination: &Path) -> Result<TransferReceipt> {
    if events.is_empty() || events.len() > 1000 {
        return Err(ApiError::new("invalid_argument", "Expected 1..1000 items"));
    }
    let root = destination.canonicalize()?;
    if !root.is_dir() {
        return Err(ApiError::new(
            "invalid_argument",
            "Destination must be an existing folder",
        ));
    }
    let mut items: Vec<Option<TransferItem>> = (0..events.len()).map(|_| None).collect();
    let mut links = Vec::new();
    let mut link_bytes = 0usize;
    for (index, event) in events.iter().enumerate() {
        if event.kind == EventKind::LinkFragment {
            match crate::link_fragments::read_event(event) {
                Ok(url) => {
                    // HTML escaping can expand every input byte; bound both the
                    // retained URLs and the generated document conservatively.
                    let estimate = url
                        .len()
                        .saturating_add(event.name.len())
                        .saturating_mul(6)
                        .saturating_add(64);
                    if link_bytes.saturating_add(estimate) > crate::text_fragments::MAX_MERGED_BYTES
                    {
                        items[index] = Some(failure(
                            event.id,
                            None,
                            ApiError::new("export_too_large", "Bookmarks exceed export limit"),
                        ));
                    } else {
                        link_bytes += estimate;
                        links.push((index, event, url));
                    }
                }
                Err(e) => items[index] = Some(failure(event.id, None, e)),
            }
            continue;
        }
        let mut created = None;
        let result = materialize_one(event, &root, &mut created);
        items[index] = Some(match result {
            Ok((path, hash)) => success(event.id, path, hash),
            Err(e) => failure(event.id, created, e),
        });
    }
    if !links.is_empty() {
        let mut html = String::from(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n",
        );
        for (_, event, url) in &links {
            let title = if event.name.is_empty() {
                url.as_str()
            } else {
                event.name.as_str()
            };
            html.push_str(&format!(
                "    <DT><A HREF=\"{}\">{}</A>\n",
                escape(url),
                escape(title)
            ));
        }
        html.push_str("</DL><p>\n");
        let mut created = None;
        let result: Result<PathBuf> = (|| {
            let (path, file) = create_unique(&root, "bookmarks.html")?;
            created = Some(path.clone());
            write_bytes(file, &path, html.as_bytes(), owned_time(links[0].1)?)?;
            for (_, event, _) in &links {
                open_file::validate(event)?;
            }
            Ok(path)
        })();
        match result {
            Ok(path) => {
                let hash = format!("{:x}", Sha256::digest(html.as_bytes()));
                for (i, event, _) in links {
                    items[i] = Some(success(event.id, path.clone(), hash.clone()));
                }
            }
            Err(e) => {
                for (i, event, _) in links {
                    items[i] = Some(failure(
                        event.id,
                        created.clone(),
                        ApiError::new(&e.code, &e.message),
                    ));
                }
            }
        }
    }
    Ok(TransferReceipt {
        items: items
            .into_iter()
            .map(|i| i.expect("every input has a receipt"))
            .collect(),
    })
}
fn success(id: i64, path: PathBuf, hash: String) -> TransferItem {
    TransferItem {
        id,
        path: Some(path),
        content_hash: Some(hash),
        status: "copied",
        error: None,
    }
}
fn failure(id: i64, path: Option<PathBuf>, error: ApiError) -> TransferItem {
    TransferItem {
        id,
        path,
        content_hash: None,
        status: "failed",
        error: Some(error),
    }
}
fn owned_time(event: &Event) -> Result<SystemTime> {
    let ms = u64::try_from(event.detected_at_ms)
        .map_err(|_| ApiError::new("invalid_timestamp", "Negative arrival timestamp"))?;
    UNIX_EPOCH
        .checked_add(Duration::from_millis(ms))
        .ok_or_else(|| ApiError::new("invalid_timestamp", "Arrival timestamp out of range"))
}
fn materialize_one(
    event: &Event,
    root: &Path,
    created: &mut Option<PathBuf>,
) -> Result<(PathBuf, String)> {
    open_file::validate(event)?;
    let bytes = match event.kind {
        EventKind::TextFragment => {
            Some(crate::text_fragments::read_text_event(event)?.into_bytes())
        }
        EventKind::TableFragment => Some(
            crate::table_fragments::to_csv_bom(&crate::table_service::read(event)?)
                .map_err(|e| ApiError::new("invalid_table", e))?,
        ),
        _ => None,
    };
    let name = match event.kind {
        EventKind::TableFragment => extension(&event.name, "csv"),
        EventKind::TextFragment => {
            if event.name.to_ascii_lowercase().ends_with(".md") {
                extension(&event.name, "md")
            } else {
                extension(&event.name, "txt")
            }
        }
        _ => event.name.clone(),
    };
    let source = if bytes.is_none() {
        Some(File::open(&event.path)?)
    } else {
        None
    };
    let time = if event.storage == "cached" {
        owned_time(event)?
    } else {
        source
            .as_ref()
            .ok_or_else(|| ApiError::new("invalid_argument", "External fragment is not supported"))?
            .metadata()?
            .modified()?
    };
    let (path, mut output) = create_unique(root, &name)?;
    *created = Some(path.clone());
    let hash = if let Some(bytes) = bytes {
        let hash = format!("{:x}", Sha256::digest(&bytes));
        write_bytes(output, &path, &bytes, time)?;
        hash
    } else {
        let mut source = source.expect("file source");
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        output.set_times(fs::FileTimes::new().set_modified(time))?;
        output.sync_all()?;
        drop(output);
        let copied_hash = format!("{:x}", hash.finalize());
        if let Some(expected) = event
            .revision
            .strip_prefix("ref:")
            .or_else(|| event.revision.strip_prefix("blob:"))
            && copied_hash != expected
        {
            return Err(ApiError::new(
                "source_changed",
                "Copied bytes differ from the recorded source revision",
            ));
        }
        if crate::preview::content_hash(&path)? != copied_hash {
            return Err(ApiError::new(
                "verification_failed",
                "Output content differs from copied bytes",
            ));
        }
        copied_hash
    };
    open_file::validate(event)?;
    Ok((path, hash))
}
fn write_bytes(mut output: File, path: &Path, bytes: &[u8], time: SystemTime) -> Result<()> {
    output.write_all(bytes)?;
    output.set_times(fs::FileTimes::new().set_modified(time))?;
    output.sync_all()?;
    drop(output);
    if crate::preview::content_hash(path)? != format!("{:x}", Sha256::digest(bytes)) {
        return Err(ApiError::new(
            "verification_failed",
            "Output content differs from expected bytes",
        ));
    }
    Ok(())
}
fn extension(name: &str, ext: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("item");
    format!("{stem}.{ext}")
}
fn create_unique(root: &Path, name: &str) -> Result<(PathBuf, File)> {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_end_matches([' ', '.']);
    let clean = if clean.is_empty() || clean == "." || clean == ".." {
        "item"
    } else {
        clean
    };
    let path = Path::new(clean);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("item");
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    for index in 0..10_000 {
        let name = if index == 0 {
            clean.to_owned()
        } else {
            format!("{stem} ({index}){extension}")
        };
        let path = root.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(ApiError::new("name_conflict", "No available numbered name"))
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(id: i64, path: &Path) -> Event {
        Event {
            kind: EventKind::FileReference,
            text_preview: None,
            delete_policy: Default::default(),
            revision: format!("ref:{}", crate::preview::content_hash(path).unwrap()),
            origin: Default::default(),
            id,
            detected_at_ms: 1,
            modified_at_ms: None,
            source: "test".into(),
            name: "a.txt".into(),
            storage: "reference".into(),
            path: path.into(),
            size: 3,
            available: true,
        }
    }
    #[test]
    fn same_folder_uses_numbered_name_and_stale_item_does_not_block_others() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.txt");
        fs::write(&source, b"one").unwrap();
        let first = event(1, &source);
        let mut stale = event(2, &source);
        stale.revision = "ref:bad".into();
        let result = materialize(&[first, stale], dir.path()).unwrap();
        assert_eq!(result.items[0].status, "copied");
        assert_eq!(result.items[1].status, "failed");
        assert_eq!(fs::read(&source).unwrap(), b"one");
        assert_eq!(
            fs::read(result.items[0].path.as_ref().unwrap()).unwrap(),
            b"one"
        );
        assert_ne!(result.items[0].path.as_ref().unwrap(), &source);
    }
    #[test]
    fn escaping_preserves_unicode_and_quotes() {
        assert_eq!(escape("🐄&<\"'"), "🐄&amp;&lt;&quot;&#39;");
    }

    #[test]
    fn grouped_links_and_text_are_independent_materializations() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out");
        fs::create_dir(&output).unwrap();
        let mut events = Vec::new();
        for (id, contents, kind, name) in [
            (
                1,
                "https://example.com/?a=1&b=2",
                EventKind::LinkFragment,
                "Птица & море",
            ),
            (2, "hello 🐄", EventKind::TextFragment, "note.txt"),
            (
                3,
                "https://example.org/🐄",
                EventKind::LinkFragment,
                "вторая",
            ),
        ] {
            let path = dir.path().join(format!("source-{id}"));
            fs::write(&path, contents).unwrap();
            let mut item = event(id, &path);
            item.kind = kind;
            item.name = name.into();
            item.storage = "cached".into();
            item.revision = format!("blob:{}", crate::preview::content_hash(&path).unwrap());
            item.detected_at_ms = 1_700_000_000_000;
            events.push(item);
        }
        let receipt = materialize(&events, &output).unwrap();
        assert!(receipt.items.iter().all(|item| item.status == "copied"));
        assert_eq!(receipt.items[0].path, receipt.items[2].path);
        let html = fs::read_to_string(receipt.items[0].path.as_ref().unwrap()).unwrap();
        assert!(html.contains("Птица &amp; море"));
        assert!(html.find("example.com").unwrap() < html.find("example.org").unwrap());
        assert_eq!(
            fs::read_to_string(receipt.items[1].path.as_ref().unwrap()).unwrap(),
            "hello 🐄"
        );
        let modified = fs::metadata(receipt.items[1].path.as_ref().unwrap())
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            modified.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            1_700_000_000
        );
    }
}

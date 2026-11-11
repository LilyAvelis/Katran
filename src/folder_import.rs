//! Explicit Folder import; source files are never modified or removed.
use crate::{
    ApiError, Command, Event, Result, Service, file_types, link_fragments, open_file,
    table_fragments,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

const MAX_BOOKMARKS: usize = 1000;

fn read_snapshot(event: &Event, limit: usize) -> Result<Vec<u8>> {
    open_file::validate(event)?;
    let mut bytes = Vec::new();
    File::open(&event.path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(ApiError::new(
            "import_too_large",
            format!("Import exceeds {limit} bytes"),
        ));
    }
    open_file::validate(event)?;
    Ok(bytes)
}

fn bookmarks(text: &str) -> Result<Vec<Option<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).trim_start();
    let marker = "<!DOCTYPE NETSCAPE-Bookmark-file-1>";
    if !text
        .get(..marker.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(marker))
    {
        return Err(ApiError::new(
            "unsupported_import",
            "HTML is not a Netscape bookmarks export",
        ));
    }
    let document = scraper::Html::parse_document(text);
    let selector = scraper::Selector::parse("a")
        .map_err(|_| ApiError::new("invalid_argument", "Invalid bookmark selector"))?;
    let links: Vec<_> = document
        .select(&selector)
        .take(MAX_BOOKMARKS + 1)
        .map(|a| a.value().attr("href").map(str::to_owned))
        .collect();
    if links.is_empty() {
        return Err(ApiError::new(
            "invalid_link",
            "Bookmark export contains no links",
        ));
    }
    if links.len() > MAX_BOOKMARKS {
        return Err(ApiError::new(
            "import_too_large",
            "Bookmark export exceeds 1000 links",
        ));
    }
    Ok(links)
}

impl Service {
    pub(crate) fn import_folder_event(&mut self, event: Event) -> Result<Value> {
        if event.id >= 0 || event.source != "work_folder" {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected a current Folder item",
            ));
        }
        let path = Path::new(&event.name);
        if file_types::is_native_image(path) {
            let bytes = read_snapshot(&event, self.quota.min(256 * 1024 * 1024) as usize)?;
            let source_hash = format!("{:x}", Sha256::digest(&bytes));
            let mut imported = self.dispatch(Command::IngestBytes {
                name: event.name.clone(),
                bytes,
                source: "manual".into(),
            })?;
            // Arrival is new, but a copy/move must preserve the source's modified date.
            if let Some(modified) = event.modified_at_ms {
                let id = imported["id"]
                    .as_i64()
                    .ok_or_else(|| ApiError::new("invalid_import", "Missing imported image ID"))?;
                self.db.execute(
                    "UPDATE events SET modified_at_ms=?1 WHERE id=?2",
                    rusqlite::params![modified, id],
                )?;
                imported["modified_at_ms"] = json!(modified);
            }
            return Ok(
                json!({"kind":"image","event":imported,"imported":1,"failed":0,"source_hash":source_hash}),
            );
        }
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match extension.as_str() {
            "txt" | "md" | "markdown" => {
                self.check_text_enabled()?;
                let bytes = read_snapshot(&event, crate::text_fragments::MAX_TEXT_BYTES)?;
                let source_hash = format!("{:x}", Sha256::digest(&bytes));
                let text = String::from_utf8(bytes)
                    .map_err(|_| ApiError::new("invalid_text", "Import must be UTF-8"))?;
                let imported = self.dispatch(Command::IngestText {
                    text,
                    source: "manual".into(),
                })?;
                Ok(
                    json!({"kind":"text","event":imported,"imported":1,"failed":0,"source_hash":source_hash}),
                )
            }
            "csv" => {
                self.check_tables_enabled()?;
                let bytes = read_snapshot(&event, table_fragments::MAX_INPUT_BYTES)?;
                let source_hash = format!("{:x}", Sha256::digest(&bytes));
                let text = String::from_utf8(bytes)
                    .map_err(|_| ApiError::new("invalid_text", "Import must be UTF-8"))?;
                let table =
                    table_fragments::parse_csv(&text).map_err(crate::table_service::invalid)?;
                let imported = self.dispatch(Command::IngestTable {
                    rows: table.rows,
                    source: "manual".into(),
                })?;
                Ok(
                    json!({"kind":"table","event":imported,"imported":1,"failed":0,"source_hash":source_hash}),
                )
            }
            "html" | "htm" => {
                let bytes = read_snapshot(&event, crate::text_fragments::MAX_MERGED_BYTES)?;
                let source_hash = format!("{:x}", Sha256::digest(&bytes));
                let text = String::from_utf8(bytes)
                    .map_err(|_| ApiError::new("invalid_text", "Import must be UTF-8"))?;
                let links = bookmarks(&text)?;
                self.check_links_enabled()?;
                let mut items = Vec::with_capacity(links.len());
                let mut imported = 0;
                for value in links {
                    let result = match value.as_deref() {
                        Some(url) if link_fragments::recognize(url).is_some() => {
                            self.dispatch(Command::IngestLink {
                                url: url.into(),
                                source: "manual".into(),
                            })
                        }
                        _ => Err(ApiError::new(
                            "invalid_link",
                            "Bookmark must contain an absolute HTTP(S) URL",
                        )),
                    };
                    match result {
                        Ok(event) => {
                            imported += 1;
                            items.push(json!({"url":value,"ok":true,"event":event}));
                        }
                        Err(error) => items.push(json!({"url":value,"ok":false,"error":error})),
                    }
                }
                let failed = items.len() - imported;
                Ok(
                    json!({"kind":"links","items":items,"imported":imported,"failed":failed,"source_hash":source_hash}),
                )
            }
            _ => Err(ApiError::new(
                "unsupported_import",
                "This file type has no visible Katran category",
            )),
        }
    }
}

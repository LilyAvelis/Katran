//! Table ingest/read/copy through the same owned blob and dispatcher boundaries.
use crate::{table_fragments::Table, *};
use serde_json::{Value, json};
use std::io::Read;
pub(crate) fn invalid(message: impl std::fmt::Display) -> ApiError {
    ApiError::new("invalid_table", message)
}
pub(crate) fn prepare(
    root: &Path,
    quota: u64,
    table: &Table,
    source: &str,
    origin: &origin::Origin,
    references: &[PathBuf],
) -> Result<storage::PreparedBlob> {
    table.validate().map_err(invalid)?;
    validate_source(source)?;
    let bytes = table_fragments::to_csv_bom(table).map_err(invalid)?;
    let tsv = table_fragments::to_tsv(table).map_err(invalid)?;
    let mut prepared = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: &format!("Table-{}.csv", now_ms()),
            source,
            bytes: &bytes,
            origin,
        },
        references,
    )?;
    prepared.mark_table(tsv);
    Ok(prepared)
}
pub(crate) fn read(event: &Event) -> Result<Table> {
    if event.kind != EventKind::TableFragment {
        return Err(ApiError::new("not_table", "Selected object is not a table"));
    }
    open_file::validate(event)?;
    let mut bytes = Vec::new();
    fs::File::open(&event.path)?
        .take((table_fragments::MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > table_fragments::MAX_INPUT_BYTES {
        return Err(invalid("Stored table exceeds read limit"));
    }
    use sha2::{Digest, Sha256};
    if event.revision != format!("blob:{:x}", Sha256::digest(&bytes)) {
        return Err(ApiError::new(
            "source_changed",
            "Table content changed while reading",
        ));
    }
    let text = String::from_utf8(bytes).map_err(invalid)?;
    let table = table_fragments::parse_csv(&text).map_err(invalid)?;
    open_file::validate(event)?;
    Ok(table)
}
pub(crate) struct PreparedCopy {
    pub(crate) result: Value,
    tsv: Option<String>,
    html: Vec<u8>,
}
pub(crate) fn prepare_copy(snapshot: Vec<(SelectionRef, Result<Event>)>) -> Result<PreparedCopy> {
    if snapshot.is_empty() || snapshot.len() > 1000 {
        return Err(ApiError::new("invalid_argument", "Expected 1..1000 tables"));
    }
    let mut seen = std::collections::HashMap::new();
    let mut tables = Vec::new();
    let mut items = Vec::new();
    let mut width = 0;
    let mut row_count = 0usize;
    let mut content = 0usize;
    for (item, event) in snapshot {
        if let Some(previous) = seen.get(&item.id) {
            if previous != &item.revision {
                items.push(json!({"id":item.id,"ok":false,"error":{"code":"stale_selection","message":"Duplicate table has conflicting revision"}}));
            }
            continue;
        }
        seen.insert(item.id, item.revision.clone());
        let result = event.and_then(|event| {
            if event.revision != item.revision {
                return Err(ApiError::new("stale_selection", "Selected table changed"));
            }
            read(&event)
        });
        match result {
            Ok(table) => {
                width = width.max(table.rows.first().map_or(0, Vec::len));
                row_count = row_count
                    .checked_add(table.rows.len() + usize::from(!tables.is_empty()))
                    .ok_or_else(|| invalid("Combined table row count overflow"))?;
                content = content
                    .checked_add(table.rows.iter().flatten().map(String::len).sum::<usize>())
                    .ok_or_else(|| invalid("Combined table content overflow"))?;
                if row_count > table_fragments::MAX_ROWS
                    || row_count
                        .checked_mul(width)
                        .is_none_or(|cells| cells > table_fragments::MAX_CELLS)
                    || content > table_fragments::MAX_CONTENT_BYTES
                {
                    return Err(invalid("Combined table exceeds cell/content/row limits"));
                }
                tables.push(table);
                items.push(json!({"id":item.id,"ok":true}));
            }
            Err(error) => items.push(json!({"id":item.id,"ok":false,"error":error})),
        }
    }
    if items.iter().any(|item| item["ok"] == false) {
        return Ok(PreparedCopy {
            result: json!({"merged":false,"table_copied":false,"items":items,"atomic":true}),
            tsv: None,
            html: vec![],
        });
    }
    let count = tables.len();
    let mut rows = Vec::new();
    for table in tables {
        if !rows.is_empty() {
            rows.push(vec![String::new(); width]);
        }
        for mut row in table.rows {
            row.resize(width, String::new());
            rows.push(row);
        }
    }
    let table = Table { rows };
    table.validate().map_err(invalid)?;
    let tsv = table_fragments::to_tsv(&table).map_err(invalid)?;
    let html = table_fragments::encode_cf_html(&table_fragments::to_html(&table).map_err(invalid)?)
        .map_err(invalid)?;
    Ok(PreparedCopy {
        result: json!({"merged":true,"table_copied":false,"items":items,"table_count":count,"row_count":table.rows.len(),"column_count":width,"atomic":true}),
        tsv: Some(tsv),
        html,
    })
}
impl Service {
    pub(crate) fn check_tables_enabled(&self) -> Result<()> {
        if !self.settings.tab_enabled(interaction::ShelfTab::Tables) {
            Err(ApiError::new(
                "tab_disabled",
                "Tables is disabled; enable its eye in Settings → Tabs",
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn ingest_table(&mut self, table: Table, source: &str) -> Result<Value> {
        self.check_tables_enabled()?;
        let prepared = prepare(
            &self.root,
            self.quota,
            &table,
            source,
            &self.ingest_origin,
            &self.reference_paths()?,
        )?;
        let id = storage::commit_prepared_bytes(&mut self.db, &self.root, self.quota, prepared)?;
        Ok(json!(self.get(id)?))
    }
    pub(crate) fn get_table(&self, id: i64) -> Result<Value> {
        let event = self.get(id)?;
        let table = read(&event)?;
        let csv = table_fragments::to_csv_bom(&table).map_err(invalid)?;
        let csv_text = String::from_utf8(csv).map_err(invalid)?;
        Ok(
            json!({"id":id,"revision":event.revision,"title":event.name,"table":table,"csv_text":csv_text}),
        )
    }
    pub(crate) fn selection_is_tables(&self, items: &[SelectionRef]) -> Result<bool> {
        if items.is_empty() || items.len() > 1000 {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 selected objects",
            ));
        }
        for item in items {
            let event = self.get(item.id)?;
            if event.revision != item.revision {
                return Err(ApiError::new("stale_selection", "Selected content changed"));
            }
            if event.kind != EventKind::TableFragment {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn copy_tables(&mut self, items: Vec<SelectionRef>) -> Result<Value> {
        let snapshot = items
            .into_iter()
            .map(|item| {
                let event = self.get(item.id);
                (item, event)
            })
            .collect();
        let prepared = prepare_copy(snapshot)?;
        self.publish_table_copy(prepared)
    }
    pub(crate) fn publish_table_copy(&mut self, mut prepared: PreparedCopy) -> Result<Value> {
        if let Some(tsv) = prepared.tsv {
            let receipt = self
                .table_clipboard
                .lock()
                .map_err(|_| ApiError::new("clipboard_error", "Table clipboard lock poisoned"))?
                .write(&tsv, &prepared.html)?;
            // EmptyClipboard has already happened even when neither format was
            // published. Fence our own change on every post-empty receipt.
            if let Some(token) = receipt.owner_token {
                if !self.owned_table_tokens.contains(&token) {
                    self.owned_table_tokens.push_back(token);
                }
                while self.owned_table_tokens.len() > 64 {
                    self.owned_table_tokens.pop_front();
                }
            }
            if receipt.html_written && receipt.text_written {
                let ids = prepared.result["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item["id"].as_i64())
                    .collect::<Vec<_>>();
                self.record_selection_publication(receipt.sequence, &ids);
            } else {
                self.record_clipboard_publication(receipt.sequence);
            }
            prepared.result["clipboard_sequence"] = json!(receipt.sequence);
            self.capture_events.cancel();
            prepared.result["table_copied"] = json!(receipt.html_written && receipt.text_written);
            prepared.result["atomic"] = json!(receipt.html_written && receipt.text_written);
            prepared.result["partial"] = json!(!(receipt.html_written && receipt.text_written));
            prepared.result["clipboard_changed"] = json!(true);
            prepared.result["html_written"] = json!(receipt.html_written);
            prepared.result["text_written"] = json!(receipt.text_written);
            prepared.result["warnings"] = json!(receipt.warnings);
        }
        Ok(prepared.result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Partial;
    impl table_clipboard::Sink for Partial {
        fn write(&self, _: &str, _: &[u8]) -> Result<table_clipboard::Receipt> {
            Ok(table_clipboard::Receipt {
                sequence: 77,
                owner_token: Some("0000000000000000000000000000004d".into()),
                html_written: false,
                text_written: false,
                warnings: vec!["synthetic post-empty failure".into()],
            })
        }
    }
    #[test]
    fn partial_after_empty_updates_sequence_and_supersedes_inflight_capture() {
        let root = tempfile::tempdir().unwrap();
        let mut service = Service::open(root.path(), 10000).unwrap();
        service.settings.tables_enabled = true;
        service.capture_enabled = true;
        service.set_table_clipboard_sink(Box::new(Partial));
        let event = service
            .ingest_table(
                Table {
                    rows: vec![vec!["🐄".into()]],
                },
                "synthetic",
            )
            .unwrap();
        let ticket = match service
            .begin_capture_event(capture_events::Trigger::Changed)
            .unwrap()
        {
            capture_events::CaptureStart::Prepare { ticket, .. } => ticket,
            _ => panic!("expected preparation"),
        };
        let result = service
            .copy_tables(vec![SelectionRef {
                id: event["id"].as_i64().unwrap(),
                revision: event["revision"].as_str().unwrap().into(),
            }])
            .unwrap();
        assert_eq!(result["table_copied"], false);
        assert_eq!(service.clipboard_sequence, 77);
        assert!(
            service
                .owned_table_tokens
                .contains(&"0000000000000000000000000000004d".into())
        );
        let result = service
            .complete_capture(
                ticket,
                Ok(capture::CaptureSnapshot {
                    sequence: 77,
                    payload: capture::CapturePayload::Unsupported,
                }),
            )
            .unwrap();
        assert_eq!(result["state"], "superseded");
    }
    struct TokenSink(std::sync::atomic::AtomicU64);
    impl table_clipboard::Sink for TokenSink {
        fn write(&self, _: &str, _: &[u8]) -> Result<table_clipboard::Receipt> {
            let n = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(table_clipboard::Receipt {
                sequence: n as u32,
                owner_token: Some(format!("{n:032x}")),
                html_written: true,
                text_written: false,
                warnings: vec![],
            })
        }
    }
    struct NoMarker;
    impl table_clipboard::Sink for NoMarker {
        fn write(&self, _: &str, _: &[u8]) -> Result<table_clipboard::Receipt> {
            Ok(table_clipboard::Receipt {
                sequence: 88,
                owner_token: None,
                html_written: true,
                text_written: true,
                warnings: vec!["synthetic marker failure".into()],
            })
        }
    }
    #[test]
    fn owned_tokens_are_bounded_and_only_successfully_written_markers_are_remembered() {
        let root = tempfile::tempdir().unwrap();
        let mut service = Service::open(root.path(), 10000).unwrap();
        service.settings.tables_enabled = true;
        let event = service
            .ingest_table(
                Table {
                    rows: vec![vec!["A".into()]],
                },
                "synthetic",
            )
            .unwrap();
        service.set_table_clipboard_sink(Box::new(TokenSink(std::sync::atomic::AtomicU64::new(1))));
        for _ in 0..65 {
            service
                .copy_tables(vec![SelectionRef {
                    id: event["id"].as_i64().unwrap(),
                    revision: event["revision"].as_str().unwrap().into(),
                }])
                .unwrap();
        }
        assert_eq!(service.owned_table_tokens.len(), 64);
        assert!(!service.owned_table_tokens.contains(&format!("{:032x}", 1)));
        assert!(service.owned_table_tokens.contains(&format!("{:032x}", 65)));
        let previous = service.owned_table_tokens.clone();
        service.set_table_clipboard_sink(Box::new(NoMarker));
        service
            .copy_tables(vec![SelectionRef {
                id: event["id"].as_i64().unwrap(),
                revision: event["revision"].as_str().unwrap().into(),
            }])
            .unwrap();
        assert_eq!(service.owned_table_tokens, previous);
        assert_eq!(service.clipboard_sequence, 88);
    }
}

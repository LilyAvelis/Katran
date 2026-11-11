use crate::{ApiError, Result, SelectionRef, Service};
use serde_json::{Value, json};
impl Service {
    pub(crate) fn get_document_text(&self, id: i64) -> Result<Value> {
        use std::io::Read;
        let event = self.get(id)?;
        if event.kind == crate::EventKind::TextFragment {
            return self.get_text(id);
        }
        if event.kind == crate::EventKind::LinkFragment {
            let url = crate::link_fragments::read_event(&event)?;
            return Ok(json!({"id":id,"revision":event.revision,"title":url,"text":url}));
        }
        if !crate::file_types::is_markdown(std::path::Path::new(&event.name)) {
            return Err(ApiError::new(
                "not_text",
                "This file is not a Markdown document",
            ));
        }
        crate::open_file::validate(&event)?;
        let mut bytes = Vec::new();
        std::fs::File::open(&event.path)?
            .take((crate::text_fragments::MAX_TEXT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > crate::text_fragments::MAX_TEXT_BYTES {
            return Err(ApiError::new(
                "text_too_large",
                "Markdown preview supports up to 1 MiB",
            ));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| ApiError::new("invalid_text", "Markdown must be UTF-8"))?;
        crate::open_file::validate(&event)?;
        Ok(
            json!({"id":id,"revision":event.revision,"title":crate::text_fragments::title(&text),"text":text}),
        )
    }
    pub(crate) fn selected_refs(&self) -> Result<Vec<SelectionRef>> {
        let ids = if self.interaction.selected.is_empty() {
            self.interaction.cursor.into_iter().collect()
        } else {
            self.interaction.selected.clone()
        };
        ids.into_iter()
            .map(|id| {
                self.get(id).map(|e| SelectionRef {
                    id,
                    revision: e.revision,
                })
            })
            .collect()
    }
    pub(crate) fn selection_is_links(&self, items: &[SelectionRef]) -> Result<bool> {
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
            if event.kind != crate::EventKind::LinkFragment {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn selection_is_text(&self, items: &[SelectionRef]) -> Result<bool> {
        if items.is_empty() || items.len() > 1000 {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 selected objects",
            ));
        }
        let mut all = true;
        for item in items {
            let event = self.get(item.id)?;
            if event.revision != item.revision {
                return Err(ApiError::new("stale_selection", "Selected content changed"));
            }
            all &= event.kind == crate::EventKind::TextFragment;
        }
        Ok(all)
    }
    /// Checks recorded complete text, not its truncated display preview.
    /// Copy publication independently revalidates the underlying revisions.
    pub(crate) fn selection_is_pods(&self, items: &[SelectionRef]) -> Result<bool> {
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
            if event.kind != crate::EventKind::TextFragment {
                return Ok(false);
            }
            let content: String = self.db.query_row(
                "SELECT c.content FROM text_fragments t JOIN events e ON e.id=t.event_id JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE t.event_id=?1",
                [item.id], |row| row.get(0),
            )?;
            if !crate::pods::is_pod(&content) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn copy_links(&mut self, items: &[SelectionRef]) -> Result<Value> {
        let snapshot = items
            .iter()
            .cloned()
            .map(|item| {
                let event = self.get(item.id);
                (item, event)
            })
            .collect();
        let mut result = crate::text_fragments::merge_snapshot_kind(snapshot, "\n", true)?;
        result["kind"] = json!("link_fragment");
        self.publish_text_result(result)
    }
    pub(crate) fn copy_text(&mut self, items: &[SelectionRef], separator: &str) -> Result<Value> {
        let result = self.merge_text(items, separator)?;
        self.publish_text_result(result)
    }
    pub(crate) fn publish_text_result(&mut self, mut result: Value) -> Result<Value> {
        if result["merged"] == true {
            let text = result["text"].as_str().unwrap();
            if text.contains('\0') {
                return Err(ApiError::new(
                    "clipboard_unrepresentable",
                    "Windows text Clipboard cannot represent embedded NUL; save TXT instead",
                ));
            }
            let sequence = self
                .text_clipboard
                .lock()
                .map_err(|_| ApiError::new("clipboard_error", "Text clipboard lock poisoned"))?
                .write(text)?;
            let ids = result["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item["id"].as_i64())
                .collect::<Vec<_>>();
            self.record_selection_publication(sequence, &ids);
            self.capture_events.cancel();
            result["text_copied"] = json!(true);
            result["clipboard_sequence"] = json!(sequence);
            result.as_object_mut().unwrap().remove("text");
        } else {
            result["text_copied"] = json!(false);
        }
        Ok(result)
    }
}

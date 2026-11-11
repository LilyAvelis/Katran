//! Explicit external-file paste shares the materialization boundary.
use crate::*;
use serde_json::{Value, json};
impl Service {
    pub fn set_clipboard_files_source(&mut self, source: Box<dyn clipboard_files::Source>) {
        self.clipboard_files = source;
    }
    pub(crate) fn record_clipboard_publication(&mut self, sequence: u32) {
        self.clipboard_sequence = sequence;
        self.transfer_intent=None;
        self.transfer_sequence=None;
    }
    /// Only an exact, validated COPY snapshot may survive a publication.
    /// A new publication always disarms an earlier CUT, even for the same IDs.
    pub(crate) fn record_selection_publication(&mut self,sequence:u32,ids:&[i64]) {
        let copy=self.transfer_intent.take().filter(|(items,cut)| !*cut
            && items.len()==ids.len()
            && items.iter().zip(ids).all(|(item,id)|item.id==*id
                && self.get(*id).is_ok_and(|event|event.revision==item.revision)));
        self.record_clipboard_publication(sequence);
        if let Some(intent)=copy {self.transfer_intent=Some(intent);self.transfer_sequence=Some(sequence);}
    }

    pub(crate) fn paste_files_to_folder(&self, paths: Vec<PathBuf>) -> Result<Value> {
        if paths.is_empty() || paths.len() > 1000 {
            return Err(ApiError::new("invalid_argument", "Expected 1..1000 files"));
        }
        let destination = self
            .work_folder
            .current()
            .ok_or_else(|| ApiError::new("folder_not_selected", "Choose a working folder first"))?;
        let mut items = Vec::new();
        for (index, path) in paths.into_iter().enumerate() {
            let id = -(index as i64) - 1;
            let result = (|| {
                let input = jobs_service::prepare_reference(
                    path,
                    "external_paste".into(),
                    origin::Origin::default(),
                )?;
                let event = Event {
                    kind: EventKind::FileReference,
                    text_preview: None,
                    delete_policy: source_policy::DeletePolicy::ShelfOnly,
                    revision: format!("ref:{}", input.content_hash),
                    origin: input.origin,
                    id,
                    detected_at_ms: now_ms(),
                    modified_at_ms: sorting::modified_ms(input.modified),
                    source: input.source,
                    name: input.name,
                    storage: "reference".into(),
                    path: input.path,
                    size: input.size,
                    available: true,
                };
                let receipt = folder_transfer::materialize(&[event], destination)?;
                serde_json::to_value(&receipt.items[0])
                    .map_err(|e| ApiError::new("serialization_error", e))
            })();
            items.push(match result {
                Ok(value) => value,
                Err(error) => json!({"id":id,"status":"failed","path":null,"error":error}),
            });
        }
        Ok(json!({"cut":false,"external":true,"items":items}))
    }
    pub(crate) fn paste_clipboard_to_folder(&mut self) -> Result<Value> {
        self.transfer_intent = None;
        self.transfer_sequence = None;
        self.paste_files_to_folder(self.clipboard_files.files()?)
    }
}

//! Private prepared receipts issued by SourceRuntime; never deserialized from a client.
use crate::{
    source_policy::{DeletePolicy, OriginalDeleteMode, SourceId},
    *,
};
impl Service {
    pub(crate) fn commit_source_file(
        &mut self,
        prepared: source_runtime::PreparedSourceFile,
    ) -> Result<Option<Event>> {
        let configuration = prepared.configuration;
        let item = prepared.candidate;
        if !self.current_source(&configuration)? {
            return Err(ApiError::new(
                "stale_source_generation",
                "Source configuration changed",
            ));
        }
        let root = configuration.root.canonicalize()?;
        let path = item.path.canonicalize()?;
        if path.parent().is_some_and(|parent|folder_watch::is_recovery_dir(&root,parent)) {
            return Err(ApiError::new("reserved_recovery_path","Recovery staging is not source content"));
        }
        if root != item.root || !path.starts_with(&root) || path.starts_with(&self.root) {
            return Err(ApiError::new(
                "unsafe_source",
                "Source escaped configured folder",
            ));
        }
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file()
            || metadata.len() != item.size
            || metadata.modified()? != item.modified
        {
            return Err(ApiError::new(
                "source_changed",
                "Source changed after preparation",
            ));
        }
        let source = configuration.source_id.as_str();
        let key = path.to_string_lossy().to_lowercase();
        let root_key = root.to_string_lossy().to_lowercase();
        let receipt:Option<(String,i64)>=self.db.query_row("SELECT hash,event_id FROM source_receipts WHERE source_id=?1 AND root_key=?2 AND path_key=?3",params![source,root_key,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((hash, id)) = receipt
            && hash == item.content_hash
            && self.get(id).is_err()
        {
            return Ok(None);
        }
        let previous:Option<(i64,String)>=self.db.query_row("SELECT p.event_id,e.reference_hash FROM source_projection p JOIN events e ON e.id=p.event_id WHERE p.source_id=?1 AND p.generation=?2 AND p.path_key=?3",params![source,configuration.generation,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((id, hash)) = previous
            && hash == item.content_hash
        {
            self.db.execute(
                "UPDATE events SET modified_at_ms=?1 WHERE id=?2",
                params![sorting::modified_ms(item.modified), id],
            )?;
            self.db.execute(
                "UPDATE source_projection SET available=1 WHERE event_id=?1",
                [id],
            )?;
            return Ok(None);
        }
        let policy = DeletePolicy::LiveOriginal {
            source_id: configuration.source_id,
            generation: configuration.generation,
            mode: match configuration.source_id {
                SourceId::Sharex => OriginalDeleteMode::Recycle,
                SourceId::Downloads => OriginalDeleteMode::Recycle,
            },
        };
        let tx = self.db.unchecked_transaction()?;
        // A pre-hardening reference is reused only after this worker verified the actual file.
        let existing:Option<i64>=tx.query_row("SELECT id FROM events WHERE id=(SELECT MAX(id) FROM events WHERE source=?1 AND path=?2 AND blob_hash IS NULL AND (legacy_source_ref=1 OR json_extract(delete_policy_json,'$.kind')='live_original')) AND reference_hash=?3",params![source,path.to_string_lossy(),item.content_hash],|r|r.get(0)).optional()?;
        let id = if let Some(id) = existing {
            id
        } else {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode filename"))?;
            let origin = origin::Origin {
                source_app: Some(
                    if configuration.source_id == SourceId::Sharex {
                        "ShareX folder"
                    } else {
                        "Downloads"
                    }
                    .into(),
                ),
                source_domain: None,
            };
            tx.execute("INSERT INTO events(detected_at_ms,source,name,path,size,blob_hash,origin_json,reference_hash) VALUES(?1,?2,?3,?4,?5,NULL,?6,?7)",params![now_ms(),source,name,path.to_string_lossy(),item.size,serde_json::to_string(&origin).unwrap(),item.content_hash])?;
            tx.last_insert_rowid()
        };
        tx.execute(
            "UPDATE events SET delete_policy_json=?1,legacy_source_ref=0,modified_at_ms=?3 WHERE id=?2",
            params![serde_json::to_string(&policy).unwrap(), id, sorting::modified_ms(item.modified)],
        )?;
        tx.execute("INSERT INTO source_projection VALUES(?1,?2,?3,?4,1) ON CONFLICT(source_id,generation,path_key) DO UPDATE SET event_id=excluded.event_id,available=1",params![source,configuration.generation,key,id])?;
        tx.execute("INSERT INTO source_receipts VALUES(?1,?2,?3,?4,?5) ON CONFLICT(source_id,root_key,path_key) DO UPDATE SET hash=excluded.hash,event_id=excluded.event_id",params![source,root_key,key,item.content_hash,id])?;
        if configuration.source_id == SourceId::Sharex {
            tx.execute("INSERT OR IGNORE INTO classification_work(event_id,revision,kind,source_generation) VALUES(?1,?2,'source',?3)",params![id,format!("ref:{}",item.content_hash),configuration.generation])?;
        }
        tx.commit()?;
        Ok(Some(self.get(id)?))
    }
    pub(crate) fn screenshot_match_inputs(&self) -> Result<serde_json::Value> {
        let Some(root) = self
            .settings
            .sharex_folder
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
        else {
            return Ok(serde_json::json!({"events":[]}));
        };
        let mut events = Vec::new();
        let mut cursor = None;
        loop {
            let page = storage::query_events(&self.db, "", 1000, cursor)?;
            let exhausted = page.len() < 1000;
            cursor = page.last().map(|e| e.id);
            events.extend(page.into_iter().filter(|e| {
                (e.source == "sharex" && e.path.starts_with(&root))
                    || matches!(e.source.as_str(), "clipboard_image" | "clipboard")
            }));
            if exhausted {
                break;
            }
        }
        Ok(serde_json::json!({"events":events}))
    }
}

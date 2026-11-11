//! Delete a revision-bound Folder selection; the folder itself is never an item.
use crate::*;
use serde_json::{Value,json};
impl Service {
    pub(crate) fn delete_folder_selection(&mut self,ids:Vec<i64>,confirmed:bool)->Result<Value> {
        let mut seen=std::collections::HashSet::new();
        let ids:Vec<_>=ids.into_iter().filter(|id|seen.insert(*id)).collect();
        if ids.is_empty() || ids.len()>1000 || ids.iter().any(|id|*id>=0) {return Err(ApiError::new("invalid_argument","Expected only Folder item IDs"));}
        if ids.len()>1 && !confirmed {return Ok(json!({"confirmation_required":true,"ids":ids,"original_files":true,"permanent_files":false,"recycle_bin":true,"deleted":0}));}
        let root=self.work_folder.current().ok_or_else(||ApiError::new("folder_not_selected","Choose a working folder first"))?.to_owned();
        let mut results=Vec::new();let mut removed=Vec::new();
        for id in &ids {
            let result=(|| {
                let event=self.get(*id)?;
                if event.path.parent()!=Some(root.as_path()) {return Err(ApiError::new("unsafe_source","Folder item escaped the selected directory"));}
                if self.folder_path_pinned(&event.path)? {return Ok(json!({"id":id,"ok":false,"skipped_pinned":true,"status":"skipped_pinned"}));}
                open_file::validate(&event)?;
                let hash=preview::content_hash(&event.path)?;
                let locked=delete_selection::prepare_locked_original(&event.path,&root,&hash)?;
                let current=self.get(*id)?;
                if current.revision!=event.revision || self.folder_path_pinned(&event.path)? {return Err(ApiError::new("stale_selection","Folder selection changed"));}
                let trash=self.trash.lock().map_err(|_|ApiError::new("recycle_failed","Trash adapter lock poisoned"))?;
                let receipt=safe_recycle::recycle_locked(locked,&event.path,&root,&hash,trash.as_ref())?;
                if receipt.recycled {removed.push(*id);}
                Ok(json!({"id":id,"ok":receipt.recycled,"recycle":receipt,"recovery_path":receipt.recovery_path,"original_removed":receipt.original_removed}))
            })();
            results.push(match result {Ok(value)=>value,Err(error)=>json!({"id":id,"ok":false,"error":error})});
        }
        self.interaction.selected.retain(|id|!removed.contains(id));
        if self.interaction.preview.is_some_and(|id|removed.contains(&id)) {self.interaction.set_preview(None);}
        Ok(json!({"ids":ids,"confirmation_required":false,"deleted":removed.len(),"items":results,"original_files":true,"permanent_files":false,"recycle_bin":true}))
    }
    pub(crate) fn folder_path_pinned(&self,path:&Path)->Result<bool> {
        let mut statement=self.db.prepare("SELECT e.path FROM events e JOIN katran_properties k ON k.event_id=e.id WHERE k.pinned=1 AND e.blob_hash IS NULL")?;
        for candidate in statement.query_map([],|row|row.get::<_,String>(0))? {
            let candidate=PathBuf::from(candidate?);
            if candidate==path || candidate.canonicalize().is_ok_and(|p|p==path) {return Ok(true);}
        }
        Ok(false)
    }
}

//! API-first Files pointer ingestion. The original remains outside Katran storage.
use crate::{ApiError, Result, Service, files_pointer, files_store, origin::Origin, sorting};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

pub(crate) type PreparedFiles = Vec<(PathBuf, Result<files_pointer::Pointer>)>;

pub(crate) fn validate_paths(paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() || paths.len() > 1000 {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected 1..1000 file paths",
        ));
    }
    Ok(())
}

impl Service {
    pub(crate) fn add_files(&mut self, paths: Vec<PathBuf>) -> Result<Value> {
        validate_paths(&paths)?;
        let prepared = paths
            .into_iter()
            .map(|input| {
                let pointer = files_pointer::prepare_pointer(&input).map_err(ApiError::from);
                (input, pointer)
            })
            .collect();
        self.commit_files(prepared, self.ingest_origin.clone())
    }

    pub(crate) fn commit_files(
        &mut self,
        prepared: PreparedFiles,
        origin: Origin,
    ) -> Result<Value> {
        let mut items = Vec::with_capacity(prepared.len());
        for (input, prepared) in prepared {
            let result = prepared.and_then(|pointer| self.commit_file(pointer, &origin));
            items.push(match result {
                Ok(event) => json!({"ok":true,"path":input,"event":event}),
                Err(error) => json!({"ok":false,"path":input,"error":error}),
            });
        }
        Ok(json!({"items":items,"atomic":false}))
    }

    pub(crate) fn commit_file(
        &mut self,
        pointer: files_pointer::Pointer,
        origin: &Origin,
    ) -> Result<Value> {
        // Cheap commit fence: hashing stays on the worker. This catches common
        // stale paths but is not an identity guarantee or deletion authority.
        let metadata = fs::symlink_metadata(&pointer.path)?;
        let regular = metadata.file_type().is_file() && !metadata.file_type().is_symlink();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(ApiError::new(
                    "source_changed",
                    "File became a reparse point",
                ));
            }
        }
        if !regular
            || metadata.len() != pointer.size
            || metadata.modified()? != pointer.modified
            || fs::canonicalize(&pointer.path)? != pointer.path
        {
            return Err(ApiError::new(
                "source_changed",
                "File changed before pointer commit",
            ));
        }
        if pointer.path.starts_with(self.root.canonicalize()?) {
            return Err(ApiError::new(
                "managed_source",
                "Katran's managed files cannot become Files pointers",
            ));
        }
        let path = pointer
            .path
            .to_str()
            .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode file path"))?;
        let name = pointer
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ApiError::new("invalid_argument", "Non-Unicode filename"))?;
        let origin = serde_json::to_string(origin)
            .map_err(|error| ApiError::new("invalid_argument", error))?;
        let id = files_store::insert_or_reuse(
            &mut self.db,
            &files_store::PreparedFile {
                path,
                path_key: path,
                name,
                size: pointer.size,
                reference_hash: &pointer.content_hash,
                modified_at_ms: sorting::modified_ms(pointer.modified),
                detected_at_ms: crate::now_ms(),
                origin_json: &origin,
            },
        )?;
        Ok(json!(self.get(id)?))
    }
}

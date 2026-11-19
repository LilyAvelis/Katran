//! External Markdown drafts for the Text shelf.
use crate::{ApiError, Event, Result, Service, open_file, storage, text_fragments};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

fn directory(parent: &Path, name: &str, create: bool) -> Result<PathBuf> {
    let path = parent.join(name);
    if create {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let resolved = path.canonicalize().map_err(|error| {
        if !create && error.kind() == ErrorKind::NotFound {
            ApiError::new(
                "text_draft_missing",
                "Open the card in Obsidian before applying edits",
            )
        } else {
            error.into()
        }
    })?;
    if resolved.parent() != Some(parent)
        || resolved.file_name() != path.file_name()
        || !resolved.is_dir()
    {
        return Err(ApiError::new(
            "invalid_text_vault",
            "Expected a directory in the Katran profile",
        ));
    }
    Ok(resolved)
}

pub(crate) fn vault(root: &Path, create: bool) -> Result<PathBuf> {
    directory(&root.canonicalize()?, "Katran Text", create)
}

pub(crate) fn display_path(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .ok_or_else(|| ApiError::new("invalid_text_vault", "The vault path must be Unicode"))?;
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        Ok(format!(r"\\{unc}"))
    } else {
        Ok(path.strip_prefix(r"\\?\").unwrap_or(path).to_owned())
    }
}

fn draft_location(root: &Path, event: &Event, create: bool) -> Result<(PathBuf, String)> {
    let hash = event
        .revision
        .strip_prefix("blob:")
        .filter(|hash| storage::is_hash(hash));
    let Some(hash) = hash.filter(|_| event.kind == crate::EventKind::TextFragment && event.id > 0)
    else {
        return Err(ApiError::new("not_text", "Choose a saved Text card"));
    };
    let folder = directory(&vault(root, create)?, &format!("Text-{}", event.id), create)?;
    let generation = event.modified_at_ms.unwrap_or(event.detected_at_ms);
    Ok((folder, format!("{generation}-{hash}.md")))
}

fn validate_draft(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() || path.canonicalize()? != path {
        return Err(ApiError::new(
            "invalid_text_draft",
            "Expected a regular Markdown draft inside the vault",
        ));
    }
    Ok(())
}

fn publish_draft(path: &Path, text: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => return validate_draft(path),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let stage = path.with_file_name(format!(
        ".draft-{}.tmp",
        crate::folder_semantic::new_session()
    ));
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stage)?;
    let written = output
        .write_all(text.as_bytes())
        .and_then(|()| output.sync_all());
    drop(output);
    let published = written.and_then(|()| fs::hard_link(&stage, path));
    let _ = fs::remove_file(&stage);
    match published {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => validate_draft(path),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn open_draft(
    root: &Path,
    event: &Event,
    launcher: &dyn open_file::Launcher,
) -> Result<Value> {
    launcher.authorize()?;
    let text = text_fragments::read_text_event(event)?;
    let (folder, name) = draft_location(root, event, true)?;
    let path = folder.join(name);
    publish_draft(&path, &text)?;
    let mut uri = url::Url::parse("obsidian://open").expect("fixed Obsidian URI");
    uri.query_pairs_mut()
        .append_pair("path", &display_path(&path)?);
    let uri = uri.as_str().replace('+', "%20");
    launcher
        .launch(Path::new(&uri), open_file::Operation::Open)
        .map_err(|_| {
            ApiError::new(
                "obsidian_open_failed",
                "Could not open Obsidian. The draft is available in the Text vault.",
            )
        })?;
    Ok(json!({"id":event.id,"obsidian_requested":true,"draft_path":display_path(&path)?}))
}

impl Service {
    pub(crate) fn apply_text_draft(&mut self, id: i64) -> Result<Value> {
        let event = self.get(id)?;
        let (folder, name) = draft_location(&self.root, &event, false)?;
        let path = folder.join(name);
        if matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == ErrorKind::NotFound)
        {
            return Err(ApiError::new(
                "text_draft_conflict",
                "There is no draft for the current revision. Earlier drafts remain in the vault.",
            ));
        }
        validate_draft(&path)?;
        let mut bytes = Vec::new();
        File::open(&path)?
            .take((text_fragments::MAX_TEXT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| ApiError::new("invalid_text_draft", "The Markdown draft must be UTF-8"))?;
        let saved = text_fragments::read_text_event(&event)?;
        let changed = text != saved;
        let revision = if changed {
            let prepared = text_fragments::prepare(
                &self.root,
                self.quota,
                &text,
                &event.source,
                &event.origin,
                &self.reference_paths()?,
            )?;
            storage::replace_text_blob(&mut self.db, &self.root, self.quota, &event, prepared)?
        } else {
            event.revision
        };
        Ok(
            json!({"id":id,"revision":revision,"text_draft_applied":changed,"draft_path":display_path(&path)?}),
        )
    }
}

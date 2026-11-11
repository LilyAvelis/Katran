//! Revision-bound previews for session-only working-folder files.
use std::{fs, io::Read, path::Path};

use crate::{
    ApiError, Event, EventKind, Result, origin::Origin, preview, source_policy::DeletePolicy,
};
use serde_json::{Value, json};

fn validate(path: &Path, revision: &str) -> Result<fs::Metadata> {
    let stale = || ApiError::new("source_changed", "Folder file changed or is unavailable");
    let metadata = fs::symlink_metadata(path).map_err(|_| stale())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(stale());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(stale());
        }
    }
    if preview::revision(path, None) != revision {
        return Err(stale());
    }
    Ok(metadata)
}

pub fn preview_file(
    path: &Path,
    revision: &str,
    max_edge: u32,
    image_only: bool,
    root: &Path,
) -> Result<Value> {
    let metadata = validate(path, revision)?;
    let event = Event {
        kind: EventKind::FileReference,
        text_preview: None,
        delete_policy: DeletePolicy::ShelfOnly,
        revision: revision.into(),
        origin: Origin::default(),
        id: 0,
        detected_at_ms: 0,
        modified_at_ms: None,
        source: "work_folder".into(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        storage: "reference".into(),
        path: path.into(),
        size: metadata.len(),
        available: true,
    };
    let result = if image_only {
        preview::render(root, &event, max_edge)
    } else {
        crate::file_visual::render(root, &event, max_edge)
    }?;
    validate(path, revision)?;
    Ok(result)
}

pub fn read_document(path: &Path, revision: &str) -> Result<Value> {
    if !crate::file_types::is_markdown(path) {
        return Err(ApiError::new(
            "not_text",
            "This file is not a Markdown document",
        ));
    }
    validate(path, revision)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
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
    validate(path, revision)?;
    Ok(json!({"revision": revision, "title": crate::text_fragments::title(&text), "text": text}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_rejects_changed_missing_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.md");
        fs::write(&path, "hello").unwrap();
        let revision = preview::revision(&path, None);
        assert_eq!(read_document(&path, &revision).unwrap()["text"], "hello");
        fs::write(&path, "changed longer").unwrap();
        assert_eq!(
            read_document(&path, &revision).unwrap_err().code,
            "source_changed"
        );
        fs::write(&path, vec![b'a'; crate::text_fragments::MAX_TEXT_BYTES + 1]).unwrap();
        assert_eq!(
            read_document(&path, &preview::revision(&path, None))
                .unwrap_err()
                .code,
            "text_too_large"
        );
        fs::remove_file(&path).unwrap();
        assert_eq!(
            read_document(&path, &revision).unwrap_err().code,
            "source_changed"
        );
    }

    #[test]
    fn image_preview_requires_current_revision_even_on_cache_hit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.png");
        image::RgbaImage::new(8, 4).save(&path).unwrap();
        let revision = preview::revision(&path, None);
        let result = preview_file(&path, &revision, 32, true, dir.path()).unwrap();
        assert_eq!(result["revision"], revision);
        fs::write(&path, b"replacement").unwrap();
        assert_eq!(
            preview_file(&path, &revision, 32, true, dir.path())
                .unwrap_err()
                .code,
            "source_changed"
        );
    }
}

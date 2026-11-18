//! Rename whole collections without changing their contents or membership.
use crate::{ApiError, Event, Result, domain_jobs::Cancellation, interaction::ShelfTab};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

/// Metadata-only capability; external bytes are validated on the worker.
pub(crate) fn can_rename(event: &Event) -> bool {
    crate::collection::is_managed(event)
        || crate::bs_collection::is_managed(event)
        || (event.id < 0
            && event.source == "work_folder"
            && crate::collection_kind::is_archive(event))
}
fn suffix(event: &Event) -> Result<&str> {
    let extension = Path::new(&event.name)
        .extension()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ApiError::new("invalid_collection", "Collection suffix missing"))?;
    if extension.eq_ignore_ascii_case("bull") {
        Ok("bull")
    } else if extension.eq_ignore_ascii_case("bs") {
        Ok("bs")
    } else {
        Err(ApiError::new(
            "invalid_collection",
            "Expected BULL or BS collection",
        ))
    }
}
fn valid_leaf(name: &str) -> bool {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name.len() > 200
        || name.encode_utf16().count() > 255
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
    {
        return false;
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return false;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(s) = base.strip_prefix(prefix)
            && matches!(
                s,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        {
            return false;
        }
    }
    true
}
/// Accept a stem or the same suffix, and always preserve the collection's type.
pub(crate) fn full_name(event: &Event, requested: &str) -> Result<String> {
    if !can_rename(event) {
        return Err(ApiError::new(
            "unsupported_collection",
            "Expected a collection",
        ));
    }
    let ext = suffix(event)?;
    if requested.is_empty() || requested.trim() != requested || requested.ends_with(['.', ' ']) {
        return Err(ApiError::new(
            "invalid_argument",
            "Enter a safe collection name",
        ));
    }
    let incoming = Path::new(requested).extension().and_then(|s| s.to_str());
    if incoming.is_some_and(|s| {
        (s.eq_ignore_ascii_case("bs") || s.eq_ignore_ascii_case("bull"))
            && !s.eq_ignore_ascii_case(ext)
    }) {
        return Err(ApiError::new(
            "invalid_argument",
            "Collection extension cannot be changed",
        ));
    }
    let result = if incoming.is_some_and(|s| s.eq_ignore_ascii_case(ext)) {
        requested.to_owned()
    } else {
        format!("{requested}.{ext}")
    };
    if !valid_leaf(&result) {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected a safe plain filename",
        ));
    }
    Ok(result)
}

/// Name and revision are checked together under the SQLite writer lock.
pub(crate) fn rename_managed(
    db: &mut Connection,
    event: &Event,
    tab: ShelfTab,
    requested: &str,
) -> Result<String> {
    if event.id <= 0
        || !(crate::collection::is_managed(event) || crate::bs_collection::is_managed(event))
    {
        return Err(ApiError::new(
            "unsupported_collection",
            "Expected a managed collection",
        ));
    }
    let name = full_name(event, requested)?;
    let Some(hash) = event.revision.strip_prefix("blob:") else {
        return Err(ApiError::new(
            "stale_selection",
            "Invalid collection revision",
        ));
    };
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !crate::storage::visible_collection_input(&tx, event.id, tab)? {
        return Err(ApiError::new(
            "stale_selection",
            "Collection is not visible on this shelf",
        ));
    }
    let current: Option<(String, String, String, String)> = tx.query_row(
        "SELECT name,source,path,blob_hash FROM events WHERE id=?1 AND blob_hash IS NOT NULL AND NOT EXISTS(SELECT 1 FROM bull_members WHERE member_id=?1)",
        [event.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional()?;
    let Some((current_name, source, path, current_hash)) = current else {
        return Err(ApiError::new(
            "stale_selection",
            "Collection no longer exists",
        ));
    };
    if current_name != event.name
        || source != event.source
        || Path::new(&path) != event.path
        || current_hash != hash
    {
        return Err(ApiError::new(
            "stale_selection",
            "Collection changed before rename",
        ));
    }
    let changed = tx.execute("UPDATE events SET name=?1 WHERE id=?2 AND name=?3 AND source=?4 AND path=?5 AND blob_hash=?6",
        params![name, event.id, event.name, event.source, path, hash])?;
    if changed != 1 {
        return Err(ApiError::new(
            "stale_selection",
            "Collection changed before rename",
        ));
    }
    tx.execute(
        "UPDATE katran_properties SET display_name=NULL WHERE event_id=?1",
        [event.id],
    )?;
    tx.commit()?;
    Ok(name)
}

struct Cancellable<'a, R> {
    inner: R,
    cancel: &'a Cancellation,
}
impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("cancelled"));
        }
        self.inner.read(buf)
    }
}
fn validate_collection(event: &Event, cancel: &Cancellation) -> Result<()> {
    let kind = crate::collection_kind::probe(event)?;
    let input = Cancellable {
        inner: std::fs::File::open(&event.path)?,
        cancel,
    };
    let result: Result<()> = match &kind {
        crate::collection_kind::Kind::Images => baloh_collection::codec::read(input)
            .map(|_| ())
            .map_err(Into::into),
        crate::collection_kind::Kind::Links => baloh_collection::links::read(input)
            .map(|_| ())
            .map_err(Into::into),
        crate::collection_kind::Kind::Text => baloh_collection::text::read(input)
            .map(|_| ())
            .map_err(Into::into),
        crate::collection_kind::Kind::Files(_) | crate::collection_kind::Kind::Bs => {
            crate::collection_kind::read_raw(input, &kind).map(|_| ())
        }
    };
    cancel.check()?;
    result.map_err(|e| ApiError::new("invalid_collection", e.message))
}

#[cfg(windows)]
pub(crate) struct PreparedFolderRename {
    event: Event,
    folder: PathBuf,
    destination: PathBuf,
    original: crate::delete_selection::LockedOriginal,
    _folder_handle: std::fs::File,
}
#[cfg(not(windows))]
pub(crate) struct PreparedFolderRename {
    event: Event,
    folder: PathBuf,
}
impl PreparedFolderRename {
    pub(crate) fn event(&self) -> &Event {
        &self.event
    }
    pub(crate) fn folder(&self) -> &Path {
        &self.folder
    }
    #[cfg(windows)]
    pub(crate) fn publish(mut self) -> Result<PathBuf> {
        if crate::delete_selection::locked_path(&self.original)?
            != self.event.path.canonicalize()?
        {
            return Err(ApiError::new(
                "source_changed",
                "Collection moved before rename",
            ));
        }
        if self.destination == self.event.path {
            return Ok(self.destination);
        }
        match crate::delete_selection::rename_locked_original(&mut self.original, &self.destination)
        {
            Ok(()) => Ok(self.destination),
            Err(_error) if std::fs::symlink_metadata(&self.destination).is_ok() => Err(
                ApiError::new("already_exists", "A file with that name already exists"),
            ),
            Err(error) => Err(error),
        }
    }
    #[cfg(not(windows))]
    pub(crate) fn publish(self) -> Result<PathBuf> {
        Err(ApiError::new(
            "unsupported_platform",
            "Folder rename requires Windows",
        ))
    }
}

#[cfg(windows)]
pub(crate) fn prepare_folder(
    event: Event,
    folder: &Path,
    requested: &str,
    cancel: &Cancellation,
) -> Result<PreparedFolderRename> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    cancel.check()?;
    if event.id >= 0 || event.source != "work_folder" {
        return Err(ApiError::new(
            "unsupported_collection",
            "Expected a Folder collection",
        ));
    }
    let name = full_name(&event, requested)?;
    let folder = folder.canonicalize()?;
    let metadata = std::fs::symlink_metadata(&folder)?;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err(ApiError::new(
            "unsafe_source",
            "Folder is not a regular directory",
        ));
    }
    let folder_handle = std::fs::OpenOptions::new()
        .access_mode(0x80)
        .share_mode(3)
        .custom_flags(0x02000000 | 0x00200000)
        .open(&folder)?;
    let opened = folder_handle.metadata()?;
    if !opened.is_dir() || opened.file_attributes() & 0x400 != 0 {
        return Err(ApiError::new("unsafe_source", "Folder changed"));
    }
    let path = event.path.canonicalize()?;
    if path.parent() != Some(folder.as_path()) || event.path != path {
        return Err(ApiError::new(
            "unsafe_source",
            "Collection is not a direct Folder item",
        ));
    }
    crate::open_file::validate(&event)?;
    cancel.check()?;
    let hash = crate::preview::content_hash(&path)?;
    cancel.check()?;
    let original = crate::delete_selection::prepare_locked_original(&path, &folder, &hash)?;
    if crate::delete_selection::locked_path(&original)? != path {
        return Err(ApiError::new(
            "source_changed",
            "Collection identity changed",
        ));
    }
    crate::open_file::validate(&event)?;
    validate_collection(&event, cancel)?;
    cancel.check()?;
    Ok(PreparedFolderRename {
        event,
        destination: folder.join(name),
        folder,
        original,
        _folder_handle: folder_handle,
    })
}
#[cfg(not(windows))]
pub(crate) fn prepare_folder(
    _: Event,
    _: &Path,
    _: &str,
    _: &Cancellation,
) -> Result<PreparedFolderRename> {
    Err(ApiError::new(
        "unsupported_platform",
        "Folder rename requires Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_leaf_rejects_windows_aliases_and_path_separators() {
        for name in [
            "CON.bull",
            "com¹.bs",
            "aux .bull",
            "a/b.bull",
            "x:stream.bs",
            "x. ",
            "..",
        ] {
            assert!(!valid_leaf(name), "{name}");
        }
        assert!(valid_leaf("Коллекция.bull"));
        assert!(valid_leaf("com10.bs"));
    }
    #[test]
    fn managed_name_cas_preserves_content_properties_and_members() {
        let root = tempfile::tempdir().unwrap();
        let mut service = crate::Service::open(root.path(), crate::DEFAULT_QUOTA).unwrap();
        let origin = crate::origin::Origin::default();
        let id = crate::storage::ingest_bytes(
            &mut service.db,
            &service.root,
            service.quota,
            crate::storage::ByteIngest {
                name: "old.bull",
                source: "image_collection",
                bytes: b"synthetic",
                origin: &origin,
            },
        )
        .unwrap();
        let member = crate::storage::ingest_bytes(
            &mut service.db,
            &service.root,
            service.quota,
            crate::storage::ByteIngest {
                name: "page.png",
                source: "manual",
                bytes: b"page",
                origin: &origin,
            },
        )
        .unwrap();
        service
            .db
            .execute(
                "INSERT INTO bull_members(collection_id,member_id,position) VALUES(?1,?2,0)",
                params![id, member],
            )
            .unwrap();
        service.db.execute("INSERT INTO katran_properties(event_id,pinned,display_name,opener_path) VALUES(?1,1,'Alias','example.exe')", [id]).unwrap();
        service
            .db
            .execute(
                "INSERT INTO katran_tags(event_id,tag_key,tag_display) VALUES(?1,'test','Test')",
                [id],
            )
            .unwrap();
        let event = service.get(id).unwrap();
        let bytes = std::fs::read(&event.path).unwrap();
        assert_eq!(
            rename_managed(&mut service.db, &event, ShelfTab::Images, "новое").unwrap(),
            "новое.bull"
        );
        let renamed = service.get(id).unwrap();
        assert_eq!(renamed.name, "новое.bull");
        assert_eq!(renamed.revision, event.revision);
        assert_eq!(renamed.detected_at_ms, event.detected_at_ms);
        assert_eq!(std::fs::read(&renamed.path).unwrap(), bytes);
        assert_eq!(
            crate::storage::collection_members(&service.db, id)
                .unwrap()
                .get(&0),
            Some(&member)
        );
        let properties = crate::pinned::properties(&service.db, id).unwrap();
        assert!(properties.pinned);
        assert_eq!(properties.display_name, None);
        assert_eq!(properties.opener.as_deref(), Some(Path::new("example.exe")));
        assert_eq!(crate::pinned::tags(&service.db, id).unwrap().len(), 1);
        assert_eq!(
            rename_managed(&mut service.db, &event, ShelfTab::Images, "other")
                .unwrap_err()
                .code,
            "stale_selection"
        );
        assert!(full_name(&renamed, "wrong.bs").is_err());
        assert_eq!(full_name(&renamed, "next.BULL").unwrap(), "next.BULL");
    }
}

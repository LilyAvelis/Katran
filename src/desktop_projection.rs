//! Projection of managed desktop assets into the ordinary shelf event model.
use crate::{ApiError, Event, EventKind, Result, origin::Origin, source_policy::DeletePolicy};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// Desktop IDs occupy the lower quarter of i64; Folder must reserve this range.
pub(crate) const DESKTOP_ID_MAX: i64 = -(1_i64 << 62) - 1;
pub(crate) fn is_desktop_id(id: i64) -> bool {
    id <= DESKTOP_ID_MAX
}

pub(crate) fn browse(
    mut events: Vec<Event>,
    query: &str,
    limit: u32,
    cursor: Option<&str>,
    sort: crate::sorting::SortSpec,
    pinned_ranks: &std::collections::HashMap<i64, i64>,
    context: (u64, bool, Option<&str>, Option<u8>),
) -> Result<(Vec<Event>, Option<String>)> {
    use crate::sorting::{self, SortMode};
    if !(1..=1000).contains(&limit) {
        return Err(ApiError::new("invalid_argument", "Expected limit 1..1000"));
    }
    let needle = query.to_lowercase();
    events.retain(|event| event.name.to_lowercase().contains(&needle));
    events.sort_by(|a, b| {
        let order = match sort.mode {
            SortMode::Arrival | SortMode::Modified => a.modified_at_ms.cmp(&b.modified_at_ms),
            SortMode::Pinned => match (pinned_ranks.get(&a.id), pinned_ranks.get(&b.id)) {
                (Some(left), Some(right)) => left.cmp(right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.detected_at_ms.cmp(&a.detected_at_ms),
            },
            SortMode::Extension => {
                sorting::extension_key(&a.name).cmp(&sorting::extension_key(&b.name))
            }
            SortMode::Shuffle => {
                sorting::shuffle_key(a.id, sort.seed).cmp(&sorting::shuffle_key(b.id, sort.seed))
            }
            SortMode::Natural => sorting::natural_key(&a.name).cmp(&sorting::natural_key(&b.name)),
            SortMode::Size => a.size.cmp(&b.size),
            SortMode::Name => sorting::name_key(&a.name).cmp(&sorting::name_key(&b.name)),
            SortMode::Origin => a.source.cmp(&b.source),
        };
        let order = order.then_with(|| a.id.cmp(&b.id));
        if sort.reverse && sort.mode != SortMode::Pinned {
            order.reverse()
        } else {
            order
        }
    });
    // Bind the offset to both the view and its current metadata snapshot. This
    // stays metadata-only: managed file contents are checked at action time.
    let ranks: Vec<_> = events
        .iter()
        .map(|event| pinned_ranks.get(&event.id))
        .collect();
    let snapshot = serde_json::to_vec(&(&needle, sort, context, &events, ranks))
        .map_err(|_| ApiError::new("invalid_cursor", "Cannot encode Desktop cursor"))?;
    let fingerprint = format!("{:x}", Sha256::digest(snapshot));
    let start = match cursor {
        None => 0,
        Some(value) => {
            let mut parts = value.split(':');
            let parsed = match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some("d1"), Some(offset), Some(digest), None) if digest == fingerprint => {
                    offset.parse::<usize>().ok()
                }
                _ => None,
            };
            parsed
                .ok_or_else(|| ApiError::new("invalid_cursor", "Stale or invalid Desktop cursor"))?
        }
    };
    if start == 0 && cursor.is_some() || start >= events.len() && cursor.is_some() {
        return Err(ApiError::new(
            "invalid_cursor",
            "Desktop cursor is out of range",
        ));
    }
    let end = start.saturating_add(limit as usize).min(events.len());
    let next = (end < events.len()).then(|| format!("d1:{end}:{fingerprint}"));
    Ok((events.drain(start..end).collect(), next))
}

fn stable_id(typed_id: &str) -> i64 {
    let digest = Sha256::digest(typed_id.as_bytes());
    let bits =
        u64::from_be_bytes(digest[..8].try_into().expect("digest size")) & ((1_u64 << 62) - 1);
    i64::MIN + bits as i64
}
pub(crate) fn id_for_typed(typed_id: &str) -> i64 {
    stable_id(typed_id)
}

/// Read and validate the current managed catalogue. Never imports or copies source files.
pub(crate) fn list(root: &Path) -> Result<Vec<Event>> {
    let images = crate::theme_backgrounds::list_fast(root)?;
    let videos =
        crate::video_backgrounds::catalogue(root).map_err(|e| ApiError::new("invalid_video", e))?;
    let wallpaper_videos = crate::desktop_assets::wallpaper_catalogue(root)?;
    let mut events = Vec::new();
    let mut ids = HashSet::new();
    for (kind, items) in [
        ("image", images["backgrounds"].as_array()),
        ("video", videos.as_array()),
        ("wallpaper_video", Some(&wallpaper_videos)),
    ] {
        for item in items.into_iter().flatten() {
            let digest = item["id"]
                .as_str()
                .ok_or_else(|| ApiError::new("invalid_background", "Missing asset ID"))?;
            let typed_id = format!("{kind}:{digest}");
            let id = stable_id(&typed_id);
            if !ids.insert(id) {
                return Err(ApiError::new(
                    "desktop_id_collision",
                    "Desktop asset IDs collide",
                ));
            }
            let path = PathBuf::from(
                item["path"]
                    .as_str()
                    .ok_or_else(|| ApiError::new("invalid_background", "Missing asset path"))?,
            );
            let metadata = fs::metadata(&path)?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(crate::sorting::modified_ms);
            events.push(Event {
                kind: EventKind::FileReference,
                text_preview: None,
                delete_policy: DeletePolicy::ShelfOnly,
                revision: crate::preview::revision(&path, None),
                origin: Origin::default(),
                id,
                detected_at_ms: modified.unwrap_or(0),
                modified_at_ms: modified,
                source: "desktop".into(),
                name: item["name"].as_str().unwrap_or("Background").to_owned(),
                storage: "reference".into(),
                path,
                size: metadata.len(),
                available: true,
            });
        }
    }
    Ok(events)
}

/// Re-resolve an ID and its typed identity from current validated managed data.
pub(crate) fn get(root: &Path, id: i64) -> Result<(Event, String)> {
    if !is_desktop_id(id) {
        return Err(ApiError::new("stale_selection", "Invalid desktop asset ID"));
    }
    let event = list(root)?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| ApiError::new("stale_selection", "Desktop asset no longer exists"))?;
    let digest = event
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ApiError::new("invalid_background", "Missing asset ID"))?;
    let kind = if event
        .path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "wallpaper-videos")
    {
        "wallpaper_video"
    } else if event
        .path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        "image"
    } else {
        "video"
    };
    let typed = format!("{kind}:{digest}");
    Ok((event, typed))
}

/// Reject a selection whose revision no longer matches the managed asset.
pub(crate) fn resolve(root: &Path, id: i64, expected_revision: &str) -> Result<(Event, String)> {
    let (event, typed) = get(root, id)?;
    if event.revision != expected_revision {
        return Err(ApiError::new(
            "stale_selection",
            "Desktop asset changed since selection",
        ));
    }
    let digest = typed
        .split_once(':')
        .map(|(_, id)| id)
        .ok_or_else(|| ApiError::new("invalid_background", "Invalid asset ID"))?;
    if crate::preview::content_hash(&event.path)? != digest
        || crate::preview::revision(&event.path, None) != expected_revision
    {
        return Err(ApiError::new(
            "stale_selection",
            "Desktop asset contents changed since selection",
        ));
    }
    Ok((event, typed))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_ids_are_reserved_and_stale_selection_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let originals = tempfile::tempdir().unwrap();
        let source = originals.path().join("synthetic.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let managed = crate::theme_backgrounds::import(root.path(), &source).unwrap();
        let a = list(root.path()).unwrap().remove(0);
        let b = list(root.path()).unwrap().remove(0);
        assert_eq!(a.id, b.id);
        assert!(is_desktop_id(a.id));
        assert!(!is_desktop_id(-1));
        assert!(!is_desktop_id(-(1_i64 << 62)));
        let (_, typed) = resolve(root.path(), a.id, &a.revision).unwrap();
        assert_eq!(typed, format!("image:{}", managed["id"].as_str().unwrap()));
        assert_eq!(
            resolve(root.path(), a.id, "ref:wrong").unwrap_err().code,
            "stale_selection"
        );
        fs::remove_file(&a.path).unwrap();
        assert_eq!(
            resolve(root.path(), a.id, &a.revision).unwrap_err().code,
            "stale_selection"
        );
        assert!(source.exists());
    }
    #[test]
    fn identical_digest_different_types_have_distinct_ids() {
        let digest = "a".repeat(64);
        assert_ne!(
            stable_id(&format!("image:{digest}")),
            stable_id(&format!("video:{digest}"))
        );
    }

    #[test]
    fn video_resolution_checks_content_against_managed_id() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("video-backgrounds");
        fs::create_dir(&dir).unwrap();
        let bytes = b"synthetic video";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let path = dir.join(format!("{digest}.mp4"));
        fs::write(&path, bytes).unwrap();
        fs::write(dir.join(format!("{digest}.json")), serde_json::json!({"id":digest,"name":"test.mp4","codec":"h264","width":10,"height":10,"duration":1.0}).to_string()).unwrap();
        let event = list(root.path()).unwrap().remove(0);
        assert_eq!(
            resolve(root.path(), event.id, &event.revision).unwrap().1,
            format!("video:{digest}")
        );
        fs::write(&path, b"different video").unwrap();
        let changed = list(root.path()).unwrap().remove(0);
        assert_eq!(
            resolve(root.path(), changed.id, &changed.revision)
                .unwrap_err()
                .code,
            "stale_selection"
        );
    }
}

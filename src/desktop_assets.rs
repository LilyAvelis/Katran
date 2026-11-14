//! Unified catalogue of Katran-managed desktop background copies.
use crate::{ApiError, Result, theme::ThemePreferences};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

// Stage complete bytes in the same directory so published names never expose
// partially written content. Temporary names are ignored by the catalogue.
fn staged_file(dir: &Path, write: impl FnOnce(&mut fs::File) -> Result<()>) -> Result<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = dir.join(format!(
        ".katran-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        write(&mut file)?;
        file.sync_all()?;
        Ok(path.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result
}

fn publish_new(temp: &Path, target: &Path) -> std::io::Result<()> {
    // A hard link provides an atomic no-replace publication on all supported
    // platforms; both paths are in the same managed directory.
    fs::hard_link(temp, target)
}

// Wallpaper Engine accepts more formats and lengths than Katran's own video
// background player. Keep their catalogues and typed identities separate.
fn safe_metadata(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
fn wallpaper_dir(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let dir = root.join("wallpaper-videos");
    fs::create_dir_all(&dir)?;
    let metadata = fs::symlink_metadata(&dir)?;
    if !metadata.is_dir()
        || !safe_metadata(&metadata)
        || dir.canonicalize()?.parent() != Some(root.as_path())
    {
        return Err(ApiError::new(
            "unsafe_background",
            "Unsafe wallpaper video directory",
        ));
    }
    Ok(dir)
}

fn wallpaper_video(root: &Path, id: &str) -> Result<Value> {
    if !crate::video_backgrounds::valid_id(id) {
        return Err(ApiError::new("invalid_video", "Invalid wallpaper video ID"));
    }
    let dir = wallpaper_dir(root)?;
    let metadata_path = dir.join(format!("{id}.json"));
    let meta = fs::symlink_metadata(&metadata_path)?;
    if !meta.is_file() || !safe_metadata(&meta) || meta.len() > 16384 {
        return Err(ApiError::new("invalid_video", "Unsafe wallpaper metadata"));
    }
    let mut data = Vec::new();
    fs::File::open(metadata_path)?
        .take(16385)
        .read_to_end(&mut data)?;
    let mut item: Value =
        serde_json::from_slice(&data).map_err(|e| ApiError::new("invalid_video", e))?;
    let ext = item["extension"].as_str().unwrap_or("");
    if item["id"].as_str() != Some(id) || !matches!(ext, "mp4" | "m4v" | "mov" | "mkv" | "webm") {
        return Err(ApiError::new("invalid_video", "Invalid wallpaper metadata"));
    }
    let path = dir.join(format!("{id}.{ext}"));
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || !safe_metadata(&meta) {
        return Err(ApiError::new("invalid_video", "Unsafe wallpaper video"));
    }
    item["path"] = json!(path);
    Ok(item)
}

pub(crate) fn wallpaper_catalogue(root: &Path) -> Result<Vec<Value>> {
    let dir = wallpaper_dir(root)?;
    let mut ids = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.strip_suffix(".json"))
            && crate::video_backgrounds::valid_id(id)
        {
            ids.push(id.to_owned());
        }
    }
    ids.sort();
    // One stale item must not prevent the remaining Desktop shelf from opening.
    Ok(ids
        .iter()
        .filter_map(|id| wallpaper_video(root, id).ok())
        .collect())
}

fn wallpaper_active(root: &Path) -> Result<Option<String>> {
    let path = wallpaper_dir(root)?.join("active");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(meta) if meta.is_file() && safe_metadata(&meta) && meta.len() == 64 => {
            let id = fs::read_to_string(path)?;
            if crate::video_backgrounds::valid_id(&id) {
                Ok(Some(id))
            } else {
                Err(ApiError::new(
                    "invalid_video",
                    "Invalid active wallpaper receipt",
                ))
            }
        }
        Ok(_) => Err(ApiError::new(
            "invalid_video",
            "Invalid active wallpaper receipt",
        )),
    }
}

/// Register bytes only after Wallpaper Engine accepted them. A separate copy
/// means deleting a shelf entry never removes the file currently used by WE.
pub(crate) fn register_wallpaper_video(
    root: &Path,
    staged_path: &Path,
    original_path: &Path,
) -> Result<Value> {
    let ext = staged_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "mp4" | "m4v" | "mov" | "mkv" | "webm") {
        return Err(ApiError::new(
            "invalid_video",
            "Unsupported wallpaper video extension",
        ));
    }
    let metadata = fs::symlink_metadata(staged_path)?;
    if !metadata.is_file() || !safe_metadata(&metadata) || metadata.len() == 0 {
        return Err(ApiError::new(
            "invalid_video",
            "Wallpaper video is unavailable",
        ));
    }
    let id = crate::preview::content_hash(staged_path)?;
    let dir = wallpaper_dir(root)?;
    let path = dir.join(format!("{id}.{ext}"));
    let temp = staged_file(&dir, |output| {
        std::io::copy(&mut fs::File::open(staged_path)?, output)?;
        Ok(())
    })?;
    let publication = (|| -> Result<()> {
        if crate::preview::content_hash(&temp)? != id {
            return Err(ApiError::new(
                "stale_selection",
                "Wallpaper video changed while saving",
            ));
        }
        match publish_new(&temp, &path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(&path)?;
                if !meta.is_file()
                    || !safe_metadata(&meta)
                    || crate::preview::content_hash(&path)? != id
                {
                    return Err(ApiError::new(
                        "invalid_video",
                        "Existing wallpaper copy does not match",
                    ));
                }
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    })();
    let _ = fs::remove_file(&temp);
    publication?;
    let name: String = original_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Video")
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect();
    let sidecar = dir.join(format!("{id}.json"));
    let temp = staged_file(&dir, |file| {
        file.write_all(
            json!({"id":id,"name":name,"extension":ext})
                .to_string()
                .as_bytes(),
        )?;
        Ok(())
    })?;
    let publication = match publish_new(&temp, &sidecar) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            wallpaper_video(root, &id).map(|_| ())
        }
        Err(e) => Err(e.into()),
    };
    let _ = fs::remove_file(&temp);
    publication?;
    // If a previous receipt cannot be read safely, fail closed rather than
    // replacing it and accidentally authorizing deletion.
    wallpaper_active(root)?;
    let active = dir.join("active");
    let temp = staged_file(&dir, |file| {
        file.write_all(id.as_bytes())?;
        Ok(())
    })?;
    let publication = fs::rename(&temp, active);
    let _ = fs::remove_file(&temp);
    publication?;
    let mut item = wallpaper_video(root, &id)?;
    item["id"] = json!(format!("wallpaper_video:{id}"));
    item["kind"] = json!("wallpaper_video");
    Ok(item)
}

fn wallpaper_matches(path: &Path, wallpapers: &[PathBuf]) -> Result<bool> {
    let managed = path.canonicalize()?;
    for wallpaper in wallpapers {
        // A failed comparison must never silently authorize a deletion.
        let current = wallpaper
            .canonicalize()
            .map_err(|e| ApiError::new("wallpaper_unavailable", e))?;
        #[cfg(windows)]
        let equal = managed
            .to_string_lossy()
            .eq_ignore_ascii_case(&current.to_string_lossy());
        #[cfg(not(windows))]
        let equal = managed == current;
        if equal {
            return Ok(true);
        }
    }
    Ok(false)
}

fn annotate(mut item: Value, kind: &str, active: bool, wallpapers: &[PathBuf]) -> Result<Value> {
    let id = item["id"]
        .as_str()
        .ok_or_else(|| ApiError::new("invalid_background", "Missing background ID"))?
        .to_owned();
    let path = PathBuf::from(
        item["path"]
            .as_str()
            .ok_or_else(|| ApiError::new("invalid_background", "Missing background path"))?,
    );
    let wallpaper = (kind == "image" && wallpaper_matches(&path, wallpapers)?)
        || (kind == "wallpaper_video" && active);
    item["id"] = json!(format!("{kind}:{id}"));
    item["kind"] = json!(kind);
    item["size"] = json!(fs::metadata(&path)?.len());
    item["in_use"] = json!(active || wallpaper);
    item["theme_active"] = json!(active && kind != "wallpaper_video");
    item["wallpaper_active"] = json!(wallpaper);
    Ok(item)
}

/// List managed copies independently of which UI tabs are visible.
pub(crate) fn list(
    root: &Path,
    theme: &ThemePreferences,
    remembered_wallpaper: Option<&str>,
    wallpapers: &[PathBuf],
) -> Result<Value> {
    let images = crate::theme_backgrounds::list(root)?;
    let videos =
        crate::video_backgrounds::catalogue(root).map_err(|e| ApiError::new("invalid_video", e))?;
    let mut items = Vec::new();
    for image in images["backgrounds"].as_array().into_iter().flatten() {
        let active = image["id"].as_str() == theme.custom_background.as_deref()
            || image["id"].as_str() == remembered_wallpaper;
        items.push(annotate(image.clone(), "image", active, wallpapers)?);
    }
    for video in videos.as_array().into_iter().flatten() {
        let active = video["id"].as_str() == theme.video_background.as_deref();
        items.push(annotate(video.clone(), "video", active, wallpapers)?);
    }
    let active = wallpaper_active(root)?;
    for video in wallpaper_catalogue(root)? {
        let is_active = video["id"].as_str() == active.as_deref();
        items.push(annotate(video, "wallpaper_video", is_active, &[])?);
    }
    Ok(json!({"items":items}))
}

pub(crate) fn list_available(
    root: &Path,
    theme: &ThemePreferences,
    remembered_wallpaper: Option<&str>,
) -> Result<Value> {
    let wallpapers = crate::desktop_wallpaper::current_wallpapers().unwrap_or_default();
    list(root, theme, remembered_wallpaper, &wallpapers)
        .or_else(|_| list(root, theme, remembered_wallpaper, &[]))
}

/// Resolve a typed ID to a validated managed object for a domain operation.
pub(crate) fn lookup(root: &Path, typed_id: &str) -> Result<Value> {
    let (kind, id) = typed_id
        .split_once(':')
        .ok_or_else(|| ApiError::new("invalid_background", "Invalid desktop asset ID"))?;
    let mut item = match kind {
        "image" => crate::theme_backgrounds::get(root, id)?,
        "video" => crate::video_backgrounds::get(root, id)
            .map_err(|e| ApiError::new("invalid_video", e))?,
        "wallpaper_video" => {
            let item = wallpaper_video(root, id)?;
            let path = PathBuf::from(
                item["path"]
                    .as_str()
                    .ok_or_else(|| ApiError::new("invalid_video", "Missing wallpaper path"))?,
            );
            if crate::preview::content_hash(&path)? != id {
                return Err(ApiError::new(
                    "stale_selection",
                    "Wallpaper video contents changed",
                ));
            }
            item
        }
        _ => {
            return Err(ApiError::new(
                "invalid_background",
                "Invalid desktop asset type",
            ));
        }
    };
    // Listing stays metadata-only; validate managed bytes when an action
    // resolves the asset, including direct ApplyDesktopAsset calls.
    if matches!(kind, "image" | "video") {
        let path = item["path"].as_str().ok_or_else(|| {
            ApiError::new("stale_selection", "Managed Desktop asset has no path")
        })?;
        if crate::preview::content_hash(Path::new(path))? != id {
            return Err(ApiError::new(
                "stale_selection",
                "Desktop asset contents changed",
            ));
        }
    }
    item["id"] = json!(typed_id);
    item["kind"] = json!(kind);
    Ok(item)
}

/// Delete an exact typed managed ID; caller must supply freshly queried wallpaper paths.
pub(crate) fn delete(
    root: &Path,
    typed_id: &str,
    theme: &ThemePreferences,
    remembered_wallpaper: Option<&str>,
    wallpapers: &[PathBuf],
) -> Result<Value> {
    let (kind, id) = typed_id
        .split_once(':')
        .ok_or_else(|| ApiError::new("invalid_background", "Invalid desktop asset ID"))?;
    match kind {
        "image" => {
            let item = crate::theme_backgrounds::get(root, id)?;
            let path =
                PathBuf::from(item["path"].as_str().ok_or_else(|| {
                    ApiError::new("invalid_background", "Missing background path")
                })?);
            if theme.custom_background.as_deref() == Some(id)
                || remembered_wallpaper == Some(id)
                || wallpaper_matches(&path, wallpapers)?
            {
                return Err(ApiError::new(
                    "background_in_use",
                    "Background is currently in use",
                ));
            }
            crate::theme_backgrounds::remove(root, id)?;
        }
        "video" => {
            if theme.video_background.as_deref() == Some(id) {
                return Err(ApiError::new(
                    "background_in_use",
                    "Background is currently in use",
                ));
            }
            crate::video_backgrounds::remove(root, id)
                .map_err(|e| ApiError::new("invalid_video", e))?;
        }
        "wallpaper_video" => {
            if wallpaper_active(root)?.as_deref() == Some(id) {
                return Err(ApiError::new(
                    "background_in_use",
                    "Wallpaper video was last applied to Wallpaper Engine",
                ));
            }
            let item = lookup(root, typed_id)?;
            let path = PathBuf::from(
                item["path"]
                    .as_str()
                    .ok_or_else(|| ApiError::new("invalid_video", "Missing wallpaper path"))?,
            );
            fs::remove_file(path)?;
            fs::remove_file(wallpaper_dir(root)?.join(format!("{id}.json")))?;
        }
        _ => {
            return Err(ApiError::new(
                "invalid_background",
                "Invalid desktop asset type",
            ));
        }
    }
    Ok(json!({"id":typed_id,"deleted":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_publication_keeps_existing_asset_and_receipt() {
        let root = tempfile::tempdir().unwrap();
        let staged = tempfile::tempdir().unwrap();
        let first = staged.path().join("first.mp4");
        let second = staged.path().join("second.mp4");
        fs::write(&first, b"first video").unwrap();
        fs::write(&second, b"second video").unwrap();
        let existing =
            register_wallpaper_video(root.path(), &first, Path::new("first.mp4")).unwrap();
        let previous = wallpaper_active(root.path()).unwrap();
        let dir = wallpaper_dir(root.path()).unwrap();
        let partial = dir.join(".katran-interrupted.tmp");
        fs::write(&partial, b"partial").unwrap();
        assert_eq!(wallpaper_catalogue(root.path()).unwrap().len(), 1);
        let id = crate::preview::content_hash(&second).unwrap();
        fs::write(dir.join(format!("{id}.mp4")), b"not the expected video").unwrap();
        assert!(register_wallpaper_video(root.path(), &second, Path::new("second.mp4")).is_err());
        assert_eq!(wallpaper_active(root.path()).unwrap(), previous);
        assert_eq!(
            lookup(root.path(), existing["id"].as_str().unwrap()).unwrap()["id"],
            existing["id"]
        );
        assert_eq!(
            fs::read(dir.join(format!("{id}.mp4"))).unwrap(),
            b"not the expected video"
        );
    }
    #[test]
    fn wallpaper_videos_keep_separate_identity_and_protect_last_applied() {
        let root = tempfile::tempdir().unwrap();
        let staged = tempfile::tempdir().unwrap();
        let first = staged.path().join("first.webm");
        let second = staged.path().join("second.mp4");
        fs::write(&first, b"video with audio").unwrap();
        fs::write(&second, b"different silent video").unwrap();
        let a = register_wallpaper_video(root.path(), &first, Path::new("original.webm")).unwrap();
        let a_id = a["id"].as_str().unwrap().to_owned();
        let event = crate::desktop_projection::list(root.path())
            .unwrap()
            .remove(0);
        assert_eq!(
            crate::desktop_projection::resolve(root.path(), event.id, &event.revision)
                .unwrap()
                .1,
            a_id
        );
        assert_eq!(
            delete(root.path(), &a_id, &ThemePreferences::default(), None, &[])
                .unwrap_err()
                .code,
            "background_in_use"
        );
        let b = register_wallpaper_video(root.path(), &second, Path::new("silent.mp4")).unwrap();
        assert_ne!(a_id, b["id"].as_str().unwrap());
        delete(root.path(), &a_id, &ThemePreferences::default(), None, &[]).unwrap();
        assert!(first.exists());
        let event = crate::desktop_projection::list(root.path())
            .unwrap()
            .remove(0);
        fs::write(&event.path, b"tampered").unwrap();
        let changed = crate::desktop_projection::list(root.path())
            .unwrap()
            .remove(0);
        assert_eq!(
            crate::desktop_projection::resolve(root.path(), changed.id, &changed.revision)
                .unwrap_err()
                .code,
            "stale_selection"
        );
        fs::remove_file(&event.path).unwrap();
        assert!(
            crate::desktop_projection::list(root.path())
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn video_is_listed_and_protected_until_no_longer_selected() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("video-backgrounds");
        fs::create_dir(&dir).unwrap();
        let id = "a".repeat(64);
        fs::write(dir.join(format!("{id}.mp4")), b"synthetic video").unwrap();
        fs::write(
            dir.join(format!("{id}.json")),
            json!({"id":id,"name":"sample","codec":"h264","width":10,"height":10,"duration":1.0})
                .to_string(),
        )
        .unwrap();
        let typed = format!("video:{id}");
        let mut theme = ThemePreferences::default();
        theme.video_background = Some(id);
        assert_eq!(
            list(root.path(), &theme, None, &[]).unwrap()["items"][0]["in_use"],
            true
        );
        assert_eq!(
            delete(root.path(), &typed, &theme, None, &[])
                .unwrap_err()
                .code,
            "background_in_use"
        );
        theme.video_background = None;
        delete(root.path(), &typed, &theme, None, &[]).unwrap();
        assert!(
            list(root.path(), &theme, None, &[]).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn managed_images_are_listed_and_in_use_images_are_protected() {
        let root = tempfile::tempdir().unwrap();
        let sources = tempfile::tempdir().unwrap();
        let original = sources.path().join("image.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([3, 5, 7, 255]))
            .save(&original)
            .unwrap();
        let image = crate::theme_backgrounds::import(root.path(), &original).unwrap();
        let id = image["id"].as_str().unwrap();
        let typed = format!("image:{id}");
        let path = PathBuf::from(image["path"].as_str().unwrap());
        let mut theme = ThemePreferences::default();
        assert_eq!(
            list(root.path(), &theme, None, &[]).unwrap()["items"][0]["id"],
            typed
        );
        theme.custom_background = Some(id.into());
        assert_eq!(
            delete(root.path(), &typed, &theme, None, &[])
                .unwrap_err()
                .code,
            "background_in_use"
        );
        theme.custom_background = None;
        assert_eq!(
            delete(root.path(), &typed, &theme, Some(id), &[])
                .unwrap_err()
                .code,
            "background_in_use"
        );
        assert_eq!(
            delete(root.path(), &typed, &theme, None, &[path])
                .unwrap_err()
                .code,
            "background_in_use"
        );
        assert!(delete(root.path(), "image:../image", &theme, None, &[]).is_err());
        delete(root.path(), &typed, &theme, None, &[]).unwrap();
        assert!(original.exists());
        assert!(
            list(root.path(), &theme, None, &[]).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}

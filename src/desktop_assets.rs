//! Unified catalogue of Katran-managed desktop background copies.
use crate::{ApiError, Result, theme::ThemePreferences};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

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
    let wallpaper = kind == "image" && wallpaper_matches(&path, wallpapers)?;
    item["id"] = json!(format!("{kind}:{id}"));
    item["kind"] = json!(kind);
    item["size"] = json!(fs::metadata(&path)?.len());
    item["in_use"] = json!(active || wallpaper);
    item["theme_active"] = json!(active);
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
        _ => {
            return Err(ApiError::new(
                "invalid_background",
                "Invalid desktop asset type",
            ));
        }
    };
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

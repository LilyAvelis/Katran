//! Bounded file visuals. Extraction never opens the file in its associated app.
use crate::{ApiError, Event, Result, preview};
use base64::Engine;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::UNIX_EPOCH};

const CACHE_BUDGET: u64 = 128 * 1024 * 1024;
const MAX_CACHE_ENTRY: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellVisual {
    Thumbnail,
    Icon,
}

/// Injectable OS boundary: providers return straight (not premultiplied) RGBA.
/// Calls may be slow; clients must run this API on bounded background workers.
pub trait Provider {
    fn extract(
        &self,
        path: &Path,
        edge: u32,
        kind: ShellVisual,
    ) -> std::result::Result<RgbaImage, String>;
}

pub struct NativeShell;
impl Provider for NativeShell {
    fn extract(
        &self,
        path: &Path,
        edge: u32,
        kind: ShellVisual,
    ) -> std::result::Result<RgbaImage, String> {
        if !(32..=512).contains(&edge) {
            return Err("invalid_visual_edge".into());
        }
        native::extract(path, edge, kind)
    }
}

#[derive(Serialize, Deserialize)]
struct Visual {
    width: u32,
    height: u32,
    revision: String,
    visual_kind: String,
    fallback_reasons: Vec<String>,
    rgba_base64: String,
    cache_hit: bool,
}

pub fn render(root: &Path, event: &Event, edge: u32) -> Result<serde_json::Value> {
    render_with_provider(root, event, edge, &NativeShell)
}

pub fn render_with_provider(
    root: &Path,
    event: &Event,
    edge: u32,
    provider: &dyn Provider,
) -> Result<serde_json::Value> {
    if !(32..=512).contains(&edge) {
        return Err(ApiError::new(
            "invalid_argument",
            "max_edge must be 32..512 for file visuals",
        ));
    }
    validate_source(event)?;
    // Preserve Katran's image decoder and image cache, including source checks.
    let mut reasons = Vec::new();
    match preview::render(root, event, edge) {
        Ok(mut image) => {
            image["visual_kind"] = "image".into();
            image["fallback_reasons"] = serde_json::json!([]);
            return Ok(image);
        }
        Err(error) if error.code == "source_changed" => return Err(error),
        Err(error) => reasons.push(format!("image: {}", error.code)),
    }
    let directory = root.join("file-visuals");
    fs::create_dir_all(&directory)?;
    if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        return Err(ApiError::new(
            "integrity_error",
            "File visual cache directory is a symbolic link",
        ));
    }
    // Path and name matter for Shell per-instance/association icons, even for blobs.
    let key = format!(
        "{:x}",
        Sha256::digest(format!(
            "v1:{}:{}:{}:{edge}",
            event.path.display(),
            event.name,
            event.revision
        ))
    );
    let cache = directory.join(format!("{key}.json"));
    if let Some(mut visual) = read_cache(&cache, event, edge) {
        validate_source(event)?;
        visual.cache_hit = true;
        return Ok(serde_json::to_value(visual).expect("visual serialization"));
    }
    let mut selected = None;
    for (kind, label) in [
        (ShellVisual::Thumbnail, "shell_thumbnail"),
        (ShellVisual::Icon, "shell_icon"),
    ] {
        match provider.extract(&event.path, edge, kind) {
            Ok(image)
                if image.width() > 0
                    && image.height() > 0
                    && image.width() <= edge
                    && image.height() <= edge =>
            {
                selected = Some((image, label));
                break;
            }
            Ok(_) => reasons.push(format!("{label}: invalid_dimensions")),
            Err(error) => reasons.push(format!(
                "{label}: {}",
                error.chars().take(256).collect::<String>()
            )),
        }
    }
    let (image, kind) = selected.unwrap_or_else(|| (generic_icon(edge.min(96)), "generic_icon"));
    validate_source(event)?;
    let visual = Visual {
        width: image.width(),
        height: image.height(),
        revision: event.revision.clone(),
        visual_kind: kind.into(),
        fallback_reasons: reasons,
        rgba_base64: base64::engine::general_purpose::STANDARD.encode(image.into_raw()),
        cache_hit: false,
    };
    // Best effort cache: inability to cache must not prevent a usable visual.
    if let Ok(bytes) = serde_json::to_vec(&visual) {
        let staging = directory.join(format!(
            "{key}-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        if fs::write(&staging, bytes).is_ok() {
            let _ = fs::rename(&staging, &cache);
        }
        let _ = fs::remove_file(staging);
    }
    trim_cache(&directory);
    Ok(serde_json::to_value(visual).expect("visual serialization"))
}

fn read_cache(path: &Path, event: &Event, edge: u32) -> Option<Visual> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_CACHE_ENTRY
    {
        return None;
    }
    let visual: Visual = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if visual.revision != event.revision
        || visual.width == 0
        || visual.height == 0
        || visual.width > edge
        || visual.height > edge
        || !matches!(
            visual.visual_kind.as_str(),
            "shell_thumbnail" | "shell_icon" | "generic_icon"
        )
    {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&visual.rgba_base64)
        .ok()?;
    (bytes.len() == visual.width as usize * visual.height as usize * 4).then_some(visual)
}

fn validate_source(event: &Event) -> Result<()> {
    if !event.path.is_file() {
        return Err(ApiError::new(
            "preview_unavailable",
            "Source file unavailable",
        ));
    }
    let current = if event.revision.starts_with("ref:") {
        format!("ref:{}", preview::content_hash(&event.path)?)
    } else {
        preview::revision(
            &event.path,
            if event.storage == "cached" {
                event.path.file_name().and_then(|s| s.to_str())
            } else {
                None
            },
        )
    };
    if current != event.revision {
        return Err(ApiError::new(
            "source_changed",
            "Source contents changed since this arrival",
        ));
    }
    Ok(())
}

fn trim_cache(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let m = fs::symlink_metadata(&path).ok()?;
            (m.is_file()
                && !m.file_type().is_symlink()
                && path.extension().is_some_and(|e| e == "json"))
            .then_some((path, m.len(), m.modified().ok()))
        })
        .collect();
    let mut size: u64 = files.iter().map(|e| e.1).sum();
    files.sort_by_key(|e| e.2);
    for (path, len, _) in files {
        if size <= CACHE_BUDGET {
            break;
        }
        if fs::remove_file(path).is_ok() {
            size = size.saturating_sub(len);
        }
    }
}

fn generic_icon(edge: u32) -> RgbaImage {
    let mut image = RgbaImage::new(edge, edge);
    for y in edge / 8..edge * 7 / 8 {
        for x in edge / 4..edge * 3 / 4 {
            let fold = x > edge / 2 && y < edge / 3 && x + edge / 3 > edge + y / 2;
            if !fold {
                image.put_pixel(x, y, image::Rgba([170, 185, 200, 255]));
            }
        }
    }
    for y in [edge / 2, edge * 5 / 8, edge * 3 / 4] {
        for x in edge / 3..edge * 2 / 3 {
            for dy in 0..(edge / 32).max(1) {
                image.put_pixel(x, y + dy, image::Rgba([70, 85, 100, 255]));
            }
        }
    }
    image
}

#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn extract(_: &Path, _: u32, _: ShellVisual) -> std::result::Result<RgbaImage, String> {
        Err("windows_shell_unavailable".into())
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{
            Foundation::{RPC_E_CHANGED_MODE, SIZE},
            Graphics::Gdi::*,
            System::Com::*,
            UI::Shell::*,
        },
        core::PCWSTR,
    };

    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }
    struct Bitmap(HBITMAP);
    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(self.0.into());
            }
        }
    }
    struct Dc(HDC);
    impl Drop for Dc {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteDC(self.0);
            }
        }
    }

    pub fn extract(
        path: &Path,
        edge: u32,
        kind: ShellVisual,
    ) -> std::result::Result<RgbaImage, String> {
        // Successful S_FALSE also owns a matching CoUninitialize; changed-mode
        // means the caller already initialized this thread and owns its lifetime.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_err() && hr != RPC_E_CHANGED_MODE {
            return Err(format!("com_initialization: {hr}"));
        }
        let _apartment = Apartment(hr.is_ok());
        let absolute = fs::canonicalize(path).map_err(|e| e.to_string())?;
        // Shell parsing names use ordinary drive/UNC paths, unlike Rust's
        // canonical verbatim paths (\\?\), which some Shell factories reject.
        let text = absolute.to_string_lossy();
        let shell_path = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
            std::path::PathBuf::from(format!("\\\\{unc}"))
        } else if let Some(drive) = text.strip_prefix("\\\\?\\") {
            std::path::PathBuf::from(drive)
        } else {
            absolute
        };
        let wide: Vec<u16> = shell_path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let factory: IShellItemImageFactory =
            unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
                .map_err(|e| format!("shell_item: {e}"))?;
        let flags = match kind {
            ShellVisual::Thumbnail => SIIGBF_THUMBNAILONLY,
            ShellVisual::Icon => SIIGBF_ICONONLY,
        };
        let bitmap = Bitmap(
            unsafe {
                factory.GetImage(
                    SIZE {
                        cx: edge as i32,
                        cy: edge as i32,
                    },
                    flags,
                )
            }
            .map_err(|e| format!("get_image: {e}"))?,
        );
        let mut info = BITMAP::default();
        if unsafe {
            GetObjectW(
                bitmap.0.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some((&mut info as *mut BITMAP).cast()),
            )
        } == 0
        {
            return Err("bitmap_metadata_unavailable".into());
        }
        let width = u32::try_from(info.bmWidth).map_err(|_| "invalid_bitmap_width")?;
        let height = info.bmHeight.unsigned_abs();
        // Reject hostile providers before allocating; no BIGGERSIZEOK requested.
        if width == 0 || height == 0 || width > edge || height > edge {
            return Err("invalid_bitmap_dimensions".into());
        }
        let dc = Dc(unsafe { CreateCompatibleDC(None) });
        if dc.0.is_invalid() {
            return Err("bitmap_dc_unavailable".into());
        }
        let mut dib = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bytes = vec![0u8; width as usize * height as usize * 4];
        if unsafe {
            GetDIBits(
                dc.0,
                bitmap.0,
                0,
                height,
                Some(bytes.as_mut_ptr().cast()),
                &mut dib,
                DIB_RGB_COLORS,
            )
        } != height as i32
        {
            return Err("bitmap_pixels_unavailable".into());
        }
        // Shell bitmaps use premultiplied BGRA. Some legacy opaque bitmaps have
        // an entirely zero alpha plane; preserve them as opaque rather than blank.
        let opaque = bytes.chunks_exact(4).all(|p| p[3] == 0);
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            if opaque {
                pixel[3] = 255;
            } else if pixel[3] > 0 {
                for c in 0..3 {
                    pixel[c] = (u32::from(pixel[c]) * 255 / u32::from(pixel[3])).min(255) as u8;
                }
            }
        }
        RgbaImage::from_raw(width, height, bytes).ok_or_else(|| "invalid_bitmap_buffer".into())
    }
}

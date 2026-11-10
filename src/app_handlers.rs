//! Associated application discovery and invocation. No executable path is a public capability.
use crate::{ApiError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const MAX_HANDLERS: usize = 64;
pub const ICON_EDGE: u32 = 32;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HandlerIcon {
    pub width: u32,
    pub height: u32,
    pub rgba_base64: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppHandler {
    pub id: String,
    pub name: String,
    pub recommended: bool,
    pub icon: Option<HandlerIcon>,
}
pub trait Provider: Send {
    fn discover(&self, extension: &str) -> Result<Vec<AppHandler>>;
    fn invoke(&self, extension: &str, handler_id: &str, path: &Path) -> Result<()>;
}
pub struct NativeProvider;
/// Extension comes from the recorded event name, never the content-addressed blob path.
pub fn extension(name: &str) -> Option<String> {
    let ext = Path::new(name).extension()?.to_str()?;
    if ext.is_empty()
        || ext.len() > 64
        || ext.chars().any(|c| c.is_control() || "\\/\0".contains(c))
    {
        return None;
    }
    Some(format!(".{}", ext.to_lowercase()))
}
fn handler_id(extension: &str, identity: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"katran-handler-v1\0");
    hash.update(extension.as_bytes());
    hash.update(b"\0");
    hash.update(identity.as_bytes());
    format!("{:x}", hash.finalize())
}

// Only decode the exact, bounded PNG explicitly supplied by Shell; never scan folders.
#[cfg(any(windows, test))]
fn png_icon(path: &Path) -> Option<HandlerIcon> {
    use base64::Engine;
    if !path.is_absolute() || !path.extension()?.eq_ignore_ascii_case("png") {
        return None;
    }
    if std::fs::metadata(path).ok()?.len() > 2 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::open(path).ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(512);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let icon = reader
        .decode()
        .ok()?
        .resize_exact(ICON_EDGE, ICON_EDGE, image::imageops::FilterType::Triangle)
        .into_rgba8();
    Some(HandlerIcon {
        width: ICON_EDGE,
        height: ICON_EDGE,
        rgba_base64: base64::engine::general_purpose::STANDARD.encode(icon.into_raw()),
    })
}

// Cache only rendered pixels: association membership and invocation remain fresh.
#[cfg(any(windows, test))]
#[derive(Default)]
struct IconCache {
    entries: std::collections::VecDeque<(String, i32, std::time::Instant, Option<HandlerIcon>)>,
}
#[cfg(any(windows, test))]
impl IconCache {
    const MAX_ENTRIES: usize = 128;
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    fn bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|(key, _, _, icon)| key.len() + icon.as_ref().map_or(0, |i| i.rgba_base64.len()))
            .sum()
    }
    fn get(
        &mut self,
        key: &str,
        index: i32,
        now: std::time::Instant,
    ) -> Option<Option<HandlerIcon>> {
        self.entries.retain(|(_, _, at, icon)| {
            now.saturating_duration_since(*at)
                < std::time::Duration::from_secs(if icon.is_some() { 10 } else { 2 })
        });
        self.entries
            .iter()
            .find(|(k, i, _, _)| k == key && *i == index)
            .map(|(_, _, _, icon)| icon.clone())
    }
    fn insert(
        &mut self,
        key: String,
        index: i32,
        icon: Option<HandlerIcon>,
        now: std::time::Instant,
    ) {
        self.entries.retain(|(k, i, _, _)| *k != key || *i != index);
        self.entries.push_back((key, index, now, icon));
        while self.entries.len() > Self::MAX_ENTRIES || self.bytes() > Self::MAX_BYTES {
            self.entries.pop_front();
        }
    }
}

#[cfg(not(windows))]
impl Provider for NativeProvider {
    fn discover(&self, _: &str) -> Result<Vec<AppHandler>> {
        Err(ApiError::new(
            "handlers_unavailable",
            "Windows Shell is unavailable",
        ))
    }
    fn invoke(&self, _: &str, _: &str, _: &Path) -> Result<()> {
        Err(ApiError::new(
            "handlers_unavailable",
            "Windows Shell is unavailable",
        ))
    }
}
#[cfg(windows)]
mod native {
    use super::*;
    use std::{collections::HashSet, os::windows::ffi::OsStrExt};
    use windows::{
        Win32::{
            System::Com::{
                COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                IDataObject,
            },
            UI::Shell::{
                ASSOC_FILTER_NONE, BHID_DataObject, IAssocHandler, IShellItem, SHAssocEnumHandlers,
                SHCreateItemFromParsingName,
            },
        },
        core::{PCWSTR, PWSTR},
    };
    struct Apartment;
    impl Apartment {
        fn new() -> Result<Self> {
            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                    .ok()
                    .map_err(|e| ApiError::new("handlers_unavailable", e))?
            };
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }
    fn err(e: impl std::fmt::Display) -> ApiError {
        ApiError::new("handlers_unavailable", e)
    }
    unsafe fn take_string(p: PWSTR) -> Option<String> {
        if p.is_null() {
            return None;
        }
        let value = unsafe { p.to_string().ok() };
        unsafe { CoTaskMemFree(Some(p.0.cast())) };
        value
    }
    fn valid_extension(ext: &str) -> bool {
        ext.starts_with('.') && extension(&format!("file{ext}")).as_deref() == Some(ext)
    }
    fn each(ext: &str, mut f: impl FnMut(IAssocHandler, String) -> Result<bool>) -> Result<()> {
        if !valid_extension(ext) {
            return Err(ApiError::new("invalid_argument", "Invalid file extension"));
        }
        let wide: Vec<u16> = ext.encode_utf16().chain(Some(0)).collect();
        let enumeration = unsafe { SHAssocEnumHandlers(PCWSTR(wide.as_ptr()), ASSOC_FILTER_NONE) }
            .map_err(err)?;
        let mut identities = HashSet::new();
        // Hard bound on enumeration calls, including malformed and duplicate entries.
        for _ in 0..MAX_HANDLERS {
            let mut handlers = [None];
            unsafe { enumeration.Next(&mut handlers, None) }.map_err(err)?;
            let Some(handler) = handlers[0].take() else {
                break;
            };
            let Some(identity) = (unsafe { handler.GetName().ok().and_then(|p| take_string(p)) })
            else {
                continue;
            };
            if identity.is_empty() || identity.len() > 32768 {
                continue;
            }
            let id = handler_id(ext, &identity);
            if identities.insert(id.clone()) && f(handler, id)? {
                break;
            }
        }
        Ok(())
    }
    impl Provider for NativeProvider {
        fn discover(&self, extension: &str) -> Result<Vec<AppHandler>> {
            if extension.is_empty() {
                return Ok(vec![]);
            }
            let _apartment = Apartment::new()?;
            let mut out = Vec::new();
            each(extension, |handler, id| {
                let name = unsafe { handler.GetUIName().ok().and_then(|p| take_string(p)) };
                if let Some(name) = name.filter(|s| !s.trim().is_empty()) {
                    let name: String = name.chars().filter(|c| !c.is_control()).take(256).collect();
                    if !name.is_empty() {
                        out.push(AppHandler {
                            id,
                            name,
                            recommended: unsafe { handler.IsRecommended().0 == 0 },
                            icon: unsafe { icon(&handler) },
                        });
                    }
                }
                Ok(false)
            })?;
            Ok(out)
        }
        fn invoke(&self, extension: &str, handler_id: &str, path: &Path) -> Result<()> {
            let _apartment = Apartment::new()?;
            let mut found = false;
            each(extension, |handler, id| {
                if id != handler_id {
                    return Ok(false);
                }
                let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
                let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
                let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
                if wide.starts_with(&unc) {
                    wide.splice(..unc.len(), [b'\\' as u16, b'\\' as u16]);
                } else if wide.starts_with(&prefix)
                    && wide.get(prefix.len() + 1) == Some(&(b':' as u16))
                {
                    wide.drain(..prefix.len());
                }
                if wide.contains(&0) {
                    return Err(ApiError::new(
                        "invalid_argument",
                        "Path contains a null character",
                    ));
                }
                wide.push(0);
                let item: IShellItem =
                    unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
                        .map_err(err)?;
                let data: IDataObject =
                    unsafe { item.BindToHandler(None, &BHID_DataObject) }.map_err(err)?;
                unsafe { handler.Invoke(&data) }.map_err(|e| ApiError::new("open_failed", e))?;
                found = true;
                Ok(true)
            })?;
            if found {
                Ok(())
            } else {
                Err(ApiError::new(
                    "handler_unavailable",
                    "Selected application is no longer associated with this file",
                ))
            }
        }
    }
    #[test]
    #[ignore = "read-only installed Windows handler timing; opt in explicitly"]
    fn native_discovery_latency() {
        use std::time::Instant;
        let _apartment = Apartment::new().unwrap();
        for ext in [".txt", ".png", ".pdf", ".mp4"] {
            for icons in [false, true] {
                let mut times = Vec::new();
                for _ in 0..12 {
                    let start = Instant::now();
                    each(ext, |handler, _| {
                        let _name =
                            unsafe { handler.GetUIName().ok().and_then(|p| take_string(p)) };
                        let _recommended = unsafe { handler.IsRecommended() };
                        if icons {
                            let _icon = unsafe { icon(&handler) };
                        }
                        Ok(false)
                    })
                    .unwrap();
                    times.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                println!("{ext} icons={icons} ms={times:?}");
            }
        }
    }
    unsafe fn icon(handler: &IAssocHandler) -> Option<HandlerIcon> {
        let mut location = PWSTR::null();
        let mut index = 0;
        unsafe { handler.GetIconLocation(&mut location, &mut index) }.ok()?;
        let text = unsafe { take_string(location) }?;
        // Exact Shell resource text and index are the identity; do not case-fold paths.
        static CACHE: std::sync::OnceLock<std::sync::Mutex<IconCache>> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let now = std::time::Instant::now();
        if let Ok(mut cache) = cache.lock()
            && let Some(icon) = cache.get(&text, index, now)
        {
            return icon;
        }
        let result = unsafe { resource_icon(&text, index) };
        if let Ok(mut cache) = cache.lock() {
            cache.insert(text, index, result.clone(), now);
        }
        result
    }
    unsafe fn resource_icon(text: &str, index: i32) -> Option<HandlerIcon> {
        use base64::Engine;
        use windows::Win32::UI::Shell::SHLoadIndirectString;
        use windows::Win32::{
            Graphics::Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
                DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
            },
            UI::{
                Shell::ExtractIconExW,
                WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON},
            },
        };
        let mut text = text.to_owned();
        if text.starts_with('@') {
            // Only accept an indirect string resolving to an absolute file path.
            // SHLoadIndirectString resolves text resources, not icon bytes.
            let source: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
            let mut output = vec![0u16; 32768];
            unsafe { SHLoadIndirectString(PCWSTR(source.as_ptr()), &mut output, None) }.ok()?;
            let end = output.iter().position(|&c| c == 0)?;
            text = String::from_utf16(&output[..end]).ok()?;
            if !Path::new(&text).is_absolute() {
                return None;
            }
        }
        if text.is_empty() {
            return None;
        }
        if index == 0
            && let Some(icon) = png_icon(Path::new(&text))
        {
            return Some(icon);
        }
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let mut hicon = HICON::default();
        if unsafe { ExtractIconExW(PCWSTR(wide.as_ptr()), index, Some(&mut hicon), None, 1) } == 0
            || hicon.is_invalid()
        {
            return None;
        }
        let result = (|| {
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.is_invalid() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: ICON_EDGE as i32,
                    biHeight: -(ICON_EDGE as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = match unsafe {
                CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            } {
                Ok(v) => v,
                Err(_) => {
                    unsafe {
                        let _ = DeleteDC(dc);
                    }
                    return None;
                }
            };
            if bits.is_null() {
                unsafe {
                    let _ = DeleteObject(bitmap.into());
                    let _ = DeleteDC(dc);
                }
                return None;
            }
            unsafe { std::ptr::write_bytes(bits, 0, (ICON_EDGE * ICON_EDGE * 4) as usize) };
            let old = unsafe { SelectObject(dc, bitmap.into()) };
            let drawn = unsafe {
                DrawIconEx(
                    dc,
                    0,
                    0,
                    hicon,
                    ICON_EDGE as i32,
                    ICON_EDGE as i32,
                    0,
                    None,
                    DI_NORMAL,
                )
            }
            .is_ok();
            let mut rgba = unsafe {
                std::slice::from_raw_parts(bits.cast::<u8>(), (ICON_EDGE * ICON_EDGE * 4) as usize)
            }
            .to_vec();
            unsafe {
                SelectObject(dc, old);
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(dc);
            }
            if !drawn || rgba.chunks_exact(4).all(|pixel| pixel[3] == 0) {
                return None;
            }
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                if pixel[3] > 0 {
                    for c in 0..3 {
                        pixel[c] = (u32::from(pixel[c]) * 255 / u32::from(pixel[3])).min(255) as u8;
                    }
                }
            }
            Some(HandlerIcon {
                width: ICON_EDGE,
                height: ICON_EDGE,
                rgba_base64: base64::engine::general_purpose::STANDARD.encode(rgba),
            })
        })();
        unsafe {
            let _ = DestroyIcon(hicon);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_cache_bounds_expiry_and_resource_identity() {
        use std::time::{Duration, Instant};
        let mut cache = IconCache::default();
        let now = Instant::now();
        let icon = HandlerIcon {
            width: 32,
            height: 32,
            rgba_base64: "pixels".into(),
        };
        cache.insert("resource".into(), 1, Some(icon.clone()), now);
        cache.insert("resource".into(), 2, None, now);
        assert!(cache.get("other", 1, now).is_none());
        assert!(cache.get("resource", 1, now).unwrap().is_some());
        assert!(cache.get("resource", 2, now).unwrap().is_none());
        assert!(
            cache
                .get("resource", 2, now + Duration::from_secs(2))
                .is_none()
        );
        assert!(
            cache
                .get("resource", 1, now + Duration::from_secs(9))
                .is_some()
        );
        assert!(
            cache
                .get("resource", 1, now + Duration::from_secs(10))
                .is_none()
        );
        for i in 0..200 {
            cache.insert(i.to_string(), 0, Some(icon.clone()), now);
        }
        assert_eq!(cache.entries.len(), IconCache::MAX_ENTRIES);
        assert!(cache.get("0", 0, now).is_none());
        let mut large = icon;
        large.rgba_base64 = "x".repeat(40000);
        for i in 0..128 {
            cache.insert(i.to_string(), 0, Some(large.clone()), now);
        }
        assert!(cache.bytes() <= IconCache::MAX_BYTES);
        cache.insert(
            "oversized".into(),
            0,
            Some(HandlerIcon {
                width: 1,
                height: 1,
                rgba_base64: "x".repeat(IconCache::MAX_BYTES + 1),
            }),
            now,
        );
        assert!(cache.bytes() <= IconCache::MAX_BYTES);
    }
    #[test]
    fn exact_png_icon_is_bounded_and_scaled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icon.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([25, 50, 75, 255]))
            .save(&path)
            .unwrap();
        let icon = png_icon(&path).unwrap();
        assert_eq!((icon.width, icon.height), (ICON_EDGE, ICON_EDGE));
        use base64::Engine;
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(icon.rgba_base64)
                .unwrap()
                .len(),
            (ICON_EDGE * ICON_EDGE * 4) as usize
        );
        assert!(png_icon(Path::new("relative.png")).is_none());
    }
    #[test]
    fn extension_and_ids_are_stable_and_separated() {
        assert_eq!(extension("тест.PnG").as_deref(), Some(".png"));
        assert_eq!(extension("no-extension"), None);
        assert_eq!(extension("x."), None);
        assert_eq!(
            handler_id(".png", "registered-app"),
            handler_id(".png", "registered-app")
        );
        assert_ne!(
            handler_id(".png", "registered-app"),
            handler_id(".txt", "registered-app")
        );
    }
}

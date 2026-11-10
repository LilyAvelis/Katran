//! One clipboard publication of table HTML plus Unicode TSV.
//! Successful publication confirms format placement, not how a target pastes it.
use crate::{ApiError, Result};
use serde::Serialize;

pub const MAX_FORMAT_BYTES: usize = 8 * 1024 * 1024;
pub const OWNER_FORMAT: &str = "Katran.TableClipboard.Owner.v1";
const MAX_OWNER_BYTES: usize = 128;
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub sequence: u32,
    pub owner_token: Option<String>,
    pub html_written: bool,
    pub text_written: bool,
    pub warnings: Vec<String>,
}
pub trait Sink: Send {
    fn write(&self, tsv: &str, cf_html: &[u8]) -> Result<Receipt>;
}
pub struct NativeClipboard;

/// Convert before opening/emptying the clipboard; never truncate an embedded NUL.
fn encode(tsv: &str, cf_html: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    if tsv.len() > MAX_FORMAT_BYTES || cf_html.is_empty() || cf_html.len() > MAX_FORMAT_BYTES {
        return Err(ApiError::new(
            "invalid_table",
            "Table clipboard format exceeds its size limit or HTML is empty",
        ));
    }
    if tsv.contains('\0') || cf_html.contains(&0) || std::str::from_utf8(cf_html).is_err() {
        return Err(ApiError::new(
            "invalid_table",
            "Table clipboard formats contain NUL or invalid UTF-8",
        ));
    }
    let units = tsv.encode_utf16().count();
    let size = units
        .checked_add(1)
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(|| ApiError::new("invalid_table", "Table text is too large"))?;
    if size > MAX_FORMAT_BYTES + 2 {
        return Err(ApiError::new(
            "invalid_table",
            "UTF-16 table text exceeds its size limit",
        ));
    }
    let mut text = Vec::with_capacity(size);
    for unit in tsv.encode_utf16().chain(Some(0)) {
        text.extend_from_slice(&unit.to_le_bytes());
    }
    let mut html = Vec::with_capacity(cf_html.len() + 1);
    html.extend_from_slice(cf_html);
    html.push(0);
    Ok((text, html))
}

// This function is called only after a successful EmptyClipboard: no error from
// a format write may erase the actual partial result or its sequence number.
trait Publication {
    fn html(&mut self) -> std::result::Result<(), String>;
    fn text(&mut self) -> std::result::Result<(), String>;
    fn marker(&mut self) -> std::result::Result<String, String>;
    fn sequence(&self) -> u32;
}
fn publish(backend: &mut impl Publication) -> Receipt {
    let mut warnings = Vec::new();
    let html_written = match backend.html() {
        Ok(()) => true,
        Err(error) => {
            warnings.push(format!("HTML clipboard publication failed: {error}"));
            false
        }
    };
    let text_written = match backend.text() {
        Ok(()) => true,
        Err(error) => {
            warnings.push(format!("Unicode clipboard publication failed: {error}"));
            false
        }
    };
    let owner_token = match backend.marker() {
        Ok(token) => Some(token),
        Err(error) => {
            warnings.push(format!("Clipboard ownership marker failed: {error}"));
            None
        }
    };
    Receipt {
        sequence: backend.sequence(),
        owner_token,
        html_written,
        text_written,
        warnings,
    }
}

fn decode_owner_token(bytes: &[u8]) -> Option<String> {
    if bytes.len() > MAX_OWNER_BYTES {
        return None;
    }
    let end = bytes.iter().position(|&b| b == 0)?;
    let token = &bytes[..end];
    if token.len() != 32
        || !token
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return None;
    }
    std::str::from_utf8(token).ok().map(str::to_owned)
}

/// Caller must hold the same open clipboard lock used to capture the payload.
#[cfg(windows)]
pub(crate) fn read_owner_token_locked() -> Result<Option<String>> {
    use windows_sys::Win32::System::{
        DataExchange::{GetClipboardData, IsClipboardFormatAvailable, RegisterClipboardFormatW},
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    };
    let name: Vec<u16> = OWNER_FORMAT.encode_utf16().chain(Some(0)).collect();
    let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    if format == 0 {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot register ownership marker format",
        ));
    }
    if unsafe { IsClipboardFormatAvailable(format) } == 0 {
        return Ok(None);
    }
    let handle = unsafe { GetClipboardData(format) };
    if handle.is_null() {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot read clipboard ownership marker",
        ));
    }
    let length = unsafe { GlobalSize(handle) };
    if length == 0 || length > MAX_OWNER_BYTES {
        return Ok(None);
    }
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        return Err(ApiError::new(
            "clipboard_error",
            "Cannot lock clipboard ownership marker",
        ));
    }
    let token =
        decode_owner_token(unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) });
    unsafe { GlobalUnlock(handle) };
    Ok(token)
}

#[cfg(not(windows))]
pub(crate) fn read_owner_token_locked() -> Result<Option<String>> {
    Ok(None)
}

#[cfg(not(windows))]
impl Sink for NativeClipboard {
    fn write(&self, _: &str, _: &[u8]) -> Result<Receipt> {
        Err(ApiError::new(
            "clipboard_error",
            "Windows clipboard adapter required",
        ))
    }
}

#[cfg(windows)]
mod windows_native {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
            System::{
                Com::CoCreateGuid,
                DataExchange::{
                    CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard,
                    RegisterClipboardFormatW, SetClipboardData,
                },
                LibraryLoader::GetModuleHandleW,
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, RegisterClassW,
                UnregisterClassW, WNDCLASSW,
            },
        },
        core::{PCWSTR, w},
    };
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
    };
    fn error(message: impl std::fmt::Display) -> ApiError {
        ApiError::new("clipboard_error", message)
    }
    struct Memory(*mut std::ffi::c_void);
    impl Memory {
        fn new(bytes: &[u8]) -> Result<Self> {
            let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
            if handle.is_null() {
                return Err(error("Cannot allocate clipboard data"));
            }
            let memory = Self(handle);
            let pointer = unsafe { GlobalLock(handle) };
            if pointer.is_null() {
                return Err(error("Cannot lock prepared clipboard data"));
            }
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
                GlobalUnlock(handle);
            }
            Ok(memory)
        }
        fn transfer(&mut self, format: u32) -> std::result::Result<(), String> {
            unsafe { SetClipboardData(format, Some(HANDLE(self.0))) }.map_err(|e| e.to_string())?;
            self.0 = std::ptr::null_mut();
            Ok(())
        }
    }
    impl Drop for Memory {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    GlobalFree(self.0);
                }
            }
        }
    }
    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }
    struct Window {
        hwnd: HWND,
        class: Vec<u16>,
        module: HINSTANCE,
    }
    impl Window {
        fn new() -> Result<Self> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let class: Vec<u16> = format!(
                "Katran.TableClipboard.{}.{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            let module = HINSTANCE(unsafe { GetModuleHandleW(None) }.map_err(error)?.0);
            let definition = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            if unsafe { RegisterClassW(&definition) } == 0 {
                return Err(error(windows::core::Error::from_thread()));
            }
            let hwnd = match unsafe {
                CreateWindowExW(
                    Default::default(),
                    PCWSTR(class.as_ptr()),
                    w!(""),
                    Default::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(module),
                    None,
                )
            } {
                Ok(hwnd) => hwnd,
                Err(e) => {
                    unsafe {
                        let _ = UnregisterClassW(PCWSTR(class.as_ptr()), Some(module));
                    }
                    return Err(error(e));
                }
            };
            Ok(Self {
                hwnd,
                class,
                module,
            })
        }
    }
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
                let _ = UnregisterClassW(PCWSTR(self.class.as_ptr()), Some(self.module));
            }
        }
    }
    struct Open(bool);
    impl Open {
        fn close(&mut self) -> std::result::Result<(), String> {
            if !self.0 {
                return Ok(());
            }
            unsafe { CloseClipboard() }.map_err(|e| e.to_string())?;
            self.0 = false;
            Ok(())
        }
    }
    impl Drop for Open {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }
    struct Formats {
        html: Memory,
        text: Memory,
        marker: Option<Memory>,
        marker_error: Option<String>,
        owner_token: String,
        marker_format: u32,
        html_format: u32,
    }
    impl Publication for Formats {
        fn html(&mut self) -> std::result::Result<(), String> {
            self.html.transfer(self.html_format)
        }
        fn text(&mut self) -> std::result::Result<(), String> {
            self.text.transfer(13)
        }
        fn marker(&mut self) -> std::result::Result<String, String> {
            let marker = self.marker.as_mut().ok_or_else(|| {
                self.marker_error
                    .clone()
                    .unwrap_or_else(|| "Marker unavailable".into())
            })?;
            marker.transfer(self.marker_format)?;
            Ok(self.owner_token.clone())
        }
        fn sequence(&self) -> u32 {
            unsafe { GetClipboardSequenceNumber() }
        }
    }
    impl Sink for NativeClipboard {
        fn write(&self, tsv: &str, cf_html: &[u8]) -> Result<Receipt> {
            let (text, html) = encode(tsv, cf_html)?;
            let html_format = unsafe { RegisterClipboardFormatW(w!("HTML Format")) };
            if html_format == 0 {
                return Err(error("Cannot register HTML clipboard format"));
            }
            let owner_token = format!(
                "{:032x}",
                unsafe { CoCreateGuid() }.map_err(error)?.to_u128()
            );
            let marker_name: Vec<u16> = OWNER_FORMAT.encode_utf16().chain(Some(0)).collect();
            let marker_format = unsafe { RegisterClipboardFormatW(PCWSTR(marker_name.as_ptr())) };
            let mut marker_bytes = owner_token.as_bytes().to_vec();
            marker_bytes.push(0);
            let (marker, marker_error) = if marker_format == 0 {
                (None, Some("Cannot register ownership format".into()))
            } else {
                match Memory::new(&marker_bytes) {
                    Ok(memory) => (Some(memory), None),
                    Err(error) => (None, Some(error.message)),
                }
            };
            let mut formats = Formats {
                html: Memory::new(&html)?,
                text: Memory::new(&text)?,
                marker,
                marker_error,
                owner_token,
                marker_format,
                html_format,
            };
            let window = Window::new()?;
            let start = Instant::now();
            loop {
                if unsafe { OpenClipboard(Some(window.hwnd)) }.is_ok() {
                    break;
                }
                if start.elapsed() >= Duration::from_millis(80) {
                    return Err(error("Clipboard busy"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let mut guard = Open(true);
            unsafe { EmptyClipboard() }.map_err(error)?;
            let mut receipt = publish(&mut formats);
            if let Err(error) = guard.close() {
                receipt
                    .warnings
                    .push(format!("Clipboard close failed: {error}"));
            }
            Ok(receipt)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        html_ok: bool,
        text_ok: bool,
        marker_ok: bool,
        calls: Vec<&'static str>,
    }
    impl Publication for Fake {
        fn html(&mut self) -> std::result::Result<(), String> {
            self.calls.push("html");
            if self.html_ok {
                Ok(())
            } else {
                Err("test".into())
            }
        }
        fn text(&mut self) -> std::result::Result<(), String> {
            self.calls.push("text");
            if self.text_ok {
                Ok(())
            } else {
                Err("test".into())
            }
        }
        fn marker(&mut self) -> std::result::Result<String, String> {
            self.calls.push("marker");
            if self.marker_ok {
                Ok("0123456789abcdef0123456789abcdef".into())
            } else {
                Err("test".into())
            }
        }
        fn sequence(&self) -> u32 {
            42
        }
    }
    #[test]
    fn encodes_both_formats_and_rejects_nul_invalid_and_excess() {
        let (text, html) = encode("a\t🐄\r\nb", b"Version:1.0\r\n").unwrap();
        assert_eq!(
            text,
            [
                "a\t🐄\r\nb"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
                vec![0, 0]
            ]
            .concat()
        );
        assert_eq!(html, b"Version:1.0\r\n\0");
        assert!(encode("a\0b", b"x").is_err());
        assert!(encode("a", b"x\0").is_err());
        assert!(encode("a", &[0xff]).is_err());
        assert!(encode("a", &vec![b'x'; MAX_FORMAT_BYTES + 1]).is_err());
    }
    #[test]
    fn token_is_exact_and_bounded_and_marker_failure_is_explicit() {
        let valid = b"0123456789abcdef0123456789abcdef\0";
        assert_eq!(
            decode_owner_token(valid).as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert!(decode_owner_token(&valid[..32]).is_none());
        assert!(decode_owner_token(b"ABC\0").is_none());
        assert!(decode_owner_token(&[0; MAX_OWNER_BYTES + 1]).is_none());
        let mut backend = Fake {
            html_ok: true,
            text_ok: true,
            marker_ok: false,
            calls: vec![],
        };
        let result = publish(&mut backend);
        assert!(result.owner_token.is_none());
        assert_eq!(result.sequence, 42);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(backend.calls, ["html", "text", "marker"]);
    }
    #[test]
    fn partial_publication_reports_each_format_and_sequence() {
        for (html_ok, text_ok) in [(true, true), (false, true), (true, false), (false, false)] {
            let mut fake = Fake {
                html_ok,
                text_ok,
                marker_ok: true,
                calls: vec![],
            };
            let result = publish(&mut fake);
            assert_eq!(fake.calls, ["html", "text", "marker"]);
            assert!(result.owner_token.is_some());
            assert_eq!(result.sequence, 42);
            assert_eq!(result.html_written, html_ok);
            assert_eq!(result.text_written, text_ok);
            assert_eq!(
                result.warnings.len(),
                usize::from(!html_ok) + usize::from(!text_ok)
            );
        }
    }
}

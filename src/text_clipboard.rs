//! Single guarded Unicode publication and sequence receipt.
use crate::{ApiError, Result};

#[cfg(any(windows, test))]
#[derive(Default)]
struct OwnedTokens(std::collections::VecDeque<String>);
#[cfg(any(windows, test))]
impl OwnedTokens {
    fn remember(&mut self, token: String) {
        self.0.push_back(token);
        if self.0.len() > 64 {
            self.0.pop_front();
        }
    }
    fn contains(&self, token: &str) -> bool {
        self.0.iter().any(|item| item == token)
    }
}
#[cfg(any(windows, test))]
static OWNED_TOKENS: std::sync::Mutex<OwnedTokens> =
    std::sync::Mutex::new(OwnedTokens(std::collections::VecDeque::new()));
#[cfg(any(windows, test))]
pub(super) fn owns_token(token: &str) -> bool {
    OWNED_TOKENS
        .lock()
        .is_ok_and(|tokens| tokens.contains(token))
}
#[cfg(test)]
pub(super) fn remember_token_for_test(token: &str) {
    OWNED_TOKENS.lock().unwrap().remember(token.to_owned());
}

#[cfg(any(windows, test))]
use sha2::{Digest, Sha256};
#[cfg(any(windows, test))]
#[derive(Clone, Debug)]
struct Receipt {
    sequence: u32,
    units: usize,
    digest: [u8; 32],
}
#[cfg(any(windows, test))]
fn receipt(sequence: u32, units: &[u16]) -> Receipt {
    let mut hash = Sha256::new();
    for unit in units {
        hash.update(unit.to_le_bytes());
    }
    Receipt {
        sequence,
        units: units.len(),
        digest: hash.finalize().into(),
    }
}
#[cfg(any(windows, test))]
fn matches_receipt(
    saved: Option<&Receipt>,
    expected: u32,
    before: u32,
    after: u32,
    formats: &[u32],
    units: &[u16],
) -> bool {
    let Some(saved) = saved else { return false };
    expected != 0
        && saved.sequence == expected
        && before != 0
        && before == after
        && formats.contains(&13)
        && formats.iter().all(|f| matches!(f, 1 | 7 | 13 | 16))
        && saved.units == units.len()
        && saved.digest == receipt(expected, units).digest
}
#[cfg(windows)]
static LATEST: std::sync::Mutex<Option<Receipt>> = std::sync::Mutex::new(None);

/// Backend owns the open Clipboard until dropped. Sequence is observed before
/// that guard is released, tying the receipt to this publication.
#[cfg(any(windows, test))]
trait Publication {
    fn publish(&mut self) -> Result<()>;
    fn sequence(&mut self) -> u32;
}
#[cfg(any(windows, test))]
fn publish_locked(mut backend: impl Publication) -> Result<u32> {
    backend.publish()?;
    Ok(backend.sequence())
}

#[cfg(not(windows))]
pub(super) fn write(_: &str) -> Result<u32> {
    Err(ApiError::new(
        "unsupported_platform",
        "Windows clipboard adapter required",
    ))
}

#[cfg(windows)]
pub(super) fn write(text: &str) -> Result<u32> {
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
    fn error(value: impl std::fmt::Display) -> ApiError {
        ApiError::new("clipboard_error", value)
    }
    if text.len() > super::MAX_MERGED_BYTES || text.contains('\0') {
        return Err(ApiError::new(
            "invalid_text",
            "Clipboard text exceeds its limit or contains NUL",
        ));
    }
    let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let size = units
        .len()
        .checked_mul(2)
        .ok_or_else(|| error("Clipboard text too large"))?;
    struct Memory(*mut std::ffi::c_void);
    impl Drop for Memory {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    GlobalFree(self.0);
                }
            }
        }
    }
    let memory = Memory(unsafe { GlobalAlloc(GMEM_MOVEABLE, size) });
    if memory.0.is_null() {
        return Err(error("Cannot allocate clipboard text"));
    }
    let pointer = unsafe { GlobalLock(memory.0) };
    if pointer.is_null() {
        return Err(error("Cannot lock clipboard text"));
    }
    unsafe {
        std::ptr::copy_nonoverlapping(units.as_ptr().cast::<u8>(), pointer.cast(), size);
        GlobalUnlock(memory.0);
    }
    let token = format!(
        "{:032x}",
        unsafe { CoCreateGuid() }.map_err(error)?.to_u128()
    );
    let marker_name: Vec<u16> = crate::table_clipboard::OWNER_FORMAT
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let marker_format = unsafe { RegisterClipboardFormatW(PCWSTR(marker_name.as_ptr())) };
    if marker_format == 0 {
        return Err(error("Cannot register clipboard ownership format"));
    }
    let marker_bytes = [token.as_bytes(), &[0]].concat();
    let marker = Memory(unsafe { GlobalAlloc(GMEM_MOVEABLE, marker_bytes.len()) });
    if marker.0.is_null() {
        return Err(error("Cannot allocate clipboard ownership marker"));
    }
    let marker_pointer = unsafe { GlobalLock(marker.0) };
    if marker_pointer.is_null() {
        return Err(error("Cannot lock clipboard ownership marker"));
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            marker_bytes.as_ptr(),
            marker_pointer.cast(),
            marker_bytes.len(),
        );
        GlobalUnlock(marker.0);
    }
    unsafe extern "system" fn procedure(
        hwnd: HWND,
        message: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wp, lp) }
    }
    struct Window {
        hwnd: HWND,
        class: Vec<u16>,
        module: HINSTANCE,
    }
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
                let _ = UnregisterClassW(PCWSTR(self.class.as_ptr()), Some(self.module));
            }
        }
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let class: Vec<u16> = format!(
        "Katran.TextClipboard.{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let module = HINSTANCE(unsafe { GetModuleHandleW(None) }.map_err(error)?.0);
    let definition = WNDCLASSW {
        lpfnWndProc: Some(procedure),
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
    let window = Window {
        hwnd,
        class,
        module,
    };
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
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }
    struct Backend {
        memory: Memory,
        marker: Memory,
        marker_format: u32,
        token: String,
        _open: Open,
        _window: Window,
    }
    impl Publication for Backend {
        fn publish(&mut self) -> Result<()> {
            unsafe { EmptyClipboard() }.map_err(error)?;
            unsafe { SetClipboardData(self.marker_format, Some(HANDLE(self.marker.0))) }
                .map_err(error)?;
            self.marker.0 = std::ptr::null_mut();
            unsafe { SetClipboardData(13, Some(HANDLE(self.memory.0))) }.map_err(error)?;
            // Clipboard now owns this allocation; never free it locally.
            self.memory.0 = std::ptr::null_mut();
            OWNED_TOKENS
                .lock()
                .map_err(|_| error("Ownership registry unavailable"))?
                .remember(self.token.clone());
            Ok(())
        }
        fn sequence(&mut self) -> u32 {
            unsafe { GetClipboardSequenceNumber() }
        }
    }
    // Serialize receipt updates with publication; never retain an old receipt if
    // this publication fails after modifying the clipboard.
    let mut latest = LATEST.lock().map_err(|_| error("Receipt unavailable"))?;
    *latest = None;
    let sequence = publish_locked(Backend {
        memory,
        marker,
        marker_format,
        token,
        _open: Open,
        _window: window,
    })?;
    if sequence != 0 {
        *latest = Some(receipt(sequence, &units[..units.len() - 1]));
    }
    Ok(sequence)
}

/// Returns the verified current sequence only for our latest plain-text receipt.
/// All native failures are deliberately indistinguishable from clipboard changes.
#[cfg(windows)]
pub(super) fn verified_sequence(expected: u32) -> Option<u32> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, SetLastError},
        System::{
            DataExchange::{
                CloseClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner,
                GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
            },
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        },
    };
    let latest = LATEST.lock().ok()?;
    let saved = latest.as_ref()?;
    if expected == 0 || saved.sequence != expected {
        return None;
    }
    if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
        return None;
    }
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    let _open = Open;
    let before = unsafe { GetClipboardSequenceNumber() };
    if before == 0 {
        return None;
    }
    // A live owner could supply delayed-rendered content on GetClipboardData.
    if !unsafe { GetClipboardOwner() }.is_null() {
        return None;
    }
    let marker_name: Vec<u16> = crate::table_clipboard::OWNER_FORMAT
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let marker_format = unsafe { RegisterClipboardFormatW(marker_name.as_ptr()) };
    if marker_format == 0 {
        return None;
    }
    let mut formats = Vec::new();
    let mut format = 0;
    loop {
        unsafe {
            SetLastError(0);
        }
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            if unsafe { GetLastError() } != 0 {
                return None;
            }
            break;
        }
        if format == marker_format {
            let token = crate::table_clipboard::read_owner_token_locked().ok()??;
            if !owns_token(&token) {
                return None;
            }
            continue;
        }
        if !matches!(format, 1 | 7 | 13 | 16) {
            return None;
        }
        formats.push(format);
    }
    if !formats.contains(&13) {
        return None;
    }
    let handle = unsafe { GetClipboardData(13) };
    if handle.is_null() {
        return None;
    }
    let size = unsafe { GlobalSize(handle) };
    let required = saved.units.checked_add(1)?.checked_mul(2)?;
    if size < required || size > super::MAX_MERGED_BYTES * 2 + 2 || size % 2 != 0 {
        return None;
    }
    let ptr = unsafe { GlobalLock(handle) };
    if ptr.is_null() {
        return None;
    }
    // Only inspect our expected bounded text and its NUL, never arbitrary extra bytes.
    let slice = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), saved.units + 1) };
    let valid = slice[saved.units] == 0
        && !slice[..saved.units].contains(&0)
        && matches_receipt(
            Some(saved),
            expected,
            before,
            unsafe { GetClipboardSequenceNumber() },
            &formats,
            &slice[..saved.units],
        );
    unsafe {
        GlobalUnlock(handle);
    }
    if valid { Some(before) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    struct Fake {
        calls: Rc<RefCell<Vec<&'static str>>>,
        fail: bool,
    }
    impl Publication for Fake {
        fn publish(&mut self) -> Result<()> {
            self.calls.borrow_mut().push("publish");
            if self.fail {
                Err(ApiError::new("clipboard_error", "synthetic failure"))
            } else {
                Ok(())
            }
        }
        fn sequence(&mut self) -> u32 {
            self.calls.borrow_mut().push("sequence");
            42
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.calls.borrow_mut().push("unlock");
        }
    }
    #[test]
    fn ownership_registry_is_bounded_and_requires_exact_token() {
        let mut tokens = OwnedTokens::default();
        for id in 0..65 {
            tokens.remember(format!("{id:032x}"));
        }
        assert!(!tokens.contains(&format!("{:032x}", 0)));
        assert!(tokens.contains(&format!("{:032x}", 64)));
        assert!(!tokens.contains("external"));
        assert_eq!(tokens.0.len(), 64);
    }
    #[test]
    fn changed_sequence_requires_exact_latest_plain_text_receipt() {
        let units: Vec<u16> = "A😀e\u{301}".encode_utf16().collect();
        let saved = receipt(42, &units);
        assert!(matches_receipt(
            Some(&saved),
            42,
            43,
            43,
            &[13, 1, 7, 16],
            &units
        ));
        assert!(!matches_receipt(None, 42, 43, 43, &[13], &units));
        assert!(!matches_receipt(Some(&saved), 41, 43, 43, &[13], &units));
        assert!(!matches_receipt(Some(&saved), 42, 0, 0, &[13], &units));
        assert!(!matches_receipt(Some(&saved), 42, 43, 44, &[13], &units));
        let different: Vec<u16> = "A😀é".encode_utf16().collect();
        assert!(!matches_receipt(
            Some(&saved),
            42,
            43,
            43,
            &[13],
            &different
        ));
        assert!(!matches_receipt(
            Some(&saved),
            42,
            43,
            43,
            &[13, 49300],
            &units
        ));
        assert!(!matches_receipt(
            Some(&saved),
            42,
            43,
            43,
            &[13, 15],
            &units
        ));
        assert!(!matches_receipt(Some(&saved), 42, 43, 43, &[1], &units));
    }
    #[test]
    fn receipt_is_read_before_unlock_and_failure_never_claims_success() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        assert_eq!(
            publish_locked(Fake {
                calls: calls.clone(),
                fail: false
            })
            .unwrap(),
            42
        );
        assert_eq!(*calls.borrow(), ["publish", "sequence", "unlock"]);
        calls.borrow_mut().clear();
        assert!(
            publish_locked(Fake {
                calls: calls.clone(),
                fail: true
            })
            .is_err()
        );
        assert_eq!(*calls.borrow(), ["publish", "unlock"]);
    }
}

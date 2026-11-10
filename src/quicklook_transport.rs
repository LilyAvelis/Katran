//! Versioned QuickLook bridge client. Never opens the legacy QuickLook pipe.
use serde_json::Value;
use std::time::Duration;

#[derive(Debug)]
pub struct TransportError {
    pub code: &'static str,
    pub message: String,
    /// A timeout after a write begins must never cause a Toggle replay.
    pub may_have_written: bool,
}
impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for TransportError {}
pub const MAX_FRAME: usize = 131_072;
fn error(code: &'static str, message: impl ToString, written: bool) -> TransportError {
    TransportError {
        code,
        message: message.to_string(),
        may_have_written: written,
    }
}
pub fn exchange(
    endpoint: &str,
    request: &Value,
    timeout: Duration,
) -> Result<Value, TransportError> {
    #[cfg(windows)]
    {
        native::exchange(endpoint, request, timeout)
    }
    #[cfg(not(windows))]
    {
        let _ = (endpoint, request, timeout);
        Err(error("unsupported_platform", "Windows required", false))
    }
}
pub fn endpoint() -> Result<String, TransportError> {
    #[cfg(windows)]
    {
        Ok(format!("QuickLook.KatranBridge.v1.{}", native::user_sid()?))
    }
    #[cfg(not(windows))]
    {
        Err(error("unsupported_platform", "Windows required", false))
    }
}
pub fn instance_exists() -> Result<bool, TransportError> {
    #[cfg(windows)]
    {
        native::instance_exists()
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}
pub fn process_image(pid: u32) -> Result<std::path::PathBuf, TransportError> {
    #[cfg(windows)]
    {
        native::process_image(pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(error("unsupported_platform", "Windows required", false))
    }
}
#[cfg(windows)]
mod native {
    use super::*;
    use std::{ffi::c_void, io, ptr, time::Instant};
    type Handle = *mut c_void;
    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: Handle,
    }
    // folder_watch declares the same Win32 functions with its private,
    // layout-identical OVERLAPPED type; opaque pointers preserve the same ABI.
    #[allow(clashing_extern_declarations)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
        fn ResetEvent(h: Handle) -> i32;
        fn GetNamedPipeServerProcessId(h: Handle, pid: *mut u32) -> i32;
        fn CreateEventW(
            security: *const c_void,
            manual: i32,
            state: i32,
            name: *const u16,
        ) -> Handle;
        fn ReadFile(
            h: Handle,
            buffer: *mut c_void,
            count: u32,
            done: *mut u32,
            ov: *mut c_void,
        ) -> i32;
        fn WriteFile(
            h: Handle,
            buffer: *const c_void,
            count: u32,
            done: *mut u32,
            ov: *mut c_void,
        ) -> i32;
        fn GetOverlappedResult(h: Handle, ov: *mut c_void, done: *mut u32, wait: i32) -> i32;
        fn CancelIoEx(h: Handle, ov: *mut c_void) -> i32;
        fn WaitForSingleObject(h: Handle, timeout: u32) -> u32;
        fn CloseHandle(h: Handle) -> i32;
        fn GetCurrentProcess() -> Handle;
        fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> Handle;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn QueryFullProcessImageNameW(h: Handle, flags: u32, name: *mut u16, len: *mut u32) -> i32;
        fn LocalFree(h: Handle) -> Handle;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: u32,
            info: *mut c_void,
            len: u32,
            needed: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *const c_void, output: *mut *mut u16) -> i32;
    }
    struct Owned(Handle);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn last(written: bool) -> TransportError {
        error(
            "quicklook_transport_failed",
            io::Error::last_os_error(),
            written,
        )
    }
    fn remaining(deadline: Instant) -> u32 {
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u32::MAX as u128) as u32
    }
    pub(super) fn user_sid() -> Result<String, TransportError> {
        unsafe {
            let mut token = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), 8, &mut token) == 0 {
                return Err(last(false));
            }
            let token = Owned(token);
            let mut n = 0;
            GetTokenInformation(token.0, 1, ptr::null_mut(), 0, &mut n);
            if n == 0 {
                return Err(last(false));
            }
            let mut buf = vec![0usize; (n as usize).div_ceil(std::mem::size_of::<usize>())];
            if GetTokenInformation(token.0, 1, buf.as_mut_ptr().cast(), n, &mut n) == 0 {
                return Err(last(false));
            }
            let sid = *(buf.as_ptr().cast::<*const c_void>());
            let mut text = ptr::null_mut();
            if ConvertSidToStringSidW(sid, &mut text) == 0 {
                return Err(last(false));
            }
            let mut n = 0;
            while *text.add(n) != 0 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
            LocalFree(text.cast());
            Ok(s)
        }
    }
    pub(super) fn instance_exists() -> Result<bool, TransportError> {
        unsafe {
            let h = OpenMutexW(0x00100000, 0, wide("QuickLook.App.Mutex").as_ptr());
            if !h.is_null() {
                drop(Owned(h));
                return Ok(true);
            }
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(2) {
                Ok(false)
            } else {
                Err(error("quicklook_instance_check_failed", e, false))
            }
        }
    }
    pub(super) fn process_image(pid: u32) -> Result<std::path::PathBuf, TransportError> {
        unsafe {
            let h = OpenProcess(0x1000, 0, pid);
            if h.is_null() {
                return Err(last(false));
            }
            let h = Owned(h);
            let mut buf = vec![0u16; 32768];
            let mut len = buf.len() as u32;
            if QueryFullProcessImageNameW(h.0, 0, buf.as_mut_ptr(), &mut len) == 0 {
                return Err(last(false));
            }
            Ok(std::path::PathBuf::from(String::from_utf16_lossy(
                &buf[..len as usize],
            )))
        }
    }
    fn transfer(
        h: Handle,
        buf: &mut [u8],
        write: bool,
        deadline: Instant,
        written: bool,
    ) -> Result<(), TransportError> {
        unsafe {
            let event = CreateEventW(ptr::null(), 1, 0, ptr::null());
            if event.is_null() {
                return Err(last(written));
            }
            let event = Owned(event);
            let mut offset = 0;
            while offset < buf.len() {
                if remaining(deadline) == 0 {
                    return Err(error(
                        "quicklook_timeout",
                        "Bridge deadline elapsed",
                        written,
                    ));
                }
                ResetEvent(event.0);
                let mut ov = Overlapped {
                    internal: 0,
                    internal_high: 0,
                    offset: 0,
                    offset_high: 0,
                    event: event.0,
                };
                let mut done = 0;
                let ov_ptr = (&mut ov as *mut Overlapped).cast();
                let ok = if write {
                    WriteFile(
                        h,
                        buf[offset..].as_ptr().cast(),
                        (buf.len() - offset) as u32,
                        &mut done,
                        ov_ptr,
                    )
                } else {
                    ReadFile(
                        h,
                        buf[offset..].as_mut_ptr().cast(),
                        (buf.len() - offset) as u32,
                        &mut done,
                        ov_ptr,
                    )
                };
                if ok == 0 {
                    if io::Error::last_os_error().raw_os_error() != Some(997) {
                        return Err(last(written));
                    }
                    if WaitForSingleObject(event.0, remaining(deadline)) != 0 {
                        CancelIoEx(h, ov_ptr);
                        GetOverlappedResult(h, ov_ptr, &mut done, 1);
                        return Err(error(
                            "quicklook_timeout",
                            "Bridge I/O deadline elapsed",
                            written,
                        ));
                    }
                    if GetOverlappedResult(h, ov_ptr, &mut done, 0) == 0 {
                        return Err(last(written));
                    }
                }
                if done == 0 {
                    return Err(error(
                        "quicklook_transport_failed",
                        "Unexpected EOF",
                        written,
                    ));
                }
                offset += done as usize;
            }
            Ok(())
        }
    }
    pub(super) fn exchange(
        endpoint: &str,
        request: &Value,
        timeout: Duration,
    ) -> Result<Value, TransportError> {
        if !endpoint.starts_with("QuickLook.KatranBridge.v1.")
            && !endpoint.starts_with("QuickLook.KatranBridge.Lab.v1.")
        {
            return Err(error(
                "invalid_argument",
                "Only versioned bridge endpoints are permitted",
                false,
            ));
        }
        let payload =
            serde_json::to_vec(request).map_err(|e| error("invalid_argument", e, false))?;
        if payload.is_empty() || payload.len() > MAX_FRAME {
            return Err(error(
                "invalid_argument",
                "Frame exceeds bridge limit",
                false,
            ));
        }
        let deadline = Instant::now() + timeout;
        let name = wide(&format!(r"\\.\pipe\{endpoint}"));
        // A tiny grace handles server-instance rearming without adding the
        // full I/O timeout to every stock-CLI compatibility request.
        let absent_grace = deadline.min(Instant::now() + Duration::from_millis(25));
        let mut seen_endpoint = false;
        let pipe = loop {
            unsafe {
                let h = CreateFileW(
                    name.as_ptr(),
                    0xc0000000,
                    0,
                    ptr::null(),
                    3,
                    0x40000000,
                    ptr::null_mut(),
                );
                if h as isize != -1 {
                    break Owned(h);
                }
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(2) && remaining(absent_grace) > 0 {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                if e.raw_os_error() == Some(2) {
                    return Err(error(
                        if seen_endpoint {
                            "quicklook_bridge_busy"
                        } else {
                            "quicklook_bridge_absent"
                        },
                        e,
                        false,
                    ));
                }
                if e.raw_os_error() != Some(231) {
                    return Err(error("quicklook_transport_failed", e, false));
                }
                seen_endpoint = true;
                let ms = remaining(deadline);
                if ms == 0 {
                    return Err(error(
                        "quicklook_bridge_busy",
                        "Connect deadline elapsed",
                        false,
                    ));
                }
                if WaitNamedPipeW(name.as_ptr(), ms) == 0 {
                    let e = io::Error::last_os_error();
                    if e.raw_os_error() == Some(2) && remaining(absent_grace) > 0 {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    return Err(error("quicklook_bridge_busy", e, false));
                }
            }
        };
        let mut frame = (payload.len() as u32).to_le_bytes().to_vec();
        frame.extend(payload);
        transfer(pipe.0, &mut frame, true, deadline, true)?;
        let mut header = [0u8; 4];
        transfer(pipe.0, &mut header, false, deadline, true)?;
        let n = u32::from_le_bytes(header) as usize;
        if n == 0 || n > MAX_FRAME {
            return Err(error(
                "quicklook_invalid_response",
                "Invalid response length",
                true,
            ));
        }
        let mut response = vec![0; n];
        transfer(pipe.0, &mut response, false, deadline, true)?;
        let result: Value = serde_json::from_slice(&response)
            .map_err(|e| error("quicklook_invalid_response", e, true))?;
        let mut server_pid = 0;
        if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut server_pid) } == 0 {
            return Err(last(true));
        }
        if result["pid"].as_u64() != Some(server_pid as u64) {
            return Err(error(
                "quicklook_identity_mismatch",
                "Bridge PID does not match pipe server",
                true,
            ));
        }
        Ok(result)
    }
}

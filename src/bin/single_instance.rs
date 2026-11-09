//! Launch-input adapter: one UI per canonical shelf root in this Windows session.
use std::{io, path::Path};

fn instance_key(root: &Path) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "LiliaAvelis.Katran.{:x}",
        Sha256::digest(root.to_string_lossy().to_lowercase().as_bytes())
    )
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::{
                CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM,
                WAIT_OBJECT_0,
            },
            System::Threading::{
                CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, SetEvent,
                WaitForSingleObject,
            },
            UI::WindowsAndMessaging::{
                AllowSetForegroundWindow, EnumWindows, GetPropW, GetWindowThreadProcessId,
                IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, SetPropW, ShowWindow,
            },
        },
        core::{BOOL, PCWSTR},
    };
    struct Handle(HANDLE);
    // Owned Win32 synchronization handles are valid across threads; only the
    // last owner closes them, after the listener exits.
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    fn error(e: windows::core::Error) -> io::Error {
        io::Error::other(e.to_string())
    }

    pub struct Primary {
        _gate: Handle,
        event: Arc<Handle>,
        marker: Vec<u16>,
        stop: Arc<AtomicBool>,
        listener: Option<thread::JoinHandle<()>>,
    }
    impl Drop for Primary {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            unsafe {
                let _ = SetEvent(self.event.0);
            }
            if let Some(listener) = self.listener.take() {
                let _ = listener.join();
            }
        }
    }
    impl Primary {
        pub fn bind(&self, window: u64) -> io::Result<()> {
            unsafe {
                SetPropW(
                    HWND(window as usize as *mut _),
                    PCWSTR(self.marker.as_ptr()),
                    Some(HANDLE(window as usize as *mut _)),
                )
                .map_err(error)
            }
        }
        pub fn listen(&mut self, wake: impl Fn() + Send + 'static) {
            if self.listener.is_some() {
                return;
            }
            let event = self.event.clone();
            let stop = self.stop.clone();
            self.listener = Some(thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    if unsafe { WaitForSingleObject(event.0, 200) } == WAIT_OBJECT_0
                        && !stop.load(Ordering::Acquire)
                    {
                        wake();
                    }
                }
            }));
        }
    }
    struct Search {
        marker: *const u16,
        window: HWND,
        pid: u32,
    }
    unsafe extern "system" fn find(window: HWND, data: LPARAM) -> BOOL {
        let search = unsafe { &mut *(data.0 as *mut Search) };
        if !unsafe { GetPropW(window, PCWSTR(search.marker)) }.is_invalid() {
            search.window = window;
            unsafe {
                GetWindowThreadProcessId(window, Some(&mut search.pid));
            }
            return BOOL(0);
        }
        BOOL(1)
    }
    fn notify_existing(key: &str) -> io::Result<()> {
        let marker = wide(key);
        let event_name = wide(&format!("Local\\{key}.Activate"));
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let mut search = Search {
                marker: marker.as_ptr(),
                window: HWND::default(),
                pid: 0,
            };
            unsafe {
                let _ = EnumWindows(Some(find), LPARAM((&mut search as *mut Search) as isize));
            }
            if search.pid != 0 {
                let event =
                    unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(event_name.as_ptr())) }
                        .map_err(error)?;
                let event = Handle(event);
                unsafe {
                    // The Raycast-launched process hands its foreground right to
                    // the original process before requesting activation.
                    let _ = AllowSetForegroundWindow(search.pid);
                    SetEvent(event.0).map_err(error)?;
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Katran is starting or unresponsive; no second shelf was opened",
                ));
            }
            thread::sleep(Duration::from_millis(40));
        }
    }
    pub fn acquire(root: &Path) -> io::Result<Option<Primary>> {
        std::fs::create_dir_all(root)?;
        let key = instance_key(&root.canonicalize()?);
        let gate_name = wide(&format!("Local\\{key}.Instance"));
        let (gate, exists) = unsafe {
            let handle = CreateMutexW(None, false, PCWSTR(gate_name.as_ptr())).map_err(error)?;
            (Handle(handle), GetLastError() == ERROR_ALREADY_EXISTS)
        };
        if exists {
            notify_existing(&key)?;
            return Ok(None);
        }
        let event_name = wide(&format!("Local\\{key}.Activate"));
        let event = unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) }
            .map_err(error)?;
        Ok(Some(Primary {
            _gate: gate,
            event: Arc::new(Handle(event)),
            marker: wide(&key),
            stop: Arc::new(AtomicBool::new(false)),
            listener: None,
        }))
    }
    pub fn show(window: u64) -> bool {
        unsafe {
            let window = HWND(window as usize as *mut _);
            let _ = ShowWindow(
                window,
                if IsIconic(window).as_bool() {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
            SetForegroundWindow(window).as_bool()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn synthetic_event_wakes_repeatedly_and_drop_releases_instance_gate() {
            let root = tempfile::tempdir().unwrap();
            let mut primary = acquire(root.path()).unwrap().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            primary.listen(move || {
                let _ = tx.send(());
            });
            for _ in 0..3 {
                unsafe {
                    SetEvent(primary.event.0).unwrap();
                }
                rx.recv_timeout(Duration::from_secs(1)).unwrap();
            }
            drop(primary);
            // An isolated synthetic root owns no window and never calls EnumWindows,
            // ShowWindow or SetForegroundWindow. Its kernel objects must be gone.
            let key = instance_key(&root.path().canonicalize().unwrap());
            let name = wide(&format!("Local\\{key}.Instance"));
            unsafe {
                let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr())).unwrap();
                let exists = GetLastError() == ERROR_ALREADY_EXISTS;
                let _handle = Handle(handle);
                assert!(!exists);
            }
        }
    }
}
#[cfg(windows)]
pub use native::{Primary, acquire, show};

#[cfg(not(windows))]
pub struct Primary;
#[cfg(not(windows))]
impl Primary {
    pub fn bind(&self, _: u64) -> io::Result<()> {
        Ok(())
    }
    pub fn listen(&mut self, _: impl Fn() + Send + 'static) {}
}
#[cfg(not(windows))]
pub fn acquire(_: &Path) -> io::Result<Option<Primary>> {
    Ok(Some(Primary))
}
#[cfg(not(windows))]
pub fn show(_: u64) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_is_stable_and_separate_for_each_shelf() {
        assert_eq!(
            instance_key(Path::new("C:/Shelf A")),
            instance_key(Path::new("c:/shelf a"))
        );
        assert_ne!(
            instance_key(Path::new("C:/Shelf A")),
            instance_key(Path::new("C:/Shelf B"))
        );
    }
}

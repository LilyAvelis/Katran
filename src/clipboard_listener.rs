//! Notification-only adapter. Never opens or reads the clipboard.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Default)]
pub struct Signal {
    generation: AtomicU64,
    failure: Mutex<Option<String>>,
    stopping: AtomicBool,
}
impl Signal {
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub fn take_failure(&self) -> Option<String> {
        self.failure.lock().unwrap().take()
    }
    fn notify(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{sync::mpsc, thread};
    use windows::{
        Win32::{
            Foundation::{
                ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
            },
            System::{
                DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener},
                LibraryLoader::GetModuleHandleW,
            },
            UI::WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
                GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE, MSG, PostMessageW,
                PostQuitMessage, RegisterClassW, SetWindowLongPtrW, WM_CLIPBOARDUPDATE, WM_CLOSE,
                WM_NCCREATE, WM_NCDESTROY, WNDCLASSW,
            },
        },
        core::{PCWSTR, w},
    };
    const CLASS: PCWSTR = w!("Katran.ClipboardEvents.MessageOnly");
    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        unsafe {
            if message == WM_NCCREATE {
                let create = &*(lp.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            if message == WM_CLIPBOARDUPDATE {
                let signal = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Signal;
                if let Some(signal) = signal.as_ref() {
                    signal.notify();
                }
                return LRESULT(0);
            }
            if message == WM_CLOSE {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            if message == WM_NCDESTROY {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            }
            DefWindowProcW(window, message, wp, lp)
        }
    }
    enum Mode {
        Native,
        #[cfg(test)]
        Synthetic,
        #[cfg(test)]
        Reject,
    }
    struct Host {
        window: HWND,
        subscribed: bool,
    }
    impl Host {
        fn close(&mut self) -> Result<(), String> {
            unsafe {
                let unregistered = if self.subscribed {
                    self.subscribed = false;
                    RemoveClipboardFormatListener(self.window).map_err(|e| e.to_string())
                } else {
                    Ok(())
                };
                let destroyed = DestroyWindow(self.window).map_err(|e| e.to_string());
                self.window = HWND::default();
                unregistered.and(destroyed)
            }
        }
    }
    impl Drop for Host {
        fn drop(&mut self) {
            if !self.window.0.is_null() {
                let _ = self.close();
            }
        }
    }
    pub struct Listener {
        window: usize,
        signal: Arc<Signal>,
        thread: Option<thread::JoinHandle<Result<(), String>>>,
        logger: Option<crate::diagnostics::Logger>,
    }
    impl Listener {
        pub fn start(
            wake: impl Fn() + Send + 'static,
            logger: Option<crate::diagnostics::Logger>,
        ) -> Result<Self, String> {
            Self::start_mode(wake, logger, Mode::Native)
        }
        fn start_mode(
            wake: impl Fn() + Send + 'static,
            logger: Option<crate::diagnostics::Logger>,
            mode: Mode,
        ) -> Result<Self, String> {
            let signal = Arc::new(Signal::default());
            let worker_signal = signal.clone();
            let worker_log = logger.clone();
            let (ready, receiver) = mpsc::sync_channel(1);
            let thread = thread::spawn(move || {
                let result = (|| -> Result<(), String> {
                    unsafe {
                        let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
                        // The class remains process-local and bounded to one;
                        // multiple isolated synthetic hosts may share it.
                        let class = WNDCLASSW {
                            lpfnWndProc: Some(procedure),
                            hInstance: HINSTANCE(module.0),
                            lpszClassName: CLASS,
                            ..Default::default()
                        };
                        if RegisterClassW(&class) == 0
                            && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
                        {
                            return Err(windows::core::Error::from_thread().to_string());
                        }
                        let window = CreateWindowExW(
                            Default::default(),
                            CLASS,
                            w!(""),
                            Default::default(),
                            0,
                            0,
                            0,
                            0,
                            Some(HWND_MESSAGE),
                            None,
                            Some(HINSTANCE(module.0)),
                            Some(Arc::as_ptr(&worker_signal).cast()),
                        )
                        .map_err(|e| e.to_string())?;
                        let mut host = Host {
                            window,
                            subscribed: false,
                        };
                        match mode {
                            Mode::Native => {
                                AddClipboardFormatListener(window).map_err(|e| e.to_string())?;
                                host.subscribed = true;
                            }
                            #[cfg(test)]
                            Mode::Synthetic => {}
                            #[cfg(test)]
                            Mode::Reject => return Err("synthetic registration failure".into()),
                        }
                        ready
                            .send(Ok(window.0 as usize))
                            .map_err(|e| e.to_string())?;
                        let mut seen = worker_signal.generation();
                        let mut message = MSG::default();
                        loop {
                            let result = GetMessageW(&mut message, None, 0, 0).0;
                            if result == -1 {
                                return Err(windows::core::Error::from_thread().to_string());
                            }
                            if result == 0 {
                                if !worker_signal.stopping.load(Ordering::Acquire) {
                                    return Err(
                                        "Clipboard notification loop stopped unexpectedly".into()
                                    );
                                }
                                break;
                            }
                            DispatchMessageW(&message);
                            let generation = worker_signal.generation();
                            if generation != seen {
                                seen = generation;
                                wake();
                            }
                        }
                        host.close()
                    }
                })();
                if let Err(error) = &result {
                    let _ = ready.send(Err(error.clone()));
                    *worker_signal.failure.lock().unwrap() = Some(error.clone());
                    if let Some(log) = &worker_log {
                        log.record(
                            "clipboard_listener_error",
                            serde_json::json!({"error":error}),
                        );
                    }
                    wake();
                }
                result
            });
            match receiver.recv().map_err(|e| e.to_string())? {
                Ok(window) => Ok(Self {
                    window,
                    signal,
                    thread: Some(thread),
                    logger,
                }),
                Err(error) => {
                    let _ = thread.join();
                    Err(error)
                }
            }
        }
        pub fn signal(&self) -> Arc<Signal> {
            self.signal.clone()
        }
    }
    impl Drop for Listener {
        fn drop(&mut self) {
            self.signal.stopping.store(true, Ordering::Release);
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(self.window as *mut _)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            if let Some(thread) = self.thread.take()
                && thread.join().is_err()
                && let Some(log) = &self.logger
            {
                log.record(
                    "clipboard_listener_error",
                    serde_json::json!({"error":"listener thread panicked"}),
                );
            }
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn synthetic_host_delivers_updates_and_tears_down_without_subscription() {
            let (tx, rx) = mpsc::channel();
            let listener = Listener::start_mode(
                move || {
                    let _ = tx.send(());
                },
                None,
                Mode::Synthetic,
            )
            .unwrap();
            let window = listener.window;
            for expected in 1..=3 {
                unsafe {
                    PostMessageW(
                        Some(HWND(window as *mut _)),
                        WM_CLIPBOARDUPDATE,
                        WPARAM(0),
                        LPARAM(0),
                    )
                    .unwrap();
                }
                rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
                assert_eq!(listener.signal().generation(), expected);
            }
            drop(listener);
            assert!(
                !unsafe {
                    windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(HWND(window as *mut _)))
                }
                .as_bool()
            );
        }
        #[test]
        fn failed_registration_is_reported_without_leaking_host() {
            let result = Listener::start_mode(|| {}, None, Mode::Reject);
            assert_eq!(result.err().unwrap(), "synthetic registration failure");
        }
        #[test]
        fn unexpected_loop_exit_is_reported_to_the_client() {
            let (tx, rx) = mpsc::channel();
            let listener = Listener::start_mode(
                move || {
                    let _ = tx.send(());
                },
                None,
                Mode::Synthetic,
            )
            .unwrap();
            unsafe {
                PostMessageW(
                    Some(HWND(listener.window as *mut _)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )
                .unwrap();
            }
            rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
            assert!(
                listener
                    .signal()
                    .take_failure()
                    .unwrap()
                    .contains("stopped unexpectedly")
            );
            drop(listener);
        }
    }
}
#[cfg(windows)]
pub use native::Listener;
#[cfg(not(windows))]
pub struct Listener;
#[cfg(not(windows))]
impl Listener {
    pub fn start(
        _: impl Fn() + Send + 'static,
        _: Option<crate::diagnostics::Logger>,
    ) -> Result<Self, String> {
        Err("Clipboard listener requires Windows".into())
    }
    pub fn signal(&self) -> Arc<Signal> {
        Arc::new(Signal::default())
    }
}

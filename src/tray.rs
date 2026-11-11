//! System-tray adapter; signals only, application policy remains in dispatcher/UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Toggle,
    Show,
    Hide,
    Quit,
}

/// Decode Shell's NOTIFYICON_VERSION_4 callback low word without side effects.
pub fn callback_event(message: u32) -> Option<Event> {
    match message {
        0x0400 | 0x0401 => Some(Event::Toggle),
        _ => None,
    }
}
/// Decode IDs returned by the tray menu; zero means it was dismissed.
pub fn menu_event(command: u32) -> Option<Event> {
    match command {
        1 => Some(Event::Show),
        2 => Some(Event::Hide),
        3 => Some(Event::Quit),
        _ => None,
    }
}

#[cfg(windows)]
mod native {
    use super::{Event, callback_event, menu_event};
    use std::{
        sync::mpsc::{self, Receiver, Sender, TryRecvError},
        thread::{self, JoinHandle},
    };
    use windows::{
        Win32::{
            Foundation::{
                ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
                WPARAM,
            },
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                Shell::{
                    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
                    NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONDATAW_0,
                    Shell_NotifyIconW,
                },
                WindowsAndMessaging::*,
            },
        },
        core::{PCWSTR, w},
    };
    const CLASS: PCWSTR = w!("Katran.Tray.Window");
    const CALLBACK: u32 = WM_APP + 34;
    struct State {
        sender: Sender<Result<Event, String>>,
        wake: Box<dyn Fn() + Send>,
        taskbar: u32,
        icon: HICON,
    }
    impl State {
        fn send(&self, event: Result<Event, String>) {
            if self.sender.send(event).is_ok() {
                (self.wake)();
            }
        }
        unsafe fn add(&self, window: HWND) -> Result<(), String> {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
                uCallbackMessage: CALLBACK,
                hIcon: self.icon,
                ..Default::default()
            };
            for (slot, unit) in data.szTip.iter_mut().zip("Katran".encode_utf16()) {
                *slot = unit;
            }
            if !unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
                return Err("Windows refused the Katran tray icon".into());
            }
            data.Anonymous = NOTIFYICONDATAW_0 {
                uVersion: NOTIFYICON_VERSION_4,
            };
            if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) }.as_bool() {
                unsafe {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                }
                return Err("Windows refused tray callback version 4".into());
            }
            Ok(())
        }
    }
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
            let state = (GetWindowLongPtrW(window, GWLP_USERDATA) as *const State).as_ref();
            if let Some(state) = state {
                if message == state.taskbar && state.taskbar != 0 {
                    if let Err(error) = state.add(window) {
                        state.send(Err(error));
                    }
                    return LRESULT(0);
                }
                if message == CALLBACK {
                    let signal = (lp.0 as u32) & 0xffff;
                    if let Some(event) = callback_event(signal) {
                        state.send(Ok(event));
                    } else if signal == WM_CONTEXTMENU && let Err(error) = show_menu(window, state) {
                        state.send(Err(error));
                    }
                    return LRESULT(0);
                }
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
    unsafe fn show_menu(window: HWND, state: &State) -> Result<(), String> {
        let menu = unsafe { CreatePopupMenu() }.map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), String> {
            unsafe {
                AppendMenuW(menu, MF_STRING, 1, w!("Показать")).map_err(|e| e.to_string())?;
                AppendMenuW(menu, MF_STRING, 2, w!("Скрыть")).map_err(|e| e.to_string())?;
                AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()).map_err(|e| e.to_string())?;
                AppendMenuW(menu, MF_STRING, 3, w!("Выйти")).map_err(|e| e.to_string())?;
                let mut point = POINT::default();
                GetCursorPos(&mut point).map_err(|e| e.to_string())?;
                let _ = SetForegroundWindow(window);
                let choice = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    None,
                    window,
                    None,
                )
                .0 as u32;
                let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
                if let Some(event) = menu_event(choice) {
                    state.send(Ok(event));
                }
            }
            Ok(())
        })();
        unsafe {
            let _ = DestroyMenu(menu);
        }
        result
    }
    pub struct Listener {
        window: usize,
        receiver: Receiver<Result<Event, String>>,
        worker: Option<JoinHandle<()>>,
    }
    impl Listener {
        pub fn start(wake: impl Fn() + Send + 'static) -> Result<Self, String> {
            let (events, receiver) = mpsc::channel();
            let (ready, start) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name("katran-system-tray".into())
                .spawn(move || {
                    let result = (|| -> Result<(), String> {
                        unsafe {
                            let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
                            let instance = HINSTANCE(module.0);
                            let class = WNDCLASSW {
                                lpfnWndProc: Some(procedure),
                                hInstance: instance,
                                lpszClassName: CLASS,
                                ..Default::default()
                            };
                            if RegisterClassW(&class) == 0
                                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
                            {
                                return Err(windows::core::Error::from_thread().to_string());
                            }
                            let icon = LoadIconW(Some(instance), PCWSTR(101usize as *const u16))
                                .map_err(|e| e.to_string())?;
                            let taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
                            if taskbar == 0 {
                                return Err("Cannot register TaskbarCreated".into());
                            }
                            let state = Box::new(State {
                                sender: events.clone(),
                                wake: Box::new(wake),
                                taskbar,
                                icon,
                            });
                            // A hidden top-level window receives Explorer's broadcast;
                            // a message-only window would not receive TaskbarCreated.
                            let window = CreateWindowExW(
                                Default::default(),
                                CLASS,
                                w!(""),
                                Default::default(),
                                0,
                                0,
                                0,
                                0,
                                None,
                                None,
                                Some(instance),
                                Some((&*state as *const State).cast()),
                            )
                            .map_err(|e| e.to_string())?;
                            if let Err(error) = state.add(window) {
                                let _ = DestroyWindow(window);
                                return Err(error);
                            }
                            if ready.send(Ok(window.0 as usize)).is_err() {
                                let data = NOTIFYICONDATAW {
                                    cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                                    hWnd: window,
                                    uID: 1,
                                    ..Default::default()
                                };
                                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                                let _ = DestroyWindow(window);
                                return Err("Tray startup receiver closed".into());
                            }
                            let mut message = MSG::default();
                            let result = loop {
                                let value = GetMessageW(&mut message, None, 0, 0).0;
                                if value == -1 {
                                    break Err(windows::core::Error::from_thread().to_string());
                                }
                                if value == 0 {
                                    break Ok(());
                                }
                                DispatchMessageW(&message);
                            };
                            let data = NOTIFYICONDATAW {
                                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                                hWnd: window,
                                uID: 1,
                                ..Default::default()
                            };
                            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                            let _ = DestroyWindow(window);
                            if let Err(error) = &result {
                                state.send(Err(error.clone()));
                            }
                            result
                        }
                    })();
                    if let Err(error) = result {
                        let _ = ready.send(Err(error.clone()));
                        let _ = events.send(Err(error));
                    }
                })
                .map_err(|e| e.to_string())?;
            match start.recv().map_err(|e| e.to_string())? {
                Ok(window) => Ok(Self {
                    window,
                    receiver,
                    worker: Some(worker),
                }),
                Err(error) => {
                    let _ = worker.join();
                    Err(error)
                }
            }
        }
        pub fn try_recv(&self) -> Result<Option<Event>, String> {
            match self.receiver.try_recv() {
                Ok(Ok(event)) => Ok(Some(event)),
                Ok(Err(error)) => Err(error),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Err("Tray listener stopped".into()),
            }
        }
    }
    impl Drop for Listener {
        fn drop(&mut self) {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(self.window as *mut _)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}
#[cfg(windows)]
pub use native::Listener;

#[cfg(not(windows))]
pub struct Listener;
#[cfg(not(windows))]
impl Listener {
    pub fn start(_: impl Fn() + Send + 'static) -> Result<Self, String> {
        Err("System tray requires Windows".into())
    }
    pub fn try_recv(&self) -> Result<Option<Event>, String> {
        Err("System tray requires Windows".into())
    }
}

//! Native Windows host and thread-owned global hotkey. No Clipboard handling.
#[cfg(windows)]
mod native {
    use crate::overlay::{Host, Hotkey, OverlayError, Result, SummonReceipt, Target, WorkArea};
    use std::{
        sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };
    use windows::Win32::{
        Foundation::{CloseHandle, FILETIME, HWND, LPARAM, WPARAM},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        System::Threading::{
            GetCurrentThreadId, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager},
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };
    fn hwnd(value: u64) -> HWND {
        HWND(value as usize as *mut _)
    }
    fn fail(code: &str, error: impl ToString) -> OverlayError {
        OverlayError::new(code, error)
    }
    pub struct WindowsHost;
    impl Host for WindowsHost {
        fn foreground(&mut self) -> Result<u64> {
            let h = unsafe { GetForegroundWindow() };
            if h.0.is_null() {
                Err(fail("foreground_unavailable", "No foreground window"))
            } else {
                Ok(h.0 as usize as u64)
            }
        }
        fn identity(&mut self, window: u64) -> Result<Target> {
            let h = hwnd(window);
            if window == 0 || !unsafe { IsWindow(Some(h)) }.as_bool() {
                return Err(fail("target_unavailable", "Window no longer exists"));
            }
            let mut pid = 0;
            if unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) } == 0 || pid == 0 {
                return Err(fail("target_unavailable", "Cannot identify window process"));
            }
            let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
                .map_err(|e| fail("target_unavailable", e))?;
            let mut created = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            let result = unsafe {
                GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user)
            };
            unsafe {
                let _ = CloseHandle(process);
            }
            result.map_err(|e| fail("target_unavailable", e))?;
            if !unsafe { IsWindow(Some(h)) }.as_bool() {
                return Err(fail("target_unavailable", "Window closed"));
            }
            let mut current = 0;
            unsafe {
                GetWindowThreadProcessId(h, Some(&mut current));
            }
            if current != pid {
                return Err(fail("target_changed", "Window process changed"));
            }
            Ok(Target {
                hwnd: window,
                pid,
                process_started: (u64::from(created.dwHighDateTime) << 32)
                    | u64::from(created.dwLowDateTime),
            })
        }
        fn work_area(&mut self, window: u64) -> Result<WorkArea> {
            let monitor = unsafe { MonitorFromWindow(hwnd(window), MONITOR_DEFAULTTONEAREST) };
            if monitor.0.is_null() {
                return Err(fail(
                    "monitor_unavailable",
                    "No monitor for foreground window",
                ));
            }
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                return Err(fail("monitor_unavailable", std::io::Error::last_os_error()));
            }
            Ok(WorkArea {
                left: info.rcWork.left,
                top: info.rcWork.top,
                right: info.rcWork.right,
                bottom: info.rcWork.bottom,
            })
        }
        fn wait_modifiers_released(&mut self) -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let held = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
                    .iter()
                    .any(|key| {
                        (unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16 & 0x8000) != 0
                    });
                if !held {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(fail(
                        "modifiers_held",
                        "Release modifier keys before pasting",
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        fn activate(&mut self, window: u64) -> Result<()> {
            let h = hwnd(window);
            if unsafe { IsIconic(h) }.as_bool() {
                unsafe {
                    let _ = ShowWindow(h, SW_RESTORE);
                }
            }
            if !unsafe { SetForegroundWindow(h) }.as_bool() {
                return Err(fail(
                    "activation_denied",
                    "Windows refused to activate the previous window",
                ));
            }
            Ok(())
        }
        fn send_paste(&mut self, expected: Target, expected_sequence: u32) -> Result<()> {
            if self.foreground()? != expected.hwnd || self.identity(expected.hwnd)? != expected {
                return Err(fail(
                    "foreground_changed",
                    "Previous application lost foreground",
                ));
            }
            if expected_sequence == 0 || crate::capture::sequence_number() != expected_sequence {
                return Err(fail(
                    "clipboard_changed",
                    "Clipboard changed before pasting",
                ));
            }
            let make = |key, flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: key,
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            };
            let inputs = [
                make(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
                make(VIRTUAL_KEY(b'V' as u16), KEYBD_EVENT_FLAGS(0)),
                make(VIRTUAL_KEY(b'V' as u16), KEYEVENTF_KEYUP),
                make(VK_CONTROL, KEYEVENTF_KEYUP),
            ];
            let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
            if sent != inputs.len() as u32 {
                // Best effort to release keys already injected; never retry paste.
                let release = [
                    make(VIRTUAL_KEY(b'V' as u16), KEYEVENTF_KEYUP),
                    make(VK_CONTROL, KEYEVENTF_KEYUP),
                ];
                if sent > 0 {
                    unsafe {
                        SendInput(&release, std::mem::size_of::<INPUT>() as i32);
                    }
                }
                return Err(fail(
                    "input_blocked",
                    format!(
                        "Windows accepted {sent}/4 input events; target integrity/UIPI may block input"
                    ),
                ));
            }
            Ok(())
        }
    }

    /// Move Katran's own window to the target's virtual desktop, center it on
    /// the target monitor and request foreground. Unsupported/denied desktop
    /// transitions are returned explicitly; this does not switch desktops.
    pub fn summon_window(own_window: u64, previous: Target, area: WorkArea) -> Result<()> {
        let mut host = WindowsHost;
        let own = host.identity(own_window)?;
        if own.pid != std::process::id()
            || previous.pid == own.pid
            || host.identity(previous.hwnd)? != previous
        {
            return Err(fail(
                "target_changed",
                "Window ownership or previous target changed",
            ));
        }
        let foreground = host.foreground()?;
        if foreground != previous.hwnd && foreground != own_window {
            return Err(fail(
                "foreground_changed",
                "Previous application lost foreground",
            ));
        }
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if initialized.is_err() && initialized.0 != 0x80010106u32 as i32 {
                return Err(fail("virtual_desktop_unavailable", initialized.to_string()));
            }
            let moved = (|| -> windows::core::Result<()> {
                let manager: IVirtualDesktopManager =
                    CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_INPROC_SERVER)?;
                // The captured foreground is on the current desktop. Avoid
                // asking for an own-window desktop ID during hidden-window
                // transitions when no move is needed (Shell may report
                // TYPE_E_ELEMENTNOTFOUND for that redundant lookup).
                if !manager
                    .IsWindowOnCurrentVirtualDesktop(hwnd(own_window))?
                    .as_bool()
                {
                    let desktop = manager.GetWindowDesktopId(hwnd(previous.hwnd))?;
                    manager.MoveWindowToDesktop(hwnd(own_window), &desktop)?;
                }
                Ok(())
            })();
            if initialized.is_ok() {
                CoUninitialize();
            }
            moved.map_err(|e| fail("virtual_desktop_unavailable", e))?;
            if IsIconic(hwnd(own_window)).as_bool() {
                let _ = ShowWindow(hwnd(own_window), SW_RESTORE);
            }
            let mut rect = windows::Win32::Foundation::RECT::default();
            GetWindowRect(hwnd(own_window), &mut rect)
                .map_err(|e| fail("window_unavailable", e))?;
            let width = (rect.right - rect.left)
                .max(1)
                .min((area.right - area.left).max(1));
            let height = (rect.bottom - rect.top)
                .max(1)
                .min((area.bottom - area.top).max(1));
            let x = area.left + ((area.right - area.left) - width) / 2;
            let y = area.top + ((area.bottom - area.top) - height) / 2;
            SetWindowPos(
                hwnd(own_window),
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_SHOWWINDOW,
            )
            .map_err(|e| fail("window_unavailable", e))?;
            SetWindowPos(
                hwnd(own_window),
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .map_err(|e| fail("window_unavailable", e))?;
            if !SetForegroundWindow(hwnd(own_window)).as_bool() {
                return Err(fail(
                    "activation_denied",
                    "Windows refused to activate Katran",
                ));
            }
            if GetForegroundWindow() != hwnd(own_window) {
                return Err(fail(
                    "activation_denied",
                    "Katran did not become foreground",
                ));
            }
        }
        Ok(())
    }

    /// Registration and unregistering happen on the same dedicated message thread.
    pub struct HotkeyListener {
        receiver: Receiver<Result<SummonReceipt>>,
        thread_id: u32,
        worker: Option<JoinHandle<()>>,
    }
    impl HotkeyListener {
        pub fn start(hotkey: Hotkey, wake: impl Fn() + Send + 'static) -> Result<Self> {
            if !(hotkey.ctrl || hotkey.alt || hotkey.shift)
                || !(1..=255).contains(&hotkey.key)
                || matches!(hotkey.key, 0x10..=0x12 | 0x5b | 0x5c | 0xa0..=0xa5)
            {
                return Err(fail(
                    "invalid_hotkey",
                    "Expected Ctrl, Alt or Shift and a non-modifier key",
                ));
            }
            let (sender, receiver) = mpsc::sync_channel(1);
            let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<u32>>(1);
            let worker = thread::Builder::new()
                .name("katran-global-hotkey".into())
                .spawn(move || hotkey_loop(hotkey, sender, ready_tx, wake))
                .map_err(|e| fail("hotkey_unavailable", e))?;
            match ready_rx.recv() {
                Ok(Ok(thread_id)) => Ok(Self {
                    receiver,
                    thread_id,
                    worker: Some(worker),
                }),
                Ok(Err(error)) => {
                    let _ = worker.join();
                    Err(error)
                }
                Err(error) => {
                    let _ = worker.join();
                    Err(fail("hotkey_unavailable", error))
                }
            }
        }
        pub fn try_recv(&self) -> Result<bool> {
            self.try_recv_capture()?
                .transpose()
                .map(|item| item.is_some())
        }
        /// Outer errors indicate listener failure; inner errors belong to one
        /// hotkey event and must not disable an otherwise registered listener.
        pub fn try_recv_capture(&self) -> Result<Option<Result<SummonReceipt>>> {
            match self.receiver.try_recv() {
                Ok(capture) => Ok(Some(capture)),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => {
                    Err(fail("hotkey_stopped", "Global hotkey listener stopped"))
                }
            }
        }
    }
    fn hotkey_loop(
        hotkey: Hotkey,
        sender: SyncSender<Result<SummonReceipt>>,
        ready: SyncSender<Result<u32>>,
        wake: impl Fn(),
    ) {
        let mut flags = MOD_NOREPEAT;
        if hotkey.ctrl {
            flags |= MOD_CONTROL;
        }
        if hotkey.alt {
            flags |= MOD_ALT;
        }
        if hotkey.shift {
            flags |= MOD_SHIFT;
        }
        if let Err(error) = unsafe { RegisterHotKey(None, 1, flags, hotkey.key) } {
            let _ = ready.send(Err(fail("hotkey_registration_failed", error)));
            return;
        }
        let thread_id = unsafe { GetCurrentThreadId() };
        // Establish the queue before Drop can post WM_QUIT after ready.
        let mut message = MSG::default();
        unsafe {
            let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        }
        if ready.send(Ok(thread_id)).is_err() {
            unsafe {
                let _ = UnregisterHotKey(None, 1);
            }
            return;
        }
        loop {
            let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
            if status <= 0 {
                break;
            }
            if message.message == WM_HOTKEY && message.wParam.0 == 1 {
                let capture = (|| {
                    let mut host = WindowsHost;
                    let window = host.foreground()?;
                    let previous = host.identity(window)?;
                    if previous.pid == std::process::id() {
                        return Err(fail("self_target", "Katran is already foreground"));
                    }
                    let work_area = host.work_area(window)?;
                    if host.foreground()? != window || host.identity(window)? != previous {
                        return Err(fail(
                            "foreground_changed",
                            "Foreground changed during hotkey capture",
                        ));
                    }
                    Ok(SummonReceipt {
                        previous,
                        work_area,
                    })
                })();
                if sender.try_send(capture).is_ok() {
                    wake();
                }
            }
        }
        unsafe {
            let _ = UnregisterHotKey(None, 1);
        }
    }
    impl Drop for HotkeyListener {
        fn drop(&mut self) {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}
#[cfg(windows)]
pub use native::{HotkeyListener, WindowsHost, summon_window};

#[cfg(not(windows))]
pub struct WindowsHost;
#[cfg(not(windows))]
impl crate::overlay::Host for WindowsHost {
    fn foreground(&mut self) -> crate::overlay::Result<u64> {
        Err(crate::overlay::OverlayError::new(
            "unsupported_platform",
            "Overlay requires Windows",
        ))
    }
    fn identity(&mut self, _: u64) -> crate::overlay::Result<crate::overlay::Target> {
        Err(crate::overlay::OverlayError::new(
            "unsupported_platform",
            "Overlay requires Windows",
        ))
    }
    fn work_area(&mut self, _: u64) -> crate::overlay::Result<crate::overlay::WorkArea> {
        Err(crate::overlay::OverlayError::new(
            "unsupported_platform",
            "Overlay requires Windows",
        ))
    }
    fn wait_modifiers_released(&mut self) -> crate::overlay::Result<()> {
        Err(crate::overlay::OverlayError::new(
            "unsupported_platform",
            "Overlay requires Windows",
        ))
    }
    fn activate(&mut self, _: u64) -> crate::overlay::Result<()> {
        Err(crate::overlay::OverlayError::new(
            "unsupported_platform",
            "Overlay requires Windows",
        ))
    }
    fn send_paste(&mut self, _: crate::overlay::Target, _: u32) -> crate::overlay::Result<()> {
        Err(crate::overlay::OverlayError::new(
            "unsupported_platform",
            "Overlay requires Windows",
        ))
    }
}

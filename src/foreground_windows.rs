//! Process-lifetime, fail-closed foreground observation for paste intent.
//! Hook callbacks record only window identities; no titles or content are read.
use crate::overlay::{ForegroundVisit, LeasePhase, OverlayError, Result, Target};

fn unavailable() -> OverlayError {
    OverlayError::new(
        "tracking_unavailable",
        "Foreground tracking is unavailable; refocus the destination and Katran",
    )
}

#[derive(Default)]
struct Policy {
    epoch: u64,
    current: u64,
    own: bool,
    candidate: Option<Target>,
    target: Option<Target>,
    armed: Option<(u64, Target)>,
    consumed: bool,
}
impl Policy {
    fn external(&mut self, hwnd: u64, target: Option<Target>) {
        if self.current == hwnd && !self.own && self.candidate == target {
            return;
        }
        let expected = self.own
            && self
                .armed
                .is_some_and(|(epoch, value)| epoch == self.epoch && Some(value) == target);
        self.current = hwnd;
        self.own = false;
        self.candidate = target;
        if !expected {
            self.epoch = self.epoch.wrapping_add(1);
            self.target = None;
            self.armed = None;
            self.consumed = false;
        }
    }
    fn own(&mut self, hwnd: u64) {
        if !self.own {
            self.epoch = self.epoch.wrapping_add(1);
            self.target = self.candidate;
            self.armed = None;
            self.consumed = false;
        }
        self.current = hwnd;
        self.own = true;
    }
    fn destroyed(&mut self, hwnd: u64) {
        if self.target.is_some_and(|t| t.hwnd == hwnd)
            || self.candidate.is_some_and(|t| t.hwnd == hwnd)
        {
            self.epoch = self.epoch.wrapping_add(1);
            self.target = None;
            self.candidate = None;
            self.armed = None;
            self.consumed = true;
        }
    }
    fn visit(&self) -> ForegroundVisit {
        ForegroundVisit {
            visit_epoch: self.epoch,
            owner_foreground: self.own,
            target: if !self.own {
                self.candidate
            } else if self.consumed {
                None
            } else {
                self.target
            },
        }
    }
    fn check(&self, epoch: u64, expected: Target, phase: LeasePhase) -> Result<()> {
        if self.consumed || self.epoch != epoch || self.target != Some(expected) {
            return Err(OverlayError::new(
                "foreground_changed",
                "The paste intent is no longer current",
            ));
        }
        let valid = match phase {
            LeasePhase::Owner => self.own,
            LeasePhase::ActivatingTarget => {
                !self.own && self.current == expected.hwnd && self.armed == Some((epoch, expected))
            }
        };
        if !valid {
            return Err(OverlayError::new(
                "foreground_changed",
                "Foreground changed during paste",
            ));
        }
        Ok(())
    }
    fn arm(&mut self, epoch: u64, expected: Target) -> Result<()> {
        self.check(epoch, expected, LeasePhase::Owner)?;
        self.armed = Some((epoch, expected));
        Ok(())
    }
    fn consume(&mut self, epoch: u64) -> Result<()> {
        if self.epoch != epoch {
            return Err(OverlayError::new(
                "foreground_changed",
                "The paste intent expired",
            ));
        }
        self.consumed = true;
        self.armed = None;
        self.target = None;
        // A new capture while the destination remains foreground is a new intent.
        self.epoch = self.epoch.wrapping_add(1);
        Ok(())
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use crate::overlay::{Host, Target};
    use std::{
        cell::{Cell, RefCell},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, SyncSender},
        },
        thread::{self, JoinHandle},
        time::Duration,
    };
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
            WindowsAndMessaging::*,
        },
    };
    thread_local! { static STATE: RefCell<Option<Policy>> = const { RefCell::new(None) }; static FAULT: Cell<bool> = const { Cell::new(false) }; }
    const WAKE: u32 = WM_APP + 411;
    const LIMIT: usize = 64;
    enum Request {
        Visit(u64, SyncSender<Result<ForegroundVisit>>),
        Check(u64, u64, Target, LeasePhase, SyncSender<Result<()>>),
        Arm(u64, u64, Target, SyncSender<Result<()>>),
        Consume(u64, u64, SyncSender<Result<()>>),
    }
    fn identity(window: u64) -> Result<Target> {
        crate::overlay_windows::WindowsHost::new().identity(window)
    }
    fn classify(window: HWND) -> Option<Target> {
        if window.0.is_null() {
            return None;
        }
        let mut name = [0u16; 128];
        let n = unsafe { GetClassNameW(window, &mut name) };
        if n <= 0 {
            return None;
        }
        let class = String::from_utf16_lossy(&name[..n as usize]);
        if matches!(
            class.as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW"
        ) {
            return None;
        }
        identity(window.0 as usize as u64).ok()
    }
    unsafe extern "system" fn callback(
        _: HWINEVENTHOOK,
        event: u32,
        window: HWND,
        object: i32,
        child: i32,
        _: u32,
        _: u32,
    ) {
        if event == EVENT_OBJECT_DESTROY && (object != 0 || child != 0) {
            return;
        }
        let value = window.0 as usize as u64;
        STATE.with(|cell| {
            let Ok(mut slot) = cell.try_borrow_mut() else {
                FAULT.with(|f| f.set(true));
                return;
            };
            let Some(state) = slot.as_mut() else {
                FAULT.with(|f| f.set(true));
                return;
            };
            if event == EVENT_OBJECT_DESTROY {
                state.destroyed(value);
                return;
            }
            if event != EVENT_SYSTEM_FOREGROUND {
                return;
            }
            let target = classify(window);
            if target.is_some_and(|t| t.pid == std::process::id()) {
                state.own(value);
            } else {
                state.external(value, target);
            }
        });
    }
    fn current(owner: u64, state: &Policy) -> Result<()> {
        let own = identity(owner)?;
        if own.pid != std::process::id() {
            return Err(unavailable());
        }
        let foreground = unsafe { GetForegroundWindow() }.0 as usize as u64;
        if foreground == 0 || foreground != state.current {
            return Err(unavailable());
        }
        Ok(())
    }
    fn service(request: Request) {
        STATE.with(|slot| {
            let state = slot.borrow();
            let Some(state) = state.as_ref() else {
                return;
            };
            let failed = FAULT.with(|f| f.get());
            match request {
                Request::Visit(owner, reply) => {
                    let _ = reply.send(if failed {
                        Err(unavailable())
                    } else {
                        current(owner, state).map(|_| state.visit())
                    });
                }
                Request::Check(owner, epoch, target, phase, reply) => {
                    let _ = reply.send(if failed {
                        Err(unavailable())
                    } else {
                        current(owner, state).and_then(|_| state.check(epoch, target, phase))
                    });
                }
                Request::Arm(..) | Request::Consume(..) => unreachable!(),
            }
        });
    }
    fn mutate(request: Request) {
        STATE.with(|slot| {
            let mut state = slot.borrow_mut();
            let Some(state) = state.as_mut() else {
                return;
            };
            let failed = FAULT.with(|f| f.get());
            match request {
                Request::Arm(owner, epoch, target, reply) => {
                    let _ = reply.send(if failed {
                        Err(unavailable())
                    } else {
                        current(owner, state).and_then(|_| state.arm(epoch, target))
                    });
                }
                Request::Consume(owner, epoch, reply) => {
                    let _ = reply.send(if failed {
                        Err(unavailable())
                    } else {
                        let _ = owner;
                        state.consume(epoch)
                    });
                }
                Request::Visit(..) | Request::Check(..) => unreachable!(),
            }
        });
    }
    pub struct Observer {
        sender: SyncSender<Request>,
        thread_id: u32,
        worker: Option<JoinHandle<()>>,
        failed: Arc<AtomicBool>,
    }
    impl Observer {
        pub fn start() -> Result<Self> {
            let (sender, receiver) = mpsc::sync_channel(LIMIT);
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let failed = Arc::new(AtomicBool::new(false));
            let flag = failed.clone();
            let worker = thread::Builder::new()
                .name("katran-foreground".into())
                .spawn(move || run(receiver, ready_tx, flag))
                .map_err(|_| unavailable())?;
            let thread_id = ready_rx
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| unavailable())??;
            Ok(Self {
                sender,
                thread_id,
                worker: Some(worker),
                failed,
            })
        }
        fn request<T>(&self, make: impl FnOnce(SyncSender<Result<T>>) -> Request) -> Result<T> {
            if self.failed.load(Ordering::Acquire) {
                return Err(unavailable());
            }
            let (sender, receiver) = mpsc::sync_channel(1);
            if self.sender.try_send(make(sender)).is_err()
                || unsafe { PostThreadMessageW(self.thread_id, WAKE, WPARAM(0), LPARAM(0)) }
                    .is_err()
            {
                self.failed.store(true, Ordering::Release);
                return Err(unavailable());
            }
            match receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(value) => value,
                Err(_) => {
                    self.failed.store(true, Ordering::Release);
                    Err(unavailable())
                }
            }
        }
        pub fn visit(&self, owner: u64) -> Result<ForegroundVisit> {
            self.request(|reply| Request::Visit(owner, reply))
        }
        pub fn check(
            &self,
            owner: u64,
            epoch: u64,
            target: Target,
            phase: LeasePhase,
        ) -> Result<()> {
            self.request(|reply| Request::Check(owner, epoch, target, phase, reply))
        }
        pub fn arm(&self, owner: u64, epoch: u64, target: Target) -> Result<()> {
            self.request(|reply| Request::Arm(owner, epoch, target, reply))
        }
        pub fn consume(&self, owner: u64, epoch: u64) -> Result<()> {
            self.request(|reply| Request::Consume(owner, epoch, reply))
        }
    }
    impl Drop for Observer {
        fn drop(&mut self) {
            self.failed.store(true, Ordering::Release);
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
    fn run(receiver: Receiver<Request>, ready: SyncSender<Result<u32>>, failed: Arc<AtomicBool>) {
        let tid = unsafe { GetCurrentThreadId() };
        let mut message = MSG::default();
        unsafe {
            let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        }
        STATE.with(|state| *state.borrow_mut() = Some(Policy::default()));
        let focus = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        let destroy = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_DESTROY,
                EVENT_OBJECT_DESTROY,
                None,
                Some(callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if focus.is_invalid() || destroy.is_invalid() {
            if !focus.is_invalid() {
                unsafe {
                    let _ = UnhookWinEvent(focus);
                }
            }
            if !destroy.is_invalid() {
                unsafe {
                    let _ = UnhookWinEvent(destroy);
                }
            }
            let _ = ready.send(Err(unavailable()));
            return;
        }
        // Initial observation after hooks were registered; later callbacks remain ordered.
        let initial = unsafe { GetForegroundWindow() };
        unsafe { callback(focus, EVENT_SYSTEM_FOREGROUND, initial, 0, 0, 0, 0) };
        if ready.send(Ok(tid)).is_ok() {
            loop {
                let result = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
                if result <= 0 {
                    break;
                }
                if message.message == WAKE {
                    while let Ok(request) = receiver.try_recv() {
                        // Drain queued native messages before replying; if the bounded
                        // barrier cannot catch up, the observer is permanently unsafe.
                        let mut pending = MSG::default();
                        let mut drained = 0usize;
                        while unsafe { PeekMessageW(&mut pending, None, 0, 0, PM_REMOVE) }.as_bool()
                        {
                            drained += 1;
                            if pending.message == WM_QUIT {
                                FAULT.with(|f| f.set(true));
                                let _ = unsafe {
                                    PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0))
                                };
                                break;
                            }
                            if pending.message != WAKE {
                                unsafe {
                                    let _ = TranslateMessage(&pending);
                                    DispatchMessageW(&pending);
                                }
                            }
                            if drained >= 256 {
                                FAULT.with(|f| f.set(true));
                                break;
                            }
                        }
                        match request {
                            Request::Visit(..) | Request::Check(..) => service(request),
                            _ => mutate(request),
                        }
                    }
                } else {
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
        }
        failed.store(true, Ordering::Release);
        unsafe {
            let _ = UnhookWinEvent(focus);
            let _ = UnhookWinEvent(destroy);
        }
        STATE.with(|state| *state.borrow_mut() = None);
    }
}
#[cfg(windows)]
pub(crate) use native::Observer;

#[cfg(test)]
mod tests {
    use super::*;
    fn t(hwnd: u64) -> Target {
        Target {
            hwnd,
            pid: hwnd as u32,
            process_started: hwnd,
        }
    }
    #[test]
    fn newer_external_wins_and_own_windows_preserve_it() {
        let mut p = Policy::default();
        p.external(10, Some(t(10)));
        p.own(100);
        p.external(20, Some(t(20)));
        p.own(100);
        let epoch = p.epoch;
        p.own(101);
        assert_eq!(p.visit().target, Some(t(20)));
        assert_eq!(p.epoch, epoch);
        p.external(30, None);
        p.own(100);
        assert_eq!(p.visit().target, None);
    }
    #[test]
    fn external_snapshot_rejects_old_capture_and_changed_identity() {
        let mut p = Policy::default();
        p.external(10, Some(t(10)));
        p.own(100);
        p.external(20, Some(t(20)));
        assert!(!p.visit().owner_foreground);
        assert_eq!(p.visit().target, Some(t(20)));
        let old_epoch = p.epoch;
        let reused = Target {
            process_started: 999,
            ..t(20)
        };
        p.external(20, Some(reused));
        assert_ne!(p.epoch, old_epoch);
        assert_eq!(p.visit().target, Some(reused));
        p.own(100);
        assert_eq!(p.visit().target, Some(reused));
    }
    #[test]
    fn third_party_steal_during_guarded_activation_cancels_permanently() {
        let mut p = Policy::default();
        p.external(10, Some(t(10)));
        p.own(100);
        let epoch = p.epoch;
        p.arm(epoch, t(10)).unwrap();
        p.external(20, Some(t(20)));
        p.external(10, Some(t(10)));
        assert!(p.check(epoch, t(10), LeasePhase::ActivatingTarget).is_err());
        p.own(100);
        assert!(p.check(epoch, t(10), LeasePhase::Owner).is_err());
    }
    #[test]
    fn lease_cancels_on_leave_return_and_destroy_and_is_single_use() {
        let mut p = Policy::default();
        p.external(10, Some(t(10)));
        p.own(100);
        let epoch = p.epoch;
        p.external(20, Some(t(20)));
        p.own(100);
        assert!(p.check(epoch, t(10), LeasePhase::Owner).is_err());
        let epoch = p.epoch;
        p.arm(epoch, t(20)).unwrap();
        p.external(20, Some(t(20)));
        p.check(epoch, t(20), LeasePhase::ActivatingTarget).unwrap();
        p.consume(epoch).unwrap();
        assert!(p.check(epoch, t(20), LeasePhase::ActivatingTarget).is_err());
        assert!(!p.visit().owner_foreground);
        assert_eq!(p.visit().target, Some(t(20)));
        assert_ne!(p.visit().visit_epoch, epoch);
        p.own(100);
        p.destroyed(20);
        assert_eq!(p.visit().target, None);
    }
}

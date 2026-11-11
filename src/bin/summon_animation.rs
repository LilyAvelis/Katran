//! Small, nonblocking native slide for a summoned window.
use std::time::{Duration, Instant};

const MIN: Duration = Duration::from_millis(220);
const CAP: Duration = Duration::from_millis(650);
const OFFSET: i32 = 16;
fn frame_settled(waits: bool, since: Option<Duration>) -> bool {
    !waits || since.is_some_and(|elapsed| elapsed >= Duration::from_millis(100))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

struct Motion {
    original: Rect,
    expected: Rect,
    started: Instant,
    serial: u64,
    waits_for_video: bool,
    ready_since: Option<Instant>,
    hiding: bool,
}
#[derive(Default)]
pub(super) struct State {
    motion: Option<Motion>,
    hidden: bool,
}

// Most movement takes 220 ms; the last few pixels cover decoder startup until
// a fresh frame arrives, with a hard deadline so failed media cannot hold it.
fn offset(elapsed: Duration, ready: bool) -> i32 {
    (OFFSET as f32 * avilon_egui::transition::reveal_remaining(elapsed, ready, MIN, CAP)).ceil()
        as i32
}
impl State {
    pub(super) fn start(&mut self, hwnd: u64, serial: u64, waits_for_video: bool) {
        self.cancel(hwnd);
        self.hidden = false;
        if !native::animations_enabled() {
            return;
        }
        if let Some(original) = native::rect(hwnd) {
            let expected = Rect {
                y: original.y + OFFSET,
                ..original
            };
            if native::move_to(hwnd, expected) {
                self.motion = Some(Motion {
                    original,
                    expected,
                    started: Instant::now(),
                    serial,
                    waits_for_video,
                    ready_since: None,
                    hiding: false,
                });
            }
        }
    }
    pub(super) fn tick(&mut self, hwnd: u64, serial: u64) -> bool {
        let Some(m) = self.motion.as_mut() else {
            return false;
        };
        // Do not fight a user drag, resize, or another window manager operation.
        if native::rect(hwnd) != Some(m.expected) {
            self.hidden |= m.hiding;
            self.motion = None;
            return false;
        }
        let elapsed = m.started.elapsed();
        let ready = !m.waits_for_video || serial != m.serial;
        if ready && m.ready_since.is_none() {
            m.ready_since = Some(Instant::now());
        }
        let settled = frame_settled(m.waits_for_video, m.ready_since.map(|t| t.elapsed()));
        let delta = if m.hiding {
            (OFFSET as f32 * avilon_egui::transition::dismiss_progress(elapsed, MIN)).floor() as i32
        } else {
            offset(elapsed, settled)
        };
        let next = Rect {
            y: m.original.y + delta,
            ..m.original
        };
        if next != m.expected && !native::move_to(hwnd, next) {
            self.hidden |= m.hiding;
            self.motion = None;
            return false;
        }
        m.expected = next;
        if m.hiding && elapsed >= MIN {
            native::hide(hwnd);
            native::move_to(hwnd, m.original);
            self.motion = None;
            self.hidden = true;
            return false;
        }
        if !m.hiding && delta == 0 {
            self.hidden |= m.hiding;
            self.motion = None;
            return false;
        }
        true
    }
    pub(super) fn take_hidden(&mut self) -> bool {
        std::mem::take(&mut self.hidden)
    }
    pub(super) fn start_hide(&mut self, hwnd: u64) -> bool {
        self.cancel(hwnd);
        self.hidden = false;
        if native::animations_enabled()
            && let Some(original) = native::rect(hwnd)
        {
            self.motion = Some(Motion {
                original,
                expected: original,
                started: Instant::now(),
                serial: 0,
                waits_for_video: false,
                ready_since: None,
                hiding: true,
            });
            true
        } else {
            native::hide(hwnd);
            false
        }
    }
    pub(super) fn cancel(&mut self, hwnd: u64) {
        self.hidden = false;
        if let Some(m) = self.motion.take()
            && native::rect(hwnd) == Some(m.expected)
        {
            native::move_to(hwnd, m.original);
        }
    }
}

#[cfg(all(windows, not(test)))]
mod native {
    use super::Rect;
    use windows_sys::Win32::{Foundation::RECT, UI::WindowsAndMessaging::*};
    fn hwnd(raw: u64) -> windows_sys::Win32::Foundation::HWND {
        raw as usize as _
    }
    pub fn hide(raw: u64) {
        use windows::Win32::{
            Foundation::HWND,
            Graphics::Dwm::{
                DWMWA_TRANSITIONS_FORCEDISABLED, DwmGetWindowAttribute, DwmSetWindowAttribute,
            },
        };
        let window = HWND(raw as usize as *mut _);
        let mut previous = 0i32;
        unsafe {
            let known = DwmGetWindowAttribute(
                window,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                std::ptr::from_mut(&mut previous).cast(),
                4,
            )
            .is_ok();
            if known {
                let disabled = 1i32;
                let _ = DwmSetWindowAttribute(
                    window,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    std::ptr::from_ref(&disabled).cast(),
                    4,
                );
            }
            ShowWindow(hwnd(raw), SW_HIDE);
            if known {
                let _ = DwmSetWindowAttribute(
                    window,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    std::ptr::from_ref(&previous).cast(),
                    4,
                );
            }
        }
    }
    pub fn animations_enabled() -> bool {
        let mut enabled: i32 = 0;
        unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut enabled as *mut i32).cast(),
                0,
            ) != 0
                && enabled != 0
        }
    }
    pub fn rect(raw: u64) -> Option<Rect> {
        if unsafe { IsZoomed(hwnd(raw)) != 0 || IsIconic(hwnd(raw)) != 0 } {
            return None;
        }
        let mut r = RECT::default();
        if unsafe { GetWindowRect(hwnd(raw), &mut r) } == 0 {
            return None;
        }
        Some(Rect {
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
        })
    }
    pub fn move_to(raw: u64, r: Rect) -> bool {
        unsafe {
            SetWindowPos(
                hwnd(raw),
                std::ptr::null_mut(),
                r.x,
                r.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            ) != 0
        }
    }
}
#[cfg(any(not(windows), test))]
mod native {
    use super::Rect;
    pub fn hide(_: u64) {}
    pub fn animations_enabled() -> bool {
        false
    }
    pub fn rect(_: u64) -> Option<Rect> {
        None
    }
    pub fn move_to(_: u64, _: Rect) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_video_gets_one_hundred_milliseconds_to_advance() {
        assert!(!frame_settled(true, None));
        assert!(!frame_settled(true, Some(Duration::from_millis(99))));
        assert!(frame_settled(true, Some(Duration::from_millis(100))));
        assert!(frame_settled(false, None));
        assert_eq!(
            avilon_egui::transition::dismiss_progress(Duration::ZERO, MIN),
            0.0
        );
        assert_eq!(avilon_egui::transition::dismiss_progress(MIN, MIN), 1.0);
    }
    #[test]
    fn motion_waits_for_frame_but_has_a_deadline() {
        assert_eq!(offset(Duration::ZERO, false), 16);
        assert!(offset(MIN, false) > 0);
        assert_eq!(offset(MIN, true), 0);
        assert_eq!(offset(CAP, false), 0);
        let values: Vec<_> = (0..=650)
            .map(|ms| offset(Duration::from_millis(ms), false))
            .collect();
        assert!(values.windows(2).all(|w| w[0] >= w[1]));
    }
}

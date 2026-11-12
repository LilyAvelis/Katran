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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    None,
    Down,
    Up,
    Left,
    Right,
}
impl Style {
    fn parse(value: &str) -> Self {
        match value {
            "none" => Self::None,
            "slide_up" => Self::Up,
            "slide_left" => Self::Left,
            "slide_right" => Self::Right,
            _ => Self::Down,
        }
    }
    fn displaced(self, original: Rect, delta: i32) -> Rect {
        match self {
            Self::None => original,
            Self::Down => Rect {
                y: original.y.saturating_add(delta),
                ..original
            },
            Self::Up => Rect {
                y: original.y.saturating_sub(delta),
                ..original
            },
            Self::Left => Rect {
                x: original.x.saturating_sub(delta),
                ..original
            },
            Self::Right => Rect {
                x: original.x.saturating_add(delta),
                ..original
            },
        }
    }
}

struct Motion {
    original: Rect,
    expected: Rect,
    started: Instant,
    serial: u64,
    waits_for_video: bool,
    ready_since: Option<Instant>,
    hiding: bool,
    style: Style,
    duration: Duration,
    deadline: Duration,
}
#[derive(Default)]
pub(super) struct State {
    motion: Option<Motion>,
    hidden: bool,
}

// Most movement takes 220 ms; the last few pixels cover decoder startup until
// a fresh frame arrives, with a hard deadline so failed media cannot hold it.
fn offset(elapsed: Duration, ready: bool, duration: Duration, deadline: Duration) -> i32 {
    (OFFSET as f32 * avilon_egui::transition::reveal_remaining(elapsed, ready, duration, deadline))
        .ceil() as i32
}
impl State {
    #[allow(dead_code)]
    pub(super) fn start(&mut self, hwnd: u64, serial: u64, waits_for_video: bool) {
        self.start_with(hwnd, serial, waits_for_video, "slide_down", 220);
    }
    pub(super) fn start_with(
        &mut self,
        hwnd: u64,
        serial: u64,
        waits_for_video: bool,
        style: &str,
        duration_ms: u64,
    ) {
        self.cancel(hwnd);
        self.hidden = false;
        let style = Style::parse(style);
        if style == Style::None || duration_ms == 0 || !native::animations_enabled() {
            return;
        }
        let duration = Duration::from_millis(duration_ms.clamp(50, 800));
        // Preserve the existing bounded video-frame wait for the default motion.
        let waits_for_video = waits_for_video && style == Style::Down && duration == MIN;
        let deadline = if waits_for_video {
            CAP
        } else {
            duration + Duration::from_millis(1)
        };
        if let Some(original) = native::rect(hwnd) {
            let expected = style.displaced(original, OFFSET);
            if native::move_to(hwnd, expected) {
                self.motion = Some(Motion {
                    original,
                    expected,
                    started: Instant::now(),
                    serial,
                    waits_for_video,
                    ready_since: None,
                    hiding: false,
                    style,
                    duration,
                    deadline,
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
            (OFFSET as f32 * avilon_egui::transition::dismiss_progress(elapsed, m.duration)).floor()
                as i32
        } else {
            offset(elapsed, settled, m.duration, m.deadline)
        };
        let next = m.style.displaced(m.original, delta);
        if next != m.expected && !native::move_to(hwnd, next) {
            self.hidden |= m.hiding;
            self.motion = None;
            return false;
        }
        m.expected = next;
        if m.hiding && elapsed >= m.duration {
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
    #[allow(dead_code)]
    pub(super) fn start_hide(&mut self, hwnd: u64) -> bool {
        self.start_hide_with(hwnd, "slide_down", 220)
    }
    pub(super) fn start_hide_with(&mut self, hwnd: u64, style: &str, duration_ms: u64) -> bool {
        self.cancel(hwnd);
        self.hidden = false;
        let style = Style::parse(style);
        if style != Style::None
            && duration_ms != 0
            && native::animations_enabled()
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
                style,
                duration: Duration::from_millis(duration_ms.clamp(50, 800)),
                deadline: Duration::ZERO,
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
    fn styles_move_along_the_requested_axis_and_unknown_uses_default() {
        let r = Rect {
            x: 100,
            y: 200,
            width: 300,
            height: 400,
        };
        assert_eq!(Style::parse("none").displaced(r, 16), r);
        assert_eq!(Style::parse("slide_down").displaced(r, 16).y, 216);
        assert_eq!(Style::parse("slide_up").displaced(r, 16).y, 184);
        assert_eq!(Style::parse("slide_left").displaced(r, 16).x, 84);
        assert_eq!(Style::parse("slide_right").displaced(r, 16).x, 116);
        assert_eq!(Style::parse("unknown"), Style::Down);
    }
    #[test]
    fn custom_duration_finishes_on_schedule() {
        for ms in [50, 100, 400, 800] {
            let d = Duration::from_millis(ms);
            assert_eq!(
                offset(Duration::ZERO, true, d, d + Duration::from_millis(1)),
                16
            );
            assert_eq!(offset(d, true, d, d + Duration::from_millis(1)), 0);
            assert_eq!(avilon_egui::transition::dismiss_progress(d, d), 1.0);
        }
    }
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
        assert_eq!(offset(Duration::ZERO, false, MIN, CAP), 16);
        assert!(offset(MIN, false, MIN, CAP) > 0);
        assert_eq!(offset(MIN, true, MIN, CAP), 0);
        assert_eq!(offset(CAP, false, MIN, CAP), 0);
        let values: Vec<_> = (0..=650)
            .map(|ms| offset(Duration::from_millis(ms), false, MIN, CAP))
            .collect();
        assert!(values.windows(2).all(|w| w[0] >= w[1]));
    }
}

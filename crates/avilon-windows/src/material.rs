//! Explicit Desktop Acrylic requests for an existing native window.
use std::fmt;

/// The material to request from DWM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialRequest {
    /// Request Desktop Acrylic and rounded corners.
    Acrylic,
    /// Restore the normal backdrop and default corner preference.
    None,
}

/// DWM accepted both requests; this does not guarantee visible material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialApplied;

/// An explicit failure, including any failure to undo a partially applied request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialError {
    Unavailable,
    InvalidWindow,
    Backdrop(String),
    Corner {
        enabled: bool,
        source: String,
        rollback: Option<String>,
    },
}

impl fmt::Display for MaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(f, "Системный Acrylic доступен только в Windows"),
            Self::InvalidWindow => write!(f, "Окно недоступно для оформления"),
            Self::Backdrop(source) => write!(f, "Системный Acrylic недоступен: {source}"),
            Self::Corner {
                enabled,
                source,
                rollback,
            } => {
                if *enabled {
                    write!(f, "Не удалось настроить скругление окна: {source}")?;
                } else {
                    write!(f, "Не удалось восстановить скругление окна: {source}")?;
                }
                if let Some(rollback) = rollback {
                    write!(f, "; откат материала не удался: {rollback}")?;
                }
                Ok(())
            }
        }
    }
}
impl std::error::Error for MaterialError {}

#[cfg(any(windows, test))]
trait Dwm {
    fn is_window(&self, hwnd: isize) -> bool;
    fn backdrop(&self, hwnd: isize, enabled: bool) -> Result<(), String>;
    fn corner(&self, hwnd: isize, enabled: bool) -> Result<(), String>;
}

#[cfg(any(windows, test))]
fn apply_with(
    provider: &impl Dwm,
    hwnd: isize,
    request: MaterialRequest,
) -> Result<MaterialApplied, MaterialError> {
    if hwnd == 0 || !provider.is_window(hwnd) {
        return Err(MaterialError::InvalidWindow);
    }
    let enabled = request == MaterialRequest::Acrylic;
    provider
        .backdrop(hwnd, enabled)
        .map_err(MaterialError::Backdrop)?;
    if let Err(source) = provider.corner(hwnd, enabled) {
        let rollback = if enabled {
            provider.backdrop(hwnd, false).err()
        } else {
            None
        };
        return Err(MaterialError::Corner {
            enabled,
            source,
            rollback,
        });
    }
    Ok(MaterialApplied)
}

/// Request a window material on its owning UI thread after HWND creation.
///
/// `hwnd` must identify a live window owned by the calling thread. DWM may
/// accept the request without making the effect visible: the host also needs
/// a transparent viewport, clear color and panel fills, and an opaque fallback.
/// The request changes only this window's DWM attributes; no assets are loaded.
#[cfg(windows)]
pub fn apply(hwnd: isize, request: MaterialRequest) -> Result<MaterialApplied, MaterialError> {
    apply_with(&NativeDwm, hwnd, request)
}

/// Material requests are unavailable outside Windows.
#[cfg(not(windows))]
pub fn apply(_: isize, _: MaterialRequest) -> Result<MaterialApplied, MaterialError> {
    Err(MaterialError::Unavailable)
}

#[cfg(windows)]
struct NativeDwm;
#[cfg(windows)]
impl Dwm for NativeDwm {
    fn is_window(&self, hwnd: isize) -> bool {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindow};
        unsafe { IsWindow(Some(HWND(hwnd as *mut _))) }.as_bool()
    }
    fn backdrop(&self, hwnd: isize, enabled: bool) -> Result<(), String> {
        use windows::Win32::{
            Foundation::HWND,
            Graphics::Dwm::{
                DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
                DwmSetWindowAttribute,
            },
        };
        let value = if enabled {
            DWMSBT_TRANSIENTWINDOW
        } else {
            DWMSBT_NONE
        };
        unsafe {
            DwmSetWindowAttribute(
                HWND(hwnd as *mut _),
                DWMWA_SYSTEMBACKDROP_TYPE,
                std::ptr::from_ref(&value).cast(),
                std::mem::size_of_val(&value) as u32,
            )
        }
        .map_err(|e| e.to_string())
    }
    fn corner(&self, hwnd: isize, enabled: bool) -> Result<(), String> {
        use windows::Win32::{
            Foundation::HWND,
            Graphics::Dwm::{
                DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_ROUND, DwmSetWindowAttribute,
            },
        };
        let value = if enabled {
            DWMWCP_ROUND
        } else {
            DWMWCP_DEFAULT
        };
        unsafe {
            DwmSetWindowAttribute(
                HWND(hwnd as *mut _),
                DWMWA_WINDOW_CORNER_PREFERENCE,
                std::ptr::from_ref(&value).cast(),
                std::mem::size_of_val(&value) as u32,
            )
        }
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct Fake {
        calls: RefCell<Vec<&'static str>>,
        valid: bool,
        fail: Vec<&'static str>,
    }
    impl Fake {
        fn new(valid: bool, fail: Vec<&'static str>) -> Self {
            Self {
                calls: RefCell::new(vec![]),
                valid,
                fail,
            }
        }
        fn call(&self, name: &'static str) -> Result<(), String> {
            self.calls.borrow_mut().push(name);
            if self.fail.contains(&name) {
                Err(name.into())
            } else {
                Ok(())
            }
        }
    }
    impl Dwm for Fake {
        fn is_window(&self, _: isize) -> bool {
            self.calls.borrow_mut().push("validate");
            self.valid
        }
        fn backdrop(&self, _: isize, enabled: bool) -> Result<(), String> {
            self.call(if enabled {
                "backdrop_on"
            } else {
                "backdrop_off"
            })
        }
        fn corner(&self, _: isize, enabled: bool) -> Result<(), String> {
            self.call(if enabled { "corner_on" } else { "corner_off" })
        }
    }
    #[test]
    fn success_and_disable() {
        let f = Fake::new(true, vec![]);
        assert_eq!(
            apply_with(&f, 1, MaterialRequest::Acrylic),
            Ok(MaterialApplied)
        );
        assert_eq!(
            apply_with(&f, 1, MaterialRequest::None),
            Ok(MaterialApplied)
        );
        assert_eq!(
            *f.calls.borrow(),
            [
                "validate",
                "backdrop_on",
                "corner_on",
                "validate",
                "backdrop_off",
                "corner_off"
            ]
        );
    }
    #[test]
    fn invalid_handles_have_no_effects() {
        let f = Fake::new(false, vec![]);
        assert_eq!(
            apply_with(&f, 0, MaterialRequest::Acrylic),
            Err(MaterialError::InvalidWindow)
        );
        assert!(f.calls.borrow().is_empty());
        assert_eq!(
            apply_with(&f, 1, MaterialRequest::Acrylic),
            Err(MaterialError::InvalidWindow)
        );
        assert_eq!(*f.calls.borrow(), ["validate"]);
    }
    #[test]
    fn backdrop_failure_stops() {
        let f = Fake::new(true, vec!["backdrop_on"]);
        assert!(matches!(
            apply_with(&f, 1, MaterialRequest::Acrylic),
            Err(MaterialError::Backdrop(_))
        ));
        assert_eq!(*f.calls.borrow(), ["validate", "backdrop_on"]);
    }
    #[test]
    fn corner_failure_rolls_back_and_reports_failure() {
        for rollback_fails in [false, true] {
            let mut failures = vec!["corner_on"];
            if rollback_fails {
                failures.push("backdrop_off");
            }
            let f = Fake::new(true, failures);
            let result = apply_with(&f, 1, MaterialRequest::Acrylic);
            assert!(
                matches!(result, Err(MaterialError::Corner { enabled: true, rollback, .. }) if rollback.is_some() == rollback_fails)
            );
            assert_eq!(
                *f.calls.borrow(),
                ["validate", "backdrop_on", "corner_on", "backdrop_off"]
            );
        }
    }
    #[test]
    fn disable_corner_failure_does_not_repeat_backdrop() {
        let f = Fake::new(true, vec!["corner_off"]);
        assert!(matches!(
            apply_with(&f, 1, MaterialRequest::None),
            Err(MaterialError::Corner {
                enabled: false,
                rollback: None,
                ..
            })
        ));
        assert_eq!(
            *f.calls.borrow(),
            ["validate", "backdrop_off", "corner_off"]
        );
    }
}

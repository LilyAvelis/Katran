//! Injectable system boundary for summon and explicit paste-back.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub hwnd: u64,
    pub pid: u32,
    pub process_started: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: u32,
}
impl Hotkey {
    pub fn parse(text: &str) -> Result<Self> {
        let mut hotkey = Self {
            ctrl: false,
            alt: false,
            shift: false,
            key: 0,
        };
        for part in text.split(['+', ' ']).filter(|part| !part.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" if !hotkey.ctrl => hotkey.ctrl = true,
                "alt" if !hotkey.alt => hotkey.alt = true,
                "shift" if !hotkey.shift => hotkey.shift = true,
                token if hotkey.key == 0 => {
                    let bytes = token.as_bytes();
                    hotkey.key = if bytes.len() == 1 && bytes[0].is_ascii_alphabetic() {
                        u32::from(bytes[0].to_ascii_uppercase())
                    } else if let Some(number) = token
                        .strip_prefix('f')
                        .and_then(|v| v.parse::<u32>().ok())
                        .filter(|n| (1..=24).contains(n))
                    {
                        0x6f + number
                    } else {
                        return Err(OverlayError::new(
                            "invalid_hotkey",
                            "Use Ctrl, Alt, Shift and one letter or F key",
                        ));
                    };
                }
                _ => {
                    return Err(OverlayError::new(
                        "invalid_hotkey",
                        "Duplicate or invalid hotkey component",
                    ));
                }
            }
        }
        if hotkey.key == 0 || !(hotkey.ctrl || hotkey.alt || hotkey.shift) {
            return Err(OverlayError::new(
                "invalid_hotkey",
                "Hotkey needs a modifier and a key",
            ));
        }
        Ok(hotkey)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OverlayError {
    pub code: String,
    pub message: String,
}
impl OverlayError {
    pub fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}
pub type Result<T> = std::result::Result<T, OverlayError>;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SummonReceipt {
    pub previous: Target,
    pub work_area: WorkArea,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PasteReceipt {
    pub sent: bool,
    pub target: Target,
}

/// Host methods must return errors for unavailable identities, focus failures,
/// held modifiers and blocked input; send_paste must recheck foreground.
pub trait Host: Send {
    fn foreground(&mut self) -> Result<u64>;
    fn identity(&mut self, hwnd: u64) -> Result<Target>;
    fn work_area(&mut self, hwnd: u64) -> Result<WorkArea>;
    fn wait_modifiers_released(&mut self) -> Result<()>;
    fn activate(&mut self, hwnd: u64) -> Result<()>;
    fn send_paste(&mut self, expected: Target, expected_sequence: u32) -> Result<()>;
}
#[derive(Default)]
pub struct Session {
    previous: Option<Target>,
}
impl Session {
    pub fn clear(&mut self) {
        self.previous = None;
    }
    pub fn summon(
        &mut self,
        host: &mut (impl Host + ?Sized),
        own_hwnd: u64,
    ) -> Result<SummonReceipt> {
        let own = host.identity(own_hwnd)?;
        let foreground = host.foreground()?;
        let target = host.identity(foreground)?;
        if target.hwnd == own_hwnd || target.pid == own.pid {
            return Err(OverlayError::new(
                "self_target",
                "Katran cannot be the previous application",
            ));
        }
        let work_area = host.work_area(foreground)?;
        self.previous = Some(target);
        Ok(SummonReceipt {
            previous: target,
            work_area,
        })
    }
    /// Revalidate a hotkey-thread capture; never trust supplied monitor geometry.
    pub fn adopt(
        &mut self,
        host: &mut (impl Host + ?Sized),
        own_hwnd: u64,
        capture: SummonReceipt,
    ) -> Result<SummonReceipt> {
        let own = host.identity(own_hwnd)?;
        let target = capture.previous;
        if target.hwnd == own_hwnd || target.pid == own.pid {
            return Err(OverlayError::new("self_target", "Katran cannot be the previous application"));
        }
        if host.identity(target.hwnd)? != target {
            return Err(OverlayError::new("target_changed", "Captured window or process changed"));
        }
        let foreground = host.foreground()?;
        if foreground != target.hwnd && foreground != own_hwnd {
            return Err(OverlayError::new("foreground_changed", "Captured application lost foreground"));
        }
        let work_area = host.work_area(target.hwnd)?;
        self.previous = Some(target);
        Ok(SummonReceipt { previous: target, work_area })
    }
    /// Call only after the dispatcher confirmed Clipboard publication.
    pub fn validate(&self, host: &mut (impl Host + ?Sized), own_hwnd: u64) -> Result<Target> {
        let target = self.previous.ok_or_else(|| {
            OverlayError::new("no_previous_target", "No previous application was captured")
        })?;
        let own = host.identity(own_hwnd)?;
        if target.pid == own.pid || target.hwnd == own_hwnd {
            return Err(OverlayError::new(
                "self_target",
                "Katran cannot paste into itself",
            ));
        }
        if host.identity(target.hwnd)? != target {
            return Err(OverlayError::new(
                "target_changed",
                "Previous window or process changed",
            ));
        }
        Ok(target)
    }
    pub fn paste(
        &mut self,
        host: &mut (impl Host + ?Sized),
        own_hwnd: u64,
        expected_sequence: u32,
    ) -> Result<PasteReceipt> {
        let target = self.validate(host, own_hwnd)?;
        host.wait_modifiers_released()?;
        if host.identity(target.hwnd)? != target {
            return Err(OverlayError::new(
                "target_changed",
                "Previous window or process changed",
            ));
        }
        host.activate(target.hwnd)?;
        if host.foreground()? != target.hwnd || host.identity(target.hwnd)? != target {
            return Err(OverlayError::new(
                "foreground_changed",
                "Previous application did not become foreground",
            ));
        }
        if expected_sequence == 0 {
            return Err(OverlayError::new(
                "clipboard_changed",
                "Clipboard publication has no verified sequence",
            ));
        }
        host.send_paste(target, expected_sequence)?;
        self.previous = None;
        Ok(PasteReceipt { sent: true, target })
    }
}

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
    #[serde(default)]
    pub win: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: u32,
}
impl Hotkey {
    /// Canonical presentation for the same physical Win32 key and modifiers.
    pub fn canonical(self) -> String {
        let mut parts = Vec::new();
        if self.win {
            parts.push("Win".to_owned());
        }
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        if (u32::from(b'A')..=u32::from(b'Z')).contains(&self.key)
            || (u32::from(b'0')..=u32::from(b'9')).contains(&self.key)
        {
            parts.push(char::from_u32(self.key).expect("ASCII letter").to_string());
        } else if (0x70..=0x87).contains(&self.key) {
            parts.push(format!("F{}", self.key - 0x6f));
        } else {
            parts.push(key_label(self.key).unwrap_or("Unsupported").to_owned());
        }
        parts.join("+")
    }
    pub fn parse(text: &str) -> Result<Self> {
        let mut hotkey = Self {
            win: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: 0,
        };
        for part in text.split(['+', ' ']).filter(|part| !part.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "win" | "super" | "meta" if !hotkey.win => hotkey.win = true,
                "ctrl" | "control" if !hotkey.ctrl => hotkey.ctrl = true,
                "alt" if !hotkey.alt => hotkey.alt = true,
                "shift" if !hotkey.shift => hotkey.shift = true,
                token if hotkey.key == 0 => {
                    let bytes = token.as_bytes();
                    hotkey.key = if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
                        u32::from(bytes[0].to_ascii_uppercase())
                    } else if let Some(key) = named_key(token) {
                        key
                    } else if let Some(number) = token
                        .strip_prefix('f')
                        .and_then(|v| v.parse::<u32>().ok())
                        .filter(|n| (1..=24).contains(n))
                    {
                        0x6f + number
                    } else {
                        return Err(OverlayError::new(
                            "invalid_hotkey",
                            "Use Win, Ctrl, Alt or Shift with a keyboard key",
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
        if hotkey.key == 0 || !(hotkey.win || hotkey.ctrl || hotkey.alt || hotkey.shift) {
            return Err(OverlayError::new(
                "invalid_hotkey",
                "Hotkey needs a modifier and a key",
            ));
        }
        Ok(hotkey)
    }
}
fn key_label(key: u32) -> Option<&'static str> {
    Some(match key {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0d => "Enter",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1b => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x2c => "PrintScreen",
        0x2d => "Insert",
        0x2e => "Delete",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0xba => "Semicolon",
        0xbb => "Equals",
        0xbc => "Comma",
        0xbd => "Minus",
        0xbe => "Period",
        0xbf => "Slash",
        0xc0 => "Backtick",
        0xdb => "OpenBracket",
        0xdc => "Backslash",
        0xdd => "CloseBracket",
        0xde => "Quote",
        _ => return None,
    })
}
fn named_key(token: &str) -> Option<u32> {
    Some(match token {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" | "return" => 0x0d,
        "pause" => 0x13,
        "capslock" => 0x14,
        "escape" | "esc" => 0x1b,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "arrowleft" | "left" => 0x25,
        "arrowup" | "up" => 0x26,
        "arrowright" | "right" => 0x27,
        "arrowdown" | "down" => 0x28,
        "printscreen" => 0x2c,
        "insert" => 0x2d,
        "delete" | "del" => 0x2e,
        "numlock" => 0x90,
        "scrolllock" => 0x91,
        "semicolon" | ";" => 0xba,
        "equals" | "equal" | "=" | "plus" => 0xbb,
        "comma" | "," => 0xbc,
        "minus" | "-" => 0xbd,
        "period" | "." => 0xbe,
        "slash" | "/" => 0xbf,
        "backtick" | "`" => 0xc0,
        "openbracket" | "bracketleft" | "[" => 0xdb,
        "backslash" | "\\" | "pipe" => 0xdc,
        "closebracket" | "bracketright" | "]" => 0xdd,
        "quote" | "'" => 0xde,
        _ => return None,
    })
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
    /// Input was submitted, but observation cleanup could not be confirmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking_warning: Option<String>,
}

/// Ordered native foreground evidence for the current Katran visit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForegroundVisit {
    pub visit_epoch: u64,
    pub owner_foreground: bool,
    pub target: Option<Target>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeasePhase {
    Owner,
    ActivatingTarget,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct PasteLease {
    pub owner: Target,
    pub target: Target,
    pub visit_epoch: Option<u64>,
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
    /// None is reserved for explicitly injected legacy hosts, not native observer failure.
    fn foreground_visit(&mut self, _owner: u64) -> Result<Option<ForegroundVisit>> {
        Ok(None)
    }
    fn lease_check(
        &mut self,
        owner: u64,
        _epoch: u64,
        expected: Target,
        phase: LeasePhase,
    ) -> Result<()> {
        let foreground = self.foreground()?;
        let current = self.identity(foreground)?;
        let own = self.identity(owner)?;
        let valid = match phase {
            LeasePhase::Owner => current.pid == own.pid,
            LeasePhase::ActivatingTarget => current == expected,
        };
        if !valid {
            return Err(OverlayError::new(
                "foreground_changed",
                "Foreground intent changed",
            ));
        }
        Ok(())
    }
    fn activate_with_lease(&mut self, _owner: u64, _epoch: u64, expected: Target) -> Result<()> {
        self.activate(expected.hwnd)
    }
    fn send_paste_leased(
        &mut self,
        owner: u64,
        epoch: u64,
        expected: Target,
        sequence: u32,
    ) -> Result<()> {
        self.lease_check(owner, epoch, expected, LeasePhase::ActivatingTarget)?;
        self.send_paste(expected, sequence)
    }
    fn consume_visit(&mut self, _owner: u64, _epoch: u64) -> Result<()> {
        Ok(())
    }
}
#[derive(Default)]
pub struct Session {
    previous: Option<Target>,
    consumed_visit: Option<u64>,
}
impl Session {
    pub fn clear(&mut self) {
        self.previous = None;
        self.consumed_visit = None;
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
            return Err(OverlayError::new(
                "self_target",
                "Katran cannot be the previous application",
            ));
        }
        if host.identity(target.hwnd)? != target {
            return Err(OverlayError::new(
                "target_changed",
                "Captured window or process changed",
            ));
        }
        let foreground = host.foreground()?;
        let own_process_foreground = host
            .identity(foreground)
            .is_ok_and(|current| current.pid == own.pid);
        if foreground != target.hwnd && !own_process_foreground {
            return Err(OverlayError::new(
                "foreground_changed",
                "Captured application lost foreground",
            ));
        }
        if let Some(visit) = host.foreground_visit(own_hwnd)?
            && (visit.target != Some(target) || self.consumed_visit == Some(visit.visit_epoch))
        {
            return Err(OverlayError::new(
                "stale_capture",
                "A newer foreground visit superseded this capture",
            ));
        }
        let work_area = host.work_area(target.hwnd)?;
        self.previous = Some(target);
        Ok(SummonReceipt {
            previous: target,
            work_area,
        })
    }
    /// Bind one Enter to observed identity and visit; no later observation retargets it.
    pub(crate) fn begin_lease(
        &self,
        host: &mut (impl Host + ?Sized),
        owner: u64,
    ) -> Result<PasteLease> {
        let own = host.identity(owner)?;
        let foreground = host.foreground()?;
        if host.identity(foreground)?.pid != own.pid {
            return Err(OverlayError::new(
                "foreground_changed",
                "Katran must be foreground to paste",
            ));
        }
        let visit = host.foreground_visit(owner)?;
        let (target, epoch) = if let Some(visit) = visit {
            if !visit.owner_foreground {
                return Err(OverlayError::new(
                    "foreground_changed",
                    "Katran must be foreground to paste",
                ));
            }
            if self.consumed_visit == Some(visit.visit_epoch) {
                return Err(OverlayError::new(
                    "no_previous_target",
                    "This foreground visit was already used",
                ));
            }
            let target = visit.target.ok_or_else(|| {
                OverlayError::new(
                    "no_previous_target",
                    "No external target is available for this visit",
                )
            })?;
            (target, Some(visit.visit_epoch))
        } else {
            (
                self.previous.ok_or_else(|| {
                    OverlayError::new("no_previous_target", "No previous application was captured")
                })?,
                None,
            )
        };
        if target.pid == own.pid || target.hwnd == own.hwnd {
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
        let lease = PasteLease {
            owner: own,
            target,
            visit_epoch: epoch,
        };
        Self::check_lease(host, &lease)?;
        Ok(lease)
    }
    pub(crate) fn check_lease(host: &mut (impl Host + ?Sized), lease: &PasteLease) -> Result<()> {
        if host.identity(lease.owner.hwnd)? != lease.owner {
            return Err(OverlayError::new(
                "owner_changed",
                "Katran window identity changed",
            ));
        }
        if let Some(epoch) = lease.visit_epoch {
            host.lease_check(lease.owner.hwnd, epoch, lease.target, LeasePhase::Owner)?;
        } else {
            let foreground = host.foreground()?;
            let current = host.identity(foreground)?;
            if current.pid != lease.owner.pid {
                return Err(OverlayError::new(
                    "foreground_changed",
                    "Foreground intent changed",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn deliver_lease(
        &mut self,
        host: &mut (impl Host + ?Sized),
        lease: PasteLease,
        sequence: u32,
    ) -> Result<PasteReceipt> {
        Self::check_lease(host, &lease)?;
        host.wait_modifiers_released()?;
        Self::check_lease(host, &lease)?;
        if host.identity(lease.target.hwnd)? != lease.target {
            return Err(OverlayError::new(
                "target_changed",
                "Previous window or process changed",
            ));
        }
        if sequence == 0 {
            return Err(OverlayError::new(
                "clipboard_changed",
                "Clipboard publication has no verified sequence",
            ));
        }
        let mut tracking_warning = None;
        if let Some(epoch) = lease.visit_epoch {
            host.activate_with_lease(lease.owner.hwnd, epoch, lease.target)?;
            host.lease_check(
                lease.owner.hwnd,
                epoch,
                lease.target,
                LeasePhase::ActivatingTarget,
            )?;
            // Take the single-use attempt before sending; even a partial send may have effects.
            self.consumed_visit = Some(epoch);
            self.previous = None;
            let sent = host.send_paste_leased(lease.owner.hwnd, epoch, lease.target, sequence);
            let consumed = host.consume_visit(lease.owner.hwnd, epoch);
            sent?;
            tracking_warning = consumed.err().map(|error| error.code);
        } else {
            host.activate(lease.target.hwnd)?;
            if host.foreground()? != lease.target.hwnd
                || host.identity(lease.target.hwnd)? != lease.target
            {
                return Err(OverlayError::new(
                    "foreground_changed",
                    "Previous application did not become foreground",
                ));
            }
            self.previous = None;
            host.send_paste(lease.target, sequence)?;
        }
        Ok(PasteReceipt {
            sent: true,
            target: lease.target,
            tracking_warning,
        })
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
        Ok(PasteReceipt {
            sent: true,
            target,
            tracking_warning: None,
        })
    }
}

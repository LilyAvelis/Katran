use katran::overlay::{Host, Hotkey, OverlayError, Session, Target, WorkArea};
struct Fake {
    foreground: u64,
    target: Target,
    own: Target,
    blocked: bool,
    steal: bool,
    sent: usize,
    held: bool,
    clipboard: u32,
}
impl Host for Fake {
    fn foreground(&mut self) -> katran::overlay::Result<u64> {
        Ok(self.foreground)
    }
    fn identity(&mut self, hwnd: u64) -> katran::overlay::Result<Target> {
        if hwnd == self.own.hwnd {
            Ok(self.own)
        } else if hwnd == self.target.hwnd {
            Ok(self.target)
        } else {
            Err(OverlayError::new("target_unavailable", "missing"))
        }
    }
    fn work_area(&mut self, _: u64) -> katran::overlay::Result<WorkArea> {
        Ok(WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        })
    }
    fn wait_modifiers_released(&mut self) -> katran::overlay::Result<()> {
        if self.held {
            Err(OverlayError::new("modifiers_held", "held"))
        } else {
            Ok(())
        }
    }
    fn activate(&mut self, hwnd: u64) -> katran::overlay::Result<()> {
        if self.blocked {
            return Err(OverlayError::new("activation_denied", "denied"));
        }
        self.foreground = if self.steal { 88 } else { hwnd };
        Ok(())
    }
    fn send_paste(
        &mut self,
        expected: Target,
        expected_sequence: u32,
    ) -> katran::overlay::Result<()> {
        if self.clipboard != expected_sequence {
            return Err(OverlayError::new("clipboard_changed", "changed"));
        }
        assert_eq!(self.foreground, expected.hwnd);
        self.sent += 1;
        Ok(())
    }
}
fn fake() -> Fake {
    Fake {
        foreground: 7,
        target: Target {
            hwnd: 7,
            pid: 12,
            process_started: 43,
        },
        own: Target {
            hwnd: 8,
            pid: 13,
            process_started: 44,
        },
        blocked: false,
        steal: false,
        sent: 0,
        held: false,
        clipboard: 77,
    }
}
#[test]
fn remembers_process_identity_and_returns_monitor_then_sends_once() {
    let mut host = fake();
    let mut session = Session::default();
    assert_eq!(session.summon(&mut host, 8).unwrap().work_area.left, -1920);
    assert_eq!(session.validate(&mut host, 8).unwrap(), host.target);
    host.foreground = 8;
    assert!(session.paste(&mut host, 8, 77).unwrap().sent);
    assert_eq!(host.sent, 1);
    assert_eq!(
        session.paste(&mut host, 8, 77).unwrap_err().code,
        "no_previous_target"
    );
}
#[test]
fn changed_target_and_lost_foreground_never_send_and_can_retry() {
    let mut host = fake();
    let mut session = Session::default();
    session.summon(&mut host, 8).unwrap();
    host.target.process_started += 1;
    assert_eq!(
        session.validate(&mut host, 8).unwrap_err().code,
        "target_changed"
    );
    assert_eq!(
        session.paste(&mut host, 8, 77).unwrap_err().code,
        "target_changed"
    );
    host.target.process_started -= 1;
    host.steal = true;
    assert_eq!(
        session.paste(&mut host, 8, 77).unwrap_err().code,
        "foreground_changed"
    );
    assert_eq!(host.sent, 0);
    host.steal = false;
    host.held = true;
    assert_eq!(
        session.paste(&mut host, 8, 77).unwrap_err().code,
        "modifiers_held"
    );
    host.held = false;
    host.blocked = true;
    assert_eq!(
        session.paste(&mut host, 8, 77).unwrap_err().code,
        "activation_denied"
    );
    host.blocked = false;
    host.clipboard = 78;
    assert_eq!(
        session.paste(&mut host, 8, 77).unwrap_err().code,
        "clipboard_changed"
    );
    assert_eq!(host.sent, 0);
    host.clipboard = 77;
    assert!(session.paste(&mut host, 8, 77).unwrap().sent);
}
#[test]
fn rejects_self_and_invalid_shortcuts() {
    let mut host = fake();
    host.foreground = 8;
    let mut session = Session::default();
    assert_eq!(
        session.summon(&mut host, 8).unwrap_err().code,
        "self_target"
    );
    assert_eq!(Hotkey::parse("Ctrl+Shift+V").unwrap().key, u32::from(b'V'));
    assert_eq!(Hotkey::parse("Alt F12").unwrap().key, 0x7b);
    for bad in ["V", "Ctrl", "Ctrl+Ctrl+V", "Ctrl+X+Y", "Ctrl+F25"] {
        assert!(Hotkey::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn captured_target_is_revalidated_and_self_summon_preserves_session() {
    let mut host = fake();
    let mut session = Session::default();
    let mut receipt = session.summon(&mut host, 8).unwrap();
    // Supplied monitor geometry is not authority.
    receipt.work_area.left = 9000;
    assert_eq!(
        session
            .adopt(&mut host, 8, receipt.clone())
            .unwrap()
            .work_area
            .left,
        -1920
    );
    host.foreground = 8;
    assert_eq!(
        session.summon(&mut host, 8).unwrap_err().code,
        "self_target"
    );
    assert_eq!(session.validate(&mut host, 8).unwrap(), host.target);
    assert_eq!(
        session
            .adopt(&mut host, 8, receipt.clone())
            .unwrap()
            .previous,
        host.target
    );
    host.foreground = 99;
    assert_eq!(
        session
            .adopt(&mut host, 8, receipt.clone())
            .unwrap_err()
            .code,
        "foreground_changed"
    );
    assert_eq!(session.validate(&mut host, 8).unwrap(), host.target);
    host.foreground = 7;
    host.target.process_started += 1;
    assert_eq!(
        session.adopt(&mut host, 8, receipt).unwrap_err().code,
        "target_changed"
    );
    assert_eq!(host.sent, 0);
}

#[test]
fn captured_external_survives_owner_activation_before_adoption_and_pastes() {
    let mut host = fake();
    let mut capture_session = Session::default();
    let capture = capture_session.summon(&mut host, 8).unwrap();
    host.foreground = 8;
    let mut session = Session::default();
    session.adopt(&mut host, 8, capture).unwrap();
    assert_eq!(session.paste(&mut host, 8, 77).unwrap().target, host.target);
    assert_eq!(host.sent, 1);
}

use katran::{
    Command, Request, Result, Service,
    interaction::{SelectionMode, ShelfTab},
    overlay::{ForegroundVisit, Host, LeasePhase, OverlayError, SummonReceipt, Target, WorkArea},
};
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
fn target(id: u64) -> Target {
    Target {
        hwnd: id,
        pid: if id == 8 || id == 9 { 1 } else { 7 },
        process_started: if id == 8 || id == 9 { 1 } else { 7 },
    }
}
struct State {
    foreground: u64,
    epoch: u64,
    target: Option<Target>,
    sent: Vec<Target>,
    fail_send: bool,
    fail_activate: bool,
    missing: Option<u64>,
    sequence: u32,
    unavailable: bool,
    consume_fails: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            foreground: 8,
            epoch: 1,
            target: Some(target(7)),
            sent: vec![],
            fail_send: false,
            fail_activate: false,
            missing: None,
            sequence: 0,
            unavailable: false,
            consume_fails: false,
        }
    }
}
struct Fake(Arc<Mutex<State>>);
impl Host for Fake {
    fn foreground(&mut self) -> katran::overlay::Result<u64> {
        Ok(self.0.lock().unwrap().foreground)
    }
    fn identity(&mut self, hwnd: u64) -> katran::overlay::Result<Target> {
        if self.0.lock().unwrap().missing == Some(hwnd) {
            Err(OverlayError::new(
                "target_unavailable",
                "synthetic unavailable",
            ))
        } else {
            Ok(target(hwnd))
        }
    }
    fn work_area(&mut self, _: u64) -> katran::overlay::Result<WorkArea> {
        Ok(WorkArea {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        })
    }
    fn wait_modifiers_released(&mut self) -> katran::overlay::Result<()> {
        Ok(())
    }
    fn activate(&mut self, hwnd: u64) -> katran::overlay::Result<()> {
        self.0.lock().unwrap().foreground = hwnd;
        Ok(())
    }
    fn send_paste(&mut self, expected: Target, sequence: u32) -> katran::overlay::Result<()> {
        let mut s = self.0.lock().unwrap();
        if s.sequence != sequence {
            return Err(OverlayError::new("clipboard_changed", "changed"));
        }
        s.sent.push(expected);
        if s.fail_send {
            Err(OverlayError::new("partial_input", "partial"))
        } else {
            Ok(())
        }
    }
    fn foreground_visit(&mut self, _: u64) -> katran::overlay::Result<Option<ForegroundVisit>> {
        let s = self.0.lock().unwrap();
        if s.unavailable {
            return Err(OverlayError::new("tracking_unavailable", "synthetic"));
        }
        Ok(Some(ForegroundVisit {
            visit_epoch: s.epoch,
            owner_foreground: s.foreground == 8 || s.foreground == 9,
            target: s.target,
        }))
    }
    fn lease_check(
        &mut self,
        _: u64,
        epoch: u64,
        expected: Target,
        phase: LeasePhase,
    ) -> katran::overlay::Result<()> {
        let s = self.0.lock().unwrap();
        if s.unavailable
            || s.epoch != epoch
            || s.target != Some(expected)
            || match phase {
                LeasePhase::Owner => s.foreground != 8 && s.foreground != 9,
                LeasePhase::ActivatingTarget => s.foreground != expected.hwnd,
            }
        {
            return Err(OverlayError::new("foreground_changed", "visit changed"));
        }
        Ok(())
    }
    fn activate_with_lease(
        &mut self,
        owner: u64,
        epoch: u64,
        expected: Target,
    ) -> katran::overlay::Result<()> {
        self.lease_check(owner, epoch, expected, LeasePhase::Owner)?;
        let mut s = self.0.lock().unwrap();
        if s.fail_activate {
            return Err(OverlayError::new("activation_timeout", "synthetic timeout"));
        }
        s.foreground = expected.hwnd;
        Ok(())
    }
    fn consume_visit(&mut self, _: u64, epoch: u64) -> katran::overlay::Result<()> {
        let mut s = self.0.lock().unwrap();
        if s.consume_fails {
            return Err(OverlayError::new(
                "tracking_unavailable",
                "synthetic cleanup failure",
            ));
        }
        if s.epoch != epoch {
            return Err(OverlayError::new("foreground_changed", "changed"));
        }
        s.target = None;
        Ok(())
    }
}
struct Sink(Arc<Mutex<State>>);
impl katran::text_fragments::ClipboardSink for Sink {
    fn write(&self, _: &str) -> Result<u32> {
        self.0.lock().unwrap().sequence = 77;
        Ok(77)
    }
}
fn reply(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "lease".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let value = reply(api, command);
    assert_eq!(value["ok"], true, "{value}");
    value["result"].clone()
}
fn setup() -> (tempfile::TempDir, Service, Arc<Mutex<State>>) {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let state = Arc::new(Mutex::new(State::default()));
    api.set_overlay_host(Box::new(Fake(state.clone())));
    api.set_text_clipboard_sink(Box::new(Sink(state.clone())));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let id = call(
        &mut api,
        Command::IngestText {
            text: "synthetic".into(),
            source: "fixture".into(),
        },
    )["id"]
        .as_i64()
        .unwrap();
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    (root, api, state)
}
fn begin(api: &mut Service) -> Value {
    reply(api, Command::BeginPastePrevious { owner_window: 8 })
}
fn finish(api: &mut Service) -> Value {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let r = reply(api, Command::CompletePastePrevious { owner_window: 8 });
        if r["result"]["state"] != "copying" {
            return r;
        }
        assert!(Instant::now() < deadline, "{r}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn last_external_visit_targets_b_without_summon_and_is_single_use() {
    let (_root, mut api, state) = setup();
    {
        let mut s = state.lock().unwrap();
        s.epoch = 2;
        s.target = Some(target(6));
    }
    assert_eq!(begin(&mut api)["ok"], true);
    let done = finish(&mut api);
    assert_eq!(done["result"]["state"], "sent", "{done}");
    assert_eq!(state.lock().unwrap().sent, vec![target(6)]);
    assert_eq!(finish(&mut api)["error"]["code"], "paste_not_prepared");
    {
        let mut s = state.lock().unwrap();
        s.foreground = 8;
    }
    assert_eq!(begin(&mut api)["error"]["code"], "no_previous_target");
    {
        let mut s = state.lock().unwrap();
        s.epoch = 3;
        s.target = Some(target(6));
    }
    assert_eq!(begin(&mut api)["ok"], true);
}
#[test]
fn lost_focus_even_after_return_cancels_old_lease() {
    let (_root, mut api, state) = setup();
    assert_eq!(begin(&mut api)["ok"], true);
    {
        let mut s = state.lock().unwrap();
        s.foreground = 8;
        s.epoch = 2;
        s.target = Some(target(6));
    }
    let done = finish(&mut api);
    assert_eq!(done["error"]["code"], "foreground_changed");
    assert!(state.lock().unwrap().sent.is_empty());
    assert_eq!(finish(&mut api)["error"]["code"], "paste_not_prepared");
}

#[test]
fn send_then_fresh_summon_from_same_external_window_can_paste_again() {
    let (_root, mut api, state) = setup();
    assert_eq!(begin(&mut api)["ok"], true);
    assert_eq!(finish(&mut api)["result"]["state"], "sent");
    {
        // Match native consume/visit: the old lease is retired while the
        // currently external window remains a valid new capture candidate.
        let mut s = state.lock().unwrap();
        assert_eq!(s.foreground, 7);
        s.epoch += 1;
        s.target = Some(target(7));
    }
    call(
        &mut api,
        Command::SummonCapturedOverlay {
            owner_window: 8,
            capture: SummonReceipt {
                previous: target(7),
                work_area: WorkArea {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                },
            },
        },
    );
    {
        let mut s = state.lock().unwrap();
        s.foreground = 8;
        s.epoch += 1; // the ensuing external→Katran activation is a new visit
    }
    assert_eq!(begin(&mut api)["ok"], true);
    assert_eq!(finish(&mut api)["result"]["state"], "sent");
    assert_eq!(state.lock().unwrap().sent, vec![target(7), target(7)]);
    assert_eq!(finish(&mut api)["error"]["code"], "paste_not_prepared");
}
#[test]
fn unavailable_observation_and_obsolete_capture_do_not_restore_old_target() {
    let (_root, mut api, state) = setup();
    {
        let mut s = state.lock().unwrap();
        s.target = Some(target(6));
        s.epoch = 2;
        s.missing = Some(6);
    }
    assert_eq!(begin(&mut api)["error"]["code"], "target_unavailable");
    {
        let mut s = state.lock().unwrap();
        s.missing = None;
    }
    let stale = reply(
        &mut api,
        Command::SummonCapturedOverlay {
            owner_window: 8,
            capture: SummonReceipt {
                previous: target(7),
                work_area: WorkArea {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                },
            },
        },
    );
    assert_eq!(stale["error"]["code"], "stale_capture");
    {
        let mut s = state.lock().unwrap();
        s.unavailable = true;
    }
    assert_eq!(begin(&mut api)["error"]["code"], "tracking_unavailable");
    assert!(state.lock().unwrap().sent.is_empty());
}
#[test]
fn activation_timeout_or_partial_send_never_retries_same_intent() {
    for partial in [false, true] {
        let (_root, mut api, state) = setup();
        {
            let mut s = state.lock().unwrap();
            s.fail_activate = !partial;
            s.fail_send = partial;
        }
        assert_eq!(begin(&mut api)["ok"], true);
        let done = finish(&mut api);
        assert_eq!(
            done["error"]["code"],
            if partial {
                "partial_input"
            } else {
                "activation_timeout"
            }
        );
        assert_eq!(state.lock().unwrap().sent.len(), usize::from(partial));
        assert_eq!(finish(&mut api)["error"]["code"], "paste_not_prepared");
    }
}

#[test]
fn successful_send_remains_sent_when_observer_cleanup_fails() {
    let (_root, mut api, state) = setup();
    state.lock().unwrap().consume_fails = true;
    assert_eq!(begin(&mut api)["ok"], true);
    let done = finish(&mut api);
    assert_eq!(done["result"]["state"], "sent", "{done}");
    assert_eq!(done["result"]["receipt"]["sent"], true);
    assert_eq!(
        done["result"]["receipt"]["tracking_warning"],
        "tracking_unavailable"
    );
    assert_eq!(state.lock().unwrap().sent.len(), 1);
    assert_eq!(finish(&mut api)["error"]["code"], "paste_not_prepared");
    state.lock().unwrap().foreground = 8;
    assert_eq!(begin(&mut api)["error"]["code"], "no_previous_target");
}

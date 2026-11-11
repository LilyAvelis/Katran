use katran::{
    Command, Request, Service,
    interaction::{SelectionMode, ShelfTab},
    overlay::{Host, OverlayError, Target, WorkArea},
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
#[derive(Default)]
struct State {
    sequence: u32,
    sent: usize,
    fail_copy: bool,
    text: String,
}
struct Sink(Arc<Mutex<State>>);
impl katran::text_fragments::ClipboardSink for Sink {
    fn write(&self, text: &str) -> katran::Result<u32> {
        let mut state = self.0.lock().unwrap();
        if state.fail_copy {
            return Err(katran::ApiError {
                code: "synthetic_copy_error".into(),
                message: "no copy".into(),
            });
        }
        state.sequence = 77;
        state.text = text.into();
        Ok(77)
    }
}
struct Fake(Arc<Mutex<State>>);
impl Host for Fake {
    fn foreground(&mut self) -> katran::overlay::Result<u64> {
        Ok(7)
    }
    fn identity(&mut self, h: u64) -> katran::overlay::Result<Target> {
        Ok(Target {
            hwnd: h,
            pid: h as u32,
            process_started: h,
        })
    }
    fn work_area(&mut self, _: u64) -> katran::overlay::Result<WorkArea> {
        Ok(WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        })
    }
    fn wait_modifiers_released(&mut self) -> katran::overlay::Result<()> {
        Ok(())
    }
    fn activate(&mut self, _: u64) -> katran::overlay::Result<()> {
        Ok(())
    }
    fn send_paste(&mut self, _: Target, sequence: u32) -> katran::overlay::Result<()> {
        let mut s = self.0.lock().unwrap();
        if s.sequence != sequence {
            return Err(OverlayError::new("clipboard_changed", "changed"));
        }
        s.sent += 1;
        Ok(())
    }
}
fn reply(api: &mut Service, c: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "paste".into(),
        command: c,
    })
}
fn call(api: &mut Service, c: Command) -> Value {
    let r = reply(api, c);
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn copy_must_succeed_before_paste_and_session_is_single_use() {
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 10000).unwrap();
        let state = Arc::new(Mutex::new(State {
            fail_copy: fail,
            ..Default::default()
        }));
        api.set_text_clipboard_sink(Box::new(Sink(state.clone())));
        api.set_overlay_host(Box::new(Fake(state.clone())));
        call(
            &mut api,
            Command::SetTabEnabled {
                tab: ShelfTab::Pods,
                enabled: true,
            },
        );
        let id = call(
            &mut api,
            Command::IngestText {
                text: "🦭".into(),
                source: "synthetic".into(),
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
        assert_eq!(
            reply(&mut api, Command::BeginPastePrevious { owner_window: 8 })["ok"],
            false
        );
        call(&mut api, Command::SummonOverlay { owner_window: 8 });
        call(&mut api, Command::BeginPastePrevious { owner_window: 8 });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let r = reply(&mut api, Command::CompletePastePrevious { owner_window: 8 });
            if r["result"]["state"] != "copying" {
                assert_eq!(r["ok"], !fail, "{r}");
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let s = state.lock().unwrap();
        assert_eq!(s.sent, usize::from(!fail));
        if !fail {
            assert_eq!(s.text, "🦭");
        }
        drop(s);
        assert_eq!(
            reply(&mut api, Command::CompletePastePrevious { owner_window: 8 })["ok"],
            false
        );
    }
}

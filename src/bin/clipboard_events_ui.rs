//! Event inbox and timer adapter; retry decisions belong to CaptureClipboard API.
use super::i18n::tr;
use super::*;
use katran::{capture_events::Trigger, clipboard_listener::Listener};

#[derive(Default)]
pub(super) struct State {
    listener: Option<Listener>,
    seen: u64,
    enabled: Option<bool>,
    pending: bool,
    retry: Option<(Instant, u64)>,
    failure: Option<String>,
}
impl State {
    pub fn start(ctx: &egui::Context, logger: Option<katran::diagnostics::Logger>) -> Self {
        let ctx = ctx.clone();
        match Listener::start(move || ctx.request_repaint(), logger) {
            Ok(listener) => Self {
                listener: Some(listener),
                ..Default::default()
            },
            Err(error) => Self {
                failure: Some(error),
                ..Default::default()
            },
        }
    }
    fn observe(&mut self, enabled: bool, generation: u64) {
        if self.enabled != Some(enabled) {
            self.enabled = Some(enabled);
            self.pending = enabled; // startup/enable reconciliation, once only
            self.retry = None;
        }
        if self.seen != generation {
            self.seen = generation;
            if enabled {
                self.pending = true;
                self.retry = None;
            }
        }
        if !enabled {
            self.pending = false;
            self.retry = None;
        }
    }
    fn collect(&mut self, enabled: bool) {
        let generation = if let Some(listener) = &self.listener {
            let signal = listener.signal();
            if let Some(error) = signal.take_failure() {
                self.failure = Some(error);
            }
            signal.generation()
        } else {
            self.seen
        };
        self.observe(enabled, generation);
    }
    fn next(&mut self, now: Instant) -> Option<Command> {
        if self.pending {
            self.pending = false;
            self.retry = None;
            Some(Command::CaptureClipboard {
                trigger: Trigger::Changed,
            })
        } else if self.retry.is_some_and(|(due, _)| due <= now) {
            let (_, ticket) = self.retry.take().unwrap();
            Some(Command::CaptureClipboard {
                trigger: Trigger::Retry { ticket },
            })
        } else {
            None
        }
    }
    pub fn completed(&mut self, result: &Value, now: Instant) {
        self.retry = if !self.pending {
            result["capture_schedule"]["retry_after_ms"]
                .as_u64()
                .zip(result["capture_schedule"]["ticket"].as_u64())
                .map(|(delay, ticket)| (now + Duration::from_millis(delay), ticket))
        } else {
            None
        };
    }
}

impl Shelf {
    /// Called by App::logic, also while the native window is minimized/hidden.
    /// No UI rendering, visibility, palette or Settings condition gates capture.
    pub(super) fn event_capture_tick(&mut self, ctx: &egui::Context) {
        self.clipboard_events.collect(self.capture);
        if let Some(error) = self.clipboard_events.failure.as_ref() {
            if self.api.is_some() && self.capture {
                let message = format!(
                    "{}: {error}",
                    tr(
                        "Подписка Clipboard недоступна; захват выключен",
                        "Clipboard listener unavailable; capture disabled"
                    )
                );
                self.call(Command::SetCaptureEnabled { enabled: false });
                self.capture = false;
                self.message = message;
            } else if self.message.is_empty() {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        "Подписка Clipboard недоступна",
                        "Clipboard listener unavailable"
                    )
                );
            }
            return;
        }
        if self.api.is_none() || self.native_drag.is_some() || self.capture_job.is_some() {
            return;
        }
        // Preserve the already captured copy selection before background reads.
        if self.job.is_none()
            && let Some(command) = self.take_queued_copy()
        {
            self.start_job(command, false, ctx.clone());
            return;
        }
        let now = Instant::now();
        if let Some(command) = self.clipboard_events.next(now) {
            self.start_job(command, true, ctx.clone());
        } else if let Some((due, _)) = self.clipboard_events.retry {
            ctx.request_repaint_after(due.saturating_duration_since(now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dirty_signal_coalesces_during_work_and_retries_never_create_periodic_poll() {
        let now = Instant::now();
        let mut inbox = State::default();
        inbox.observe(true, 0);
        assert!(matches!(
            inbox.next(now),
            Some(Command::CaptureClipboard {
                trigger: Trigger::Changed
            })
        ));
        assert!(inbox.next(now).is_none());
        inbox.observe(true, 1);
        inbox.observe(true, 2); // updates while one snapshot is running
        inbox.completed(
            &serde_json::json!({"capture_schedule":{"retry_after_ms":25,"ticket":1}}),
            now,
        );
        assert!(inbox.retry.is_none());
        assert!(matches!(
            inbox.next(now),
            Some(Command::CaptureClipboard {
                trigger: Trigger::Changed
            })
        ));
        assert!(inbox.next(now).is_none());
        inbox.completed(
            &serde_json::json!({"capture_schedule":{"retry_after_ms":25,"ticket":2}}),
            now,
        );
        assert!(inbox.next(now).is_none());
        assert!(matches!(
            inbox.next(now + Duration::from_millis(25)),
            Some(Command::CaptureClipboard {
                trigger: Trigger::Retry { ticket: 2 }
            })
        ));
        inbox.completed(
            &serde_json::json!({"captured":0,"capture_schedule":{"retry_after_ms":null}}),
            now,
        );
        assert!(inbox.next(now + Duration::from_secs(600)).is_none());
        inbox.observe(false, 3);
        assert!(inbox.next(now).is_none());
        inbox.observe(true, 3);
        assert!(inbox.next(now).is_some());
    }

    #[test]
    fn background_logic_captures_without_rendering_or_closing_dialogs() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Synthetic(Arc<AtomicUsize>);
        impl katran::capture_events::Source for Synthetic {
            fn capture(&mut self, service: &mut Service) -> katran::Result<Value> {
                self.0.fetch_add(1, Ordering::SeqCst);
                let ingested = service.execute(Request {
                    api_version: 1,
                    request_id: "synthetic-capture".into(),
                    command: Command::IngestBytes {
                        name: "synthetic.png".into(),
                        bytes: vec![1, 2, 3],
                        source: "synthetic".into(),
                    },
                });
                assert_eq!(ingested["ok"], true);
                Ok(serde_json::json!({"captured":1}))
            }
        }
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let calls = Arc::new(AtomicUsize::new(0));
        app.api
            .as_mut()
            .unwrap()
            .set_capture_source(Box::new(Synthetic(calls.clone())));
        app.call(Command::SetCaptureEnabled { enabled: true });
        app.capture = true;
        app.actions = true;
        app.save = true;
        app.call(Command::ShowSettings);
        let ctx = egui::Context::default();
        // No egui pass, painting, native window or system clipboard contents.
        app.event_capture_tick(&ctx);
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.api.is_none() && Instant::now() < deadline {
            app.finish_job();
            std::thread::yield_now();
        }
        assert!(app.api.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(app.events.len(), 1);
        assert!(app.actions && app.save);
        assert_eq!(app.call(Command::Interaction)["settings_open"], true);
        for _ in 0..100 {
            app.event_capture_tick(&ctx);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "idle ticks must not poll");
        assert!(app.job.is_none());
    }
    #[test]
    fn foreground_preview_does_not_gate_background_capture_submission() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Preview {
            started: mpsc::Sender<()>,
            release: mpsc::Receiver<()>,
        }
        impl katran::quicklook::Launcher for Preview {
            fn launch(&self, _: &std::path::Path) -> katran::Result<()> {
                self.started.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            }
        }
        struct Capture(Arc<AtomicUsize>);
        impl katran::capture_events::Source for Capture {
            fn capture(&mut self, _: &mut Service) -> katran::Result<Value> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::json!({"captured":0}))
            }
        }
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let id = app.call(Command::IngestBytes {
            name: "synthetic.pdf".into(),
            bytes: vec![1],
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        app.api
            .as_mut()
            .unwrap()
            .set_preview_launcher(Box::new(Preview {
                started: started_tx,
                release: release_rx,
            }));
        let ctx = egui::Context::default();
        assert!(app.start_job(Command::PreviewExternal { id }, false, ctx.clone()));
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        app.api
            .as_mut()
            .unwrap()
            .set_capture_source(Box::new(Capture(calls.clone())));
        app.call(Command::SetCaptureEnabled { enabled: true });
        app.capture = true;
        app.event_capture_tick(&ctx);
        let captured = calls.load(Ordering::SeqCst);
        let still_pending = app.job.is_some();
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.job.is_some() {
            app.finish_job();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(
            captured, 1,
            "Foreground preview prevented capture submission"
        );
        assert!(
            still_pending,
            "Fake preview must still be blocked while capture runs"
        );
    }
    #[test]
    fn listener_failure_disables_capture_instead_of_falling_back_to_polling() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::SetCaptureEnabled { enabled: true });
        app.capture = true;
        app.clipboard_events.failure = Some("synthetic registration failure".into());
        app.event_capture_tick(&egui::Context::default());
        assert!(!app.capture);
        assert!(app.job.is_none());
        assert_eq!(app.call(Command::Status)["capture_enabled"], false);
        assert!(app.message.contains("synthetic registration failure"));
    }
}

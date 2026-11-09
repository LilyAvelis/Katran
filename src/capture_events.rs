//! Event-triggered scheduling around the existing single-dispatcher snapshot API.
//! No clipboard reads in this policy; native and synthetic sources share it.
use crate::{Result, Service};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// At most eight attempts per event; no periodic work after success/exhaustion.
pub const RETRY_DELAYS_MS: [u64; 7] = [25, 50, 100, 200, 400, 400, 400];
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    Changed,
    Retry { ticket: u64 },
}
pub trait Clock: Send {
    fn now(&self) -> Duration;
}
pub struct SystemClock(Instant);
impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}
/// Injection boundary for synthetic capture tests/embedded clients. Native mode
/// delegates to the unchanged PollClipboard snapshot adapter.
pub trait Source: Send {
    fn capture(&mut self, service: &mut Service) -> Result<Value>;
}

#[derive(Default)]
pub(crate) struct Schedule {
    ticket: u64,
    attempts: usize,
    due: Option<Duration>,
}
impl Schedule {
    pub(crate) fn cancel(&mut self) {
        self.ticket = self.ticket.wrapping_add(1);
        self.attempts = 0;
        self.due = None;
    }
    fn status(&self, now: Duration, exhausted: bool, stale: bool) -> Value {
        json!({"ticket": self.ticket, "attempts": self.attempts,
            "retry_after_ms": self.due.map(|due| due.saturating_sub(now).as_millis() as u64),
            "exhausted": exhausted, "stale": stale})
    }
}
impl Service {
    /// Set before use; intended for deterministic policy tests, not a GUI clock.
    pub fn set_capture_clock(&mut self, clock: Box<dyn Clock>) {
        self.capture_clock = clock;
    }
    pub fn set_capture_source(&mut self, source: Box<dyn Source>) {
        self.capture_source = Some(source);
    }

    pub(crate) fn capture_from_event(&mut self, trigger: Trigger) -> Result<Value> {
        let now = self.capture_clock.now();
        if !self.capture_enabled {
            self.capture_events.cancel();
            return Ok(json!({"captured":0,"state":"paused","read_ok":true,
                "capture_schedule":self.capture_events.status(now,false,false)}));
        }
        match trigger {
            Trigger::Changed => self.capture_events.cancel(),
            Trigger::Retry { ticket } => {
                let schedule = &self.capture_events;
                if ticket != schedule.ticket || schedule.due.is_none() {
                    return Ok(json!({"captured":0,"read_ok":true,"state":"superseded",
                        "capture_schedule":schedule.status(now,false,true)}));
                }
                if schedule.due.is_some_and(|due| now < due) {
                    return Ok(json!({"captured":0,"read_ok":true,"state":"waiting",
                        "capture_schedule":schedule.status(now,false,false)}));
                }
            }
        }
        self.capture_events.attempts += 1;
        self.capture_events.due = None;
        let outcome = if let Some(mut source) = self.capture_source.take() {
            let outcome = source.capture(self);
            self.capture_source = Some(source);
            outcome
        } else {
            self.poll_clipboard()
        };
        let (mut result, retry) = match outcome {
            Ok(mut result) => {
                let retry = result["state"] == "retry";
                result["read_ok"] = json!(true);
                (result, retry)
            }
            Err(error) => {
                let retry = matches!(error.code.as_str(), "clipboard_busy" | "clipboard_error");
                (
                    json!({"captured":0,"read_ok":false,"capture_error":error}),
                    retry,
                )
            }
        };
        let now = self.capture_clock.now();
        let delay = retry
            .then(|| RETRY_DELAYS_MS.get(self.capture_events.attempts - 1))
            .flatten();
        if let Some(delay) = delay {
            self.capture_events.due = Some(now.saturating_add(Duration::from_millis(*delay)));
        }
        result["capture_schedule"] =
            self.capture_events
                .status(now, retry && delay.is_none(), false);
        Ok(result)
    }
}

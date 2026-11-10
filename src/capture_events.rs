//! Shared event-triggered capture scheduling for synchronous and asynchronous clients.
use crate::{Result, Service, capture::CaptureSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
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
/// Compatibility injection seam. Synthetic sources execute only on the dispatcher.
pub trait Source: Send {
    fn capture(&mut self, service: &mut Service) -> Result<Value>;
}
pub(crate) enum CaptureStart {
    Immediate(Value),
    Prepare { ticket: u64, last_sequence: u32 },
}
enum Attempt {
    Immediate(Value),
    Prepare(u64),
}
#[derive(Default)]
pub(crate) struct Schedule {
    ticket: u64,
    attempts: usize,
    due: Option<Duration>,
    pending: bool,
}
impl Schedule {
    pub(crate) fn cancel(&mut self) {
        self.ticket = self.ticket.wrapping_add(1);
        self.attempts = 0;
        self.due = None;
        self.pending = false;
    }
    fn status(&self, now: Duration, exhausted: bool, stale: bool) -> Value {
        json!({"ticket":self.ticket,"attempts":self.attempts,"retry_after_ms":self.due.map(|due|due.saturating_sub(now).as_millis() as u64),"exhausted":exhausted,"stale":stale})
    }
    fn immediate(&self, state: &str, now: Duration, stale: bool) -> Value {
        json!({"captured":0,"read_ok":true,"state":state,"capture_schedule":self.status(now,false,stale)})
    }
    fn begin(&mut self, enabled: bool, trigger: Trigger, now: Duration) -> Attempt {
        if !enabled {
            self.cancel();
            return Attempt::Immediate(self.immediate("paused", now, false));
        }
        match trigger {
            Trigger::Changed => self.cancel(),
            Trigger::Retry { ticket } => {
                if ticket != self.ticket || self.due.is_none() {
                    return Attempt::Immediate(self.immediate("superseded", now, true));
                }
                if self.due.is_some_and(|due| now < due) {
                    return Attempt::Immediate(self.immediate("waiting", now, false));
                }
            }
        }
        self.attempts += 1;
        self.due = None;
        self.pending = true;
        Attempt::Prepare(self.ticket)
    }
    fn accepts(&self, ticket: u64) -> bool {
        self.pending && self.ticket == ticket
    }
    fn finish(&mut self, ticket: u64, outcome: Result<Value>, now: Duration) -> Value {
        if !self.accepts(ticket) {
            return self.immediate("superseded", now, true);
        }
        self.pending = false;
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
        let delay = retry
            .then(|| RETRY_DELAYS_MS.get(self.attempts - 1))
            .flatten();
        if let Some(delay) = delay {
            self.due = Some(now.saturating_add(Duration::from_millis(*delay)));
        }
        result["capture_schedule"] = self.status(now, retry && delay.is_none(), false);
        result
    }
}
impl Service {
    pub fn set_capture_clock(&mut self, clock: Box<dyn Clock>) {
        self.capture_clock = clock;
    }
    pub fn set_capture_source(&mut self, source: Box<dyn Source>) {
        self.capture_source = Some(source);
    }
    pub(crate) fn begin_capture_event(&mut self, trigger: Trigger) -> Result<CaptureStart> {
        let ticket =
            match self
                .capture_events
                .begin(self.capture_enabled, trigger, self.capture_clock.now())
            {
                Attempt::Immediate(value) => return Ok(CaptureStart::Immediate(value)),
                Attempt::Prepare(ticket) => ticket,
            };
        if let Some(mut source) = self.capture_source.take() {
            let outcome = source.capture(self);
            self.capture_source = Some(source);
            let result = self
                .capture_events
                .finish(ticket, outcome, self.capture_clock.now());
            return Ok(CaptureStart::Immediate(result));
        }
        Ok(CaptureStart::Prepare {
            ticket,
            last_sequence: self.clipboard_sequence,
        })
    }
    pub(crate) fn complete_capture(
        &mut self,
        ticket: u64,
        snapshot: Result<CaptureSnapshot>,
    ) -> Result<Value> {
        self.complete_capture_at_sequence(ticket, snapshot, crate::capture::sequence_number())
    }
    fn complete_capture_at_sequence(
        &mut self,
        ticket: u64,
        snapshot: Result<CaptureSnapshot>,
        current_sequence: u32,
    ) -> Result<Value> {
        if !self.capture_enabled {
            return Ok(self
                .capture_events
                .immediate("paused", self.capture_clock.now(), false));
        }
        if !self.capture_events.accepts(ticket) {
            return Ok(self
                .capture_events
                .immediate("superseded", self.capture_clock.now(), true));
        }
        let outcome =
            snapshot.and_then(|snapshot| self.commit_capture_snapshot(snapshot, current_sequence));
        Ok(self
            .capture_events
            .finish(ticket, outcome, self.capture_clock.now()))
    }
    pub(crate) fn capture_from_event(&mut self, trigger: Trigger) -> Result<Value> {
        match self.begin_capture_event(trigger)? {
            CaptureStart::Immediate(result) => Ok(result),
            CaptureStart::Prepare {
                ticket,
                last_sequence,
            } => {
                let references = self.reference_paths()?;
                let snapshot = crate::capture::prepare_snapshot_for_ingest(
                    last_sequence,
                    &self.root,
                    self.quota,
                    &references,
                );
                self.complete_capture(ticket, snapshot)
            }
        }
    }
}

#[cfg(test)]
mod asynchronous_tests {
    use super::*;
    use crate::{
        capture::{CapturePayload, CaptureSnapshot},
        origin::Origin,
    };
    fn service() -> (tempfile::TempDir, Service) {
        let root = tempfile::tempdir().unwrap();
        let mut service = Service::open(root.path(), 1024).unwrap();
        service.capture_enabled = true;
        service.clipboard_sequence = 9;
        (root, service)
    }
    fn begin(service: &mut Service) -> u64 {
        match service.begin_capture_event(Trigger::Changed).unwrap() {
            CaptureStart::Prepare { ticket, .. } => ticket,
            CaptureStart::Immediate(_) => panic!("expected synthetic preparation"),
        }
    }
    fn image(service: &Service, sequence: u32) -> CaptureSnapshot {
        let prepared = crate::storage::prepare_bytes(
            &service.root,
            service.quota,
            crate::storage::ByteIngest {
                name: "synthetic.png",
                source: "clipboard_image",
                bytes: b"synthetic owned image bytes",
                origin: &Origin::default(),
            },
            &[],
        )
        .unwrap();
        CaptureSnapshot {
            sequence,
            payload: CapturePayload::PreparedImage(prepared),
        }
    }
    fn count(service: &Service) -> i64 {
        service
            .db
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap()
    }
    #[test]
    fn late_ticket_and_disabled_capture_discard_owned_receipts_without_ingesting() {
        let (_root, mut service) = service();
        let old = begin(&mut service);
        let snapshot = image(&service, 42);
        let latest = begin(&mut service);
        let result = service
            .complete_capture_at_sequence(old, Ok(snapshot), 42)
            .unwrap();
        assert_eq!(result["state"], "superseded");
        assert_eq!(count(&service), 0);
        let snapshot = image(&service, 42);
        service.capture_enabled = false;
        assert_eq!(
            service
                .complete_capture_at_sequence(latest, Ok(snapshot), 42)
                .unwrap()["state"],
            "paused"
        );
        assert_eq!(count(&service), 0);
        assert_eq!(
            std::fs::read_dir(service.root.join("blobs"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn changed_clipboard_sequence_requests_retry_without_publishing_staged_bytes() {
        let (_root, mut service) = service();
        let ticket = begin(&mut service);
        let snapshot = image(&service, 42);
        let result = service
            .complete_capture_at_sequence(ticket, Ok(snapshot), 43)
            .unwrap();
        assert_eq!(result["state"], "retry");
        assert_eq!(result["capture_schedule"]["retry_after_ms"], 25);
        assert_eq!(count(&service), 0);
        assert_eq!(service.clipboard_sequence, 9);
        assert_eq!(
            std::fs::read_dir(service.root.join("blobs"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn successful_capture_completion_is_consumed_once() {
        let (_root, mut service) = service();
        let ticket = begin(&mut service);
        let snapshot = image(&service, 42);
        let result = service
            .complete_capture_at_sequence(ticket, Ok(snapshot), 42)
            .unwrap();
        assert_eq!(result["captured"], 1);
        assert_eq!(count(&service), 1);
        assert_eq!(service.clipboard_sequence, 42);
        let duplicate = image(&service, 42);
        assert_eq!(
            service
                .complete_capture_at_sequence(ticket, Ok(duplicate), 42)
                .unwrap()["state"],
            "superseded"
        );
        assert_eq!(count(&service), 1);
    }
    #[test]
    fn prepared_file_batch_retains_per_item_failures_and_shelf_only_policy() {
        let (_root, mut service) = service();
        let originals = tempfile::tempdir().unwrap();
        let path = originals.path().join("synthetic.bin");
        std::fs::write(&path, b"external fixture").unwrap();
        let good = crate::jobs_service::prepare_reference(
            path.clone(),
            "clipboard_file".into(),
            Origin::default(),
        )
        .unwrap();
        let missing = originals.path().join("missing.bin");
        let bad = crate::jobs_service::prepare_reference(
            missing.clone(),
            "clipboard_file".into(),
            Origin::default(),
        );
        let ticket = begin(&mut service);
        let snapshot = CaptureSnapshot {
            sequence: 42,
            payload: CapturePayload::PreparedFiles {
                items: vec![(path.clone(), Ok(good)), (missing, bad)],
            },
        };
        let result = service
            .complete_capture_at_sequence(ticket, Ok(snapshot), 42)
            .unwrap();
        assert_eq!(result["captured"], 1);
        assert_eq!(result["items"][1]["ok"], false);
        assert_eq!(
            result["items"][0]["event"]["delete_policy"]["kind"],
            "shelf_only"
        );
        assert_eq!(std::fs::read(path).unwrap(), b"external fixture");
    }
}

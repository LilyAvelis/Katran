use katran::{
    ApiError,
    domain_jobs::{DomainJobs, JobId, JobState, Lane, MAX_ACTIVE_JOBS, MAX_RETAINED_JOBS},
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
fn error(code: &str) -> ApiError {
    ApiError {
        code: code.into(),
        message: code.into(),
    }
}
fn ready<T: Send + 'static>(jobs: &mut DomainJobs<T>, id: JobId) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if jobs.status(id).unwrap().state == JobState::Ready {
            return;
        }
        assert!(Instant::now() < deadline, "job timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn completion_is_owned_once_and_finish_cannot_be_replayed() {
    let mut jobs = DomainJobs::<u64>::new();
    let id = jobs.submit(Lane::Foreground, |_| Ok(42)).unwrap();
    assert!(jobs.finish(id, Ok(json!("forged early finish"))).is_err());
    ready(&mut jobs, id);
    let completed = jobs.take_ready(255);
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].0, id);
    assert_eq!(*completed[0].1.as_ref().unwrap(), 42);
    assert!(jobs.take_ready(8).is_empty());
    assert_eq!(jobs.cancel(id).unwrap_err().code, "job_already_claimed");
    let status = jobs.finish(id, Ok(json!({"committed":42}))).unwrap();
    assert_eq!(status.state, JobState::Succeeded);
    assert!(jobs.finish(id, Ok(json!({"committed":99}))).is_err());
    assert_eq!(jobs.status(id).unwrap().result.unwrap()["committed"], 42);
    assert_eq!(jobs.stats().active, 0);
}
#[test]
fn blocked_update_keeps_copy_and_capture_workers_available() {
    let mut jobs = DomainJobs::<u64>::new();
    let (tx, rx) = mpsc::channel();
    let update = jobs
        .submit(Lane::Update, move |_| {
            rx.recv().unwrap();
            Ok(1)
        })
        .unwrap();
    let foreground = jobs.submit(Lane::Foreground, |_| Ok(2)).unwrap();
    let capture = jobs.submit(Lane::Capture, |_| Ok(3)).unwrap();
    ready(&mut jobs, foreground);
    ready(&mut jobs, capture);
    assert_ne!(jobs.status(update).unwrap().state, JobState::Ready);
    tx.send(()).unwrap();
    ready(&mut jobs, update);
}
#[test]
fn blocked_discovery_cannot_take_foreground_or_capture_workers() {
    let mut jobs = DomainJobs::<u64>::new();
    let (release, blocked) = mpsc::channel();
    let (started, observed) = mpsc::channel();
    let discovery = jobs
        .submit(Lane::Discovery, move |_| {
            started.send(()).unwrap();
            blocked
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| error("fixture_timeout"))?;
            Ok(1)
        })
        .unwrap();
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    let foreground = jobs.submit(Lane::Foreground, |_| Ok(2)).unwrap();
    let capture = jobs.submit(Lane::Capture, |_| Ok(3)).unwrap();
    ready(&mut jobs, foreground);
    ready(&mut jobs, capture);
    assert_eq!(jobs.status(discovery).unwrap().state, JobState::Running);
    assert_eq!(jobs.stats().workers, 3);
    release.send(()).unwrap();
}
#[test]
fn cancellation_keeps_inflight_capacity_and_skips_queued_work() {
    let mut jobs = DomainJobs::<u64>::new();
    let (release, blocked) = mpsc::channel();
    let (started, observed) = mpsc::channel();
    let first = jobs
        .submit(Lane::Discovery, move |_| {
            started.send(()).unwrap();
            blocked.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(1)
        })
        .unwrap();
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    let called = Arc::new(AtomicUsize::new(0));
    let invoked = Arc::clone(&called);
    let second = jobs
        .submit(Lane::Discovery, move |_| {
            invoked.fetch_add(1, Ordering::Relaxed);
            Ok(2)
        })
        .unwrap();
    assert_eq!(
        jobs.submit(Lane::Discovery, |_| Ok(3)).unwrap_err().code,
        "job_capacity"
    );
    assert_eq!(jobs.cancel(second).unwrap().state, JobState::Cancelled);
    assert_eq!(jobs.cancel(first).unwrap().state, JobState::Cancelled);
    assert_eq!(
        jobs.stats().active,
        2,
        "cancellation must not spawn replacement work around stuck I/O"
    );
    assert!(jobs.submit(Lane::Discovery, |_| Ok(4)).is_err());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while jobs.stats().active != 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(called.load(Ordering::Relaxed), 0);
    assert!(jobs.take_ready(8).is_empty());
}
#[test]
fn cancelled_ready_resources_are_dropped_and_never_delivered() {
    struct Prepared(Arc<AtomicUsize>);
    impl Drop for Prepared {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let dropped = Arc::new(AtomicUsize::new(0));
    let prepared = Arc::clone(&dropped);
    let mut jobs = DomainJobs::new();
    let id = jobs
        .submit(Lane::Foreground, move |_| Ok(Prepared(prepared)))
        .unwrap();
    ready(&mut jobs, id);
    jobs.cancel(id).unwrap();
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
    assert!(jobs.take_ready(8).is_empty());
}
#[test]
fn worker_panic_is_a_typed_failure_and_lane_still_progresses() {
    let mut jobs = DomainJobs::<u64>::new();
    let first = jobs
        .submit(Lane::Capture, |_| panic!("synthetic worker panic"))
        .unwrap();
    ready(&mut jobs, first);
    let (_, result) = jobs.take_ready(1).pop().unwrap();
    let failure = result.unwrap_err();
    assert_eq!(failure.code, "job_worker_panic");
    assert_eq!(
        jobs.finish(first, Err(failure)).unwrap().state,
        JobState::Failed
    );
    let second = jobs.submit(Lane::Capture, |_| Ok(7)).unwrap();
    ready(&mut jobs, second);
    assert_eq!(jobs.take_ready(1).pop().unwrap().1.unwrap(), 7);
}
#[test]
fn fresh_validation_failure_can_finish_without_publishing_preparation() {
    let mut jobs = DomainJobs::new();
    let id = jobs
        .submit(Lane::Foreground, |_| Ok((7_u64, "owned prepared fixture")))
        .unwrap();
    ready(&mut jobs, id);
    let (id, result) = jobs.take_ready(1).pop().unwrap();
    let (expected_revision, _prepared) = result.unwrap();
    let current_revision = 8;
    assert_ne!(expected_revision, current_revision);
    let status = jobs.finish(id, Err(error("stale_selection"))).unwrap();
    assert_eq!(status.state, JobState::Failed);
    assert_eq!(status.error.unwrap().code, "stale_selection");
}
#[test]
fn terminal_status_retention_and_admission_are_bounded() {
    let mut jobs = DomainJobs::<u64>::new();
    let mut first = None;
    for value in 0..(MAX_RETAINED_JOBS + 10) {
        let id = jobs
            .submit(Lane::Foreground, move |_| Ok(value as u64))
            .unwrap();
        first.get_or_insert(id);
        ready(&mut jobs, id);
        jobs.take_ready(8);
        jobs.finish(id, Ok(json!(value))).unwrap();
        let stats = jobs.stats();
        assert!(stats.active <= MAX_ACTIVE_JOBS);
        assert!(stats.retained <= MAX_RETAINED_JOBS);
        assert!(stats.workers <= 3);
    }
    assert_eq!(jobs.stats().retained, MAX_RETAINED_JOBS);
    assert_eq!(jobs.status(first.unwrap()).unwrap_err().code, "unknown_job");
}
#[test]
fn commit_polling_is_bounded_and_fair_across_lanes() {
    let mut jobs = DomainJobs::<u64>::new();
    let mut submitted = Vec::new();
    for (lane, value) in [
        (Lane::Foreground, 1),
        (Lane::Foreground, 2),
        (Lane::Capture, 3),
        (Lane::Discovery, 4),
    ] {
        let id = jobs.submit(lane, move |_| Ok(value)).unwrap();
        submitted.push(id);
    }
    for id in submitted {
        ready(&mut jobs, id);
    }
    let values: Vec<_> = jobs
        .take_ready(255)
        .into_iter()
        .map(|(_, result)| result.unwrap())
        .collect();
    assert_eq!(values, vec![1, 3, 2, 4]);
    assert!(values.len() <= 8);
}
#[test]
fn cancellation_effect_fence_never_reports_cancelled_after_effect_wins() {
    for _ in 0..12 {
        let mut jobs = DomainJobs::<bool>::new();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let worker_barrier = Arc::clone(&barrier);
        let effected = Arc::new(AtomicUsize::new(0));
        let worker_effected = Arc::clone(&effected);
        let id = jobs
            .submit(Lane::Foreground, move |cancel| {
                worker_barrier.wait();
                cancel.begin_effect()?;
                worker_effected.fetch_add(1, Ordering::SeqCst);
                Ok(true)
            })
            .unwrap();
        barrier.wait();
        match jobs.cancel(id) {
            Ok(_) => {
                let deadline = Instant::now() + Duration::from_secs(5);
                while jobs.stats().active != 0 {
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
                assert_eq!(jobs.status(id).unwrap().state, JobState::Cancelled);
                assert_eq!(effected.load(Ordering::SeqCst), 0);
            }
            Err(error) => {
                assert_eq!(error.code, "job_not_cancellable");
                ready(&mut jobs, id);
                let (_, result) = jobs.take_ready(1).pop().unwrap();
                assert!(result.unwrap());
                jobs.finish(id, Ok(json!({"effect":true}))).unwrap();
                assert_eq!(effected.load(Ordering::SeqCst), 1);
            }
        }
    }
}
#[test]
fn private_preparation_continuation_keeps_id_capacity_and_effect_fence() {
    let mut jobs = DomainJobs::<u64>::new();
    let id = jobs.submit(Lane::Foreground, |_| Ok(1)).unwrap();
    ready(&mut jobs, id);
    let (claimed, first) = jobs.take_ready(1).pop().unwrap();
    assert_eq!(claimed, id);
    assert_eq!(first.unwrap(), 1);
    jobs.continue_claimed(id, |cancel| {
        cancel.begin_effect()?;
        Ok(2)
    })
    .unwrap();
    assert_eq!(jobs.stats().active, 1);
    ready(&mut jobs, id);
    assert_eq!(jobs.cancel(id).unwrap_err().code, "job_not_cancellable");
    let (continued, second) = jobs.take_ready(1).pop().unwrap();
    assert_eq!(continued, id);
    assert_eq!(second.unwrap(), 2);
    jobs.finish(id, Ok(json!({"done":true}))).unwrap();
    assert!(jobs.continue_claimed(id, |_| Ok(3)).is_err());
    assert_eq!(jobs.stats().active, 0);
}

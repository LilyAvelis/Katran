//! Bounded domain preparation lanes. Workers own immutable inputs, never a Service.
//!
//! Only trusted in-process code submits closures or receives T; neither crosses
//! the wire. The dispatcher validates freshness after `take_ready`, commits once,
//! then calls `finish`. Cancellation is cooperative preparation cancellation; it
//! cannot roll back a publication or other irreversible effect. Callers must
//! bound the bytes owned by each input/result using their existing domain quotas.
use crate::{ApiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
};

pub const MAX_ACTIVE_JOBS: usize = 12;
pub const MAX_RETAINED_JOBS: usize = 64;
const MAX_COMPLETIONS_PER_POLL: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Foreground,
    Capture,
    Discovery,
}
impl Lane {
    fn index(self) -> usize {
        match self {
            Self::Foreground => 0,
            Self::Capture => 1,
            Self::Discovery => 2,
        }
    }
    fn capacity(self) -> usize {
        match self {
            Self::Foreground => 8,
            Self::Capture | Self::Discovery => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Ready,
    Succeeded,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
}
impl From<ApiError> for JobError {
    fn from(error: ApiError) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobStatus {
    pub job_id: JobId,
    pub lane: Lane,
    pub state: JobState,
    pub result: Option<Value>,
    pub error: Option<JobError>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct JobStats {
    pub active: usize,
    pub retained: usize,
    pub workers: usize,
}
#[derive(Clone)]
pub struct Cancellation(Arc<AtomicU8>);
impl Cancellation {
    /// Fence an intentional external effect. Once begun, cancellation must not
    /// claim that an export/preview request was undone.
    pub fn begin_effect(&self) -> Result<()> {
        match self
            .0
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) | Err(2) => Ok(()),
            Err(_) => Err(ApiError::new(
                "job_cancelled",
                "Cancelled before external effect",
            )),
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire) == 1
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(ApiError::new("job_cancelled", "Preparation was cancelled"))
        } else {
            Ok(())
        }
    }
}
type Work<T> = Box<dyn FnOnce(Cancellation) -> Result<T> + Send + 'static>;
struct Task<T> {
    id: JobId,
    cancellation: Cancellation,
    phase: Arc<AtomicU8>,
    work: Work<T>,
}
struct Worker<T> {
    sender: mpsc::SyncSender<Task<T>>,
}
struct Entry<T> {
    status: JobStatus,
    cancellation: Cancellation,
    phase: Arc<AtomicU8>,
    pending_worker: bool,
    outstanding: bool,
    claimed: bool,
    prepared: Option<Result<T>>,
}
pub struct DomainJobs<T: Send + 'static> {
    entries: BTreeMap<JobId, Entry<T>>,
    terminal: VecDeque<JobId>,
    workers: [Option<Worker<T>>; 3],
    sender: mpsc::SyncSender<(JobId, Result<T>)>,
    receiver: mpsc::Receiver<(JobId, Result<T>)>,
    next_id: u64,
    fairness: usize,
}
impl<T: Send + 'static> Default for DomainJobs<T> {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel(MAX_ACTIVE_JOBS);
        Self {
            entries: BTreeMap::new(),
            terminal: VecDeque::new(),
            workers: std::array::from_fn(|_| None),
            sender,
            receiver,
            next_id: 1,
            fairness: 0,
        }
    }
}
impl<T: Send + 'static> DomainJobs<T> {
    pub fn new() -> Self {
        Self::default()
    }
    fn ensure_worker(&mut self, lane: Lane) -> Result<()> {
        if self.workers[lane.index()].is_some() {
            return Ok(());
        }
        let (sender, receiver) = mpsc::sync_channel::<Task<T>>(lane.capacity());
        let completed = self.sender.clone();
        std::thread::Builder::new()
            .name(format!("katran-job-{lane:?}"))
            .spawn(move || {
                while let Ok(task) = receiver.recv() {
                    let result = if task.cancellation.is_cancelled() {
                        Err(ApiError::new(
                            "job_cancelled",
                            "Preparation was cancelled before starting",
                        ))
                    } else {
                        task.phase.store(1, Ordering::Release);
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            (task.work)(task.cancellation.clone())
                        })) {
                            Ok(result) => result,
                            Err(_) => Err(ApiError::new(
                                "job_worker_panic",
                                "Preparation worker failed; no automatic retry",
                            )),
                        }
                    };
                    let result = if task.cancellation.is_cancelled() {
                        drop(result);
                        Err(ApiError::new("job_cancelled", "Preparation was cancelled"))
                    } else {
                        result
                    };
                    task.phase.store(2, Ordering::Release);
                    if completed.send((task.id, result)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| ApiError::new("job_worker_error", error))?;
        self.workers[lane.index()] = Some(Worker { sender });
        Ok(())
    }
    /// Dispatcher preflight before an immediate command with side effects.
    /// The caller must not submit another job between this check and admission.
    pub fn check_admission(&mut self, lane: Lane) -> Result<()> {
        self.collect();
        let active = self
            .entries
            .values()
            .filter(|entry| entry.outstanding)
            .count();
        let lane_active = self
            .entries
            .values()
            .filter(|entry| entry.outstanding && entry.status.lane == lane)
            .count();
        if active >= MAX_ACTIVE_JOBS || lane_active >= lane.capacity() {
            return Err(ApiError::new(
                "job_capacity",
                "Preparation lane is full; retry after completion",
            ));
        }
        self.ensure_worker(lane)?;
        Ok(())
    }
    pub fn submit(
        &mut self,
        lane: Lane,
        work: impl FnOnce(Cancellation) -> Result<T> + Send + 'static,
    ) -> Result<JobId> {
        self.check_admission(lane)?;
        let id = JobId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| ApiError::new("job_id_exhausted", "Job identifier exhausted"))?;
        let cancellation = Cancellation(Arc::new(AtomicU8::new(0)));
        let phase = Arc::new(AtomicU8::new(0));
        let task = Task {
            id,
            cancellation: cancellation.clone(),
            phase: Arc::clone(&phase),
            work: Box::new(work),
        };
        self.workers[lane.index()]
            .as_ref()
            .expect("worker initialized")
            .sender
            .try_send(task)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    ApiError::new("job_capacity", "Preparation queue is full")
                }
                mpsc::TrySendError::Disconnected(_) => {
                    ApiError::new("job_worker_error", "Preparation worker is unavailable")
                }
            })?;
        self.entries.insert(
            id,
            Entry {
                status: JobStatus {
                    job_id: id,
                    lane,
                    state: JobState::Queued,
                    result: None,
                    error: None,
                },
                cancellation,
                phase,
                pending_worker: true,
                outstanding: true,
                claimed: false,
                prepared: None,
            },
        );
        Ok(id)
    }
    fn collect(&mut self) {
        while let Ok((id, result)) = self.receiver.try_recv() {
            let Some(entry) = self.entries.get_mut(&id) else {
                continue;
            };
            entry.pending_worker = false;
            if entry.cancellation.is_cancelled() {
                drop(result);
                entry.outstanding = false;
                self.retain_terminal(id);
            } else {
                entry.prepared = Some(result);
                entry.status.state = JobState::Ready;
            }
        }
    }
    fn retain_terminal(&mut self, id: JobId) {
        self.terminal.push_back(id);
        while self.terminal.len() > MAX_RETAINED_JOBS {
            if let Some(expired) = self.terminal.pop_front() {
                self.entries.remove(&expired);
            }
        }
    }
    /// Results are consumed once. Revalidate revisions/generations/clipboard
    /// sequence on the dispatcher immediately before the corresponding commit.
    /// Until `finish`, a claimed result still occupies its lane admission slot.
    pub fn take_ready(&mut self, max: usize) -> Vec<(JobId, Result<T>)> {
        self.collect();
        let order = [
            Lane::Foreground,
            Lane::Capture,
            Lane::Foreground,
            Lane::Discovery,
        ];
        let mut output = Vec::new();
        for _ in 0..max.min(MAX_COMPLETIONS_PER_POLL) {
            let mut selected = None;
            for _ in 0..order.len() {
                let lane = order[self.fairness];
                self.fairness = (self.fairness + 1) % order.len();
                selected = self.entries.iter().find_map(|(id, entry)| {
                    (entry.status.lane == lane && entry.prepared.is_some()).then_some(*id)
                });
                if selected.is_some() {
                    break;
                }
            }
            let Some(id) = selected else {
                break;
            };
            let entry = self.entries.get_mut(&id).expect("selected entry exists");
            entry.claimed = true;
            output.push((id, entry.prepared.take().expect("selected result exists")));
        }
        output
    }
    /// Authorize the irreversible phase on the dispatcher after freshness checks.
    /// Later settings changes cannot undo an action that crossed this fence.
    pub fn authorize_claimed_effect(&mut self, id: JobId) -> Result<()> {
        let entry = self.entries.get(&id).ok_or_else(|| {
            ApiError::new(
                "unknown_job",
                "Job is unknown or its retained status expired",
            )
        })?;
        if !entry.claimed || !entry.outstanding {
            return Err(ApiError::new(
                "job_not_claimed",
                "Only a claimed preparation can be authorized",
            ));
        }
        entry.cancellation.begin_effect()
    }
    /// A validated preparation may continue on its original worker lane under
    /// the same ID/admission slot. The dispatcher must finish an error if this
    /// transition fails; it must never repeat an uncertain external effect.
    pub fn continue_claimed(
        &mut self,
        id: JobId,
        work: impl FnOnce(Cancellation) -> Result<T> + Send + 'static,
    ) -> Result<()> {
        let entry = self.entries.get_mut(&id).ok_or_else(|| {
            ApiError::new(
                "unknown_job",
                "Job is unknown or its retained status expired",
            )
        })?;
        if !entry.claimed || !entry.outstanding || entry.pending_worker {
            return Err(ApiError::new(
                "job_not_claimed",
                "Only a claimed preparation can continue",
            ));
        }
        let lane = entry.status.lane;
        entry.phase.store(0, Ordering::Release);
        let task = Task {
            id,
            cancellation: entry.cancellation.clone(),
            phase: Arc::clone(&entry.phase),
            work: Box::new(work),
        };
        self.workers[lane.index()]
            .as_ref()
            .expect("claimed job owns a worker lane")
            .sender
            .try_send(task)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    ApiError::new("job_capacity", "Continuation lane is full")
                }
                mpsc::TrySendError::Disconnected(_) => {
                    ApiError::new("job_worker_error", "Continuation worker is unavailable")
                }
            })?;
        entry.claimed = false;
        entry.pending_worker = true;
        entry.status.state = JobState::Queued;
        entry.status.result = None;
        entry.status.error = None;
        Ok(())
    }
    pub fn finish(&mut self, id: JobId, result: Result<Value>) -> Result<JobStatus> {
        let entry = self.entries.get_mut(&id).ok_or_else(|| {
            ApiError::new(
                "unknown_job",
                "Job is unknown or its retained status expired",
            )
        })?;
        if !entry.claimed || !entry.outstanding {
            return Err(ApiError::new(
                "job_not_claimed",
                "Job must be claimed exactly once before completion",
            ));
        }
        entry.outstanding = false;
        match result {
            Ok(result) => {
                entry.status.state = JobState::Succeeded;
                entry.status.result = Some(result);
            }
            Err(error) => {
                entry.status.state = JobState::Failed;
                entry.status.error = Some(error.into());
            }
        }
        let status = entry.status.clone();
        self.retain_terminal(id);
        Ok(status)
    }
    pub fn status(&mut self, id: JobId) -> Result<JobStatus> {
        self.collect();
        let entry = self.entries.get(&id).ok_or_else(|| {
            ApiError::new(
                "unknown_job",
                "Job is unknown or its retained status expired",
            )
        })?;
        let mut status = entry.status.clone();
        if status.state == JobState::Queued && entry.phase.load(Ordering::Acquire) != 0 {
            status.state = JobState::Running;
        }
        Ok(status)
    }
    pub fn cancel(&mut self, id: JobId) -> Result<JobStatus> {
        self.collect();
        let entry = self.entries.get_mut(&id).ok_or_else(|| {
            ApiError::new(
                "unknown_job",
                "Job is unknown or its retained status expired",
            )
        })?;
        if entry.claimed && entry.outstanding {
            return Err(ApiError::new(
                "job_already_claimed",
                "Commit has been claimed; cancellation cannot undo effects",
            ));
        }
        if !entry.outstanding {
            return Ok(entry.status.clone());
        }
        if entry
            .cancellation
            .0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            == Err(2)
        {
            return Err(ApiError::new(
                "job_not_cancellable",
                "External effect has begun; cancellation cannot undo it",
            ));
        }
        entry.status.state = JobState::Cancelled;
        entry.status.error = Some(JobError {
            code: "job_cancelled".into(),
            message: "Preparation was cancelled".into(),
        });
        drop(entry.prepared.take());
        let terminal = !entry.pending_worker;
        if terminal {
            entry.outstanding = false;
        }
        let status = entry.status.clone();
        if terminal {
            self.retain_terminal(id);
        }
        Ok(status)
    }
    pub fn stats(&mut self) -> JobStats {
        self.collect();
        JobStats {
            active: self
                .entries
                .values()
                .filter(|entry| entry.outstanding)
                .count(),
            retained: self.terminal.len(),
            workers: self
                .workers
                .iter()
                .filter(|worker| worker.is_some())
                .count(),
        }
    }
}
impl<T: Send + 'static> Drop for DomainJobs<T> {
    fn drop(&mut self) {
        for entry in self.entries.values() {
            let _ =
                entry
                    .cancellation
                    .0
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
        }
        // Dropping senders/receiver releases workers without waiting for I/O.
    }
}

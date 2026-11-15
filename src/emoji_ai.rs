//! Local emoji search worker adapter. Never logs query text.
mod runtime;
use crate::{ApiError, Result};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

pub(crate) fn status(profile_root: &std::path::Path) -> Result<Value> {
    runtime::status(profile_root)?;
    Ok(json!({"available": true}))
}

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FRAME: u64 = 1024 * 1024;

/// Boundary is injectable so API tests never launch a model or touch user data.
pub trait Backend: Send {
    fn search(&mut self, query: &str, offset: u32, limit: u32) -> Result<Value>;
    fn invalidate(&mut self) {}
    /// Implementations that can interrupt work should override this method.
    /// The default preserves compatibility but cannot interrupt a blocked backend.
    fn search_cancellable(
        &mut self,
        query: &str,
        offset: u32,
        limit: u32,
        cancel: &crate::domain_jobs::Cancellation,
    ) -> Result<Value> {
        cancel.check()?;
        let result = self.search(query, offset, limit);
        cancel.check()?;
        result
    }
}

pub struct Resident {
    profile_root: std::path::PathBuf,
    process: Option<Worker>,
    next_id: u64,
}
struct Worker {
    child: Child,
    writer: SyncSender<(Vec<u8>, SyncSender<bool>)>,
    reader: Receiver<Option<Value>>,
}

impl Worker {
    fn start(
        profile_root: &std::path::Path,
        cancel: Option<&crate::domain_jobs::Cancellation>,
    ) -> Result<Self> {
        let runtime = runtime::resolve(profile_root, cancel)?;
        let mut command = Command::new(runtime.python);
        command
            .arg(runtime.worker)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| {
            ApiError::new(
                "emoji_ai_unavailable",
                "Local emoji AI worker could not start",
            )
        })?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (writer, writes) = mpsc::sync_channel::<(Vec<u8>, SyncSender<bool>)>(1);
        std::thread::spawn(move || {
            for (bytes, ack) in writes {
                let ok = stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_ok();
                let _ = ack.send(ok);
                if !ok {
                    break;
                }
            }
        });
        let (frames, reader) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let mut data = Vec::new();
                let result = (&mut input)
                    .take(MAX_FRAME + 1)
                    .read_until(b'\n', &mut data);
                if result.is_err()
                    || data.is_empty()
                    || data.len() as u64 > MAX_FRAME
                    || data.last() != Some(&b'\n')
                {
                    let _ = frames.send(None);
                    break;
                }
                let frame = serde_json::from_slice::<Value>(&data).ok();
                if frames.send(frame).is_err() {
                    break;
                }
            }
        });
        let worker = Self {
            child,
            writer,
            reader,
        };
        let ready = worker.receive(STARTUP_TIMEOUT, cancel)?;
        if ready["type"] != "ready"
            || ready["protocol"] != 1
            || ready["count"].as_u64() != Some(crate::pod_metadata::emoji_entries().len() as u64)
        {
            return Err(ApiError::new(
                "emoji_ai_unavailable",
                "Local emoji AI initialization failed",
            ));
        }
        Ok(worker)
    }
    fn receive(
        &self,
        timeout: Duration,
        cancel: Option<&crate::domain_jobs::Cancellation>,
    ) -> Result<Value> {
        wait_for(&self.reader, timeout, cancel)?.ok_or_else(|| {
            ApiError::new(
                "emoji_ai_unavailable",
                "Local emoji AI worker did not respond",
            )
        })
    }
    fn send(
        &self,
        value: &Value,
        timeout: Duration,
        cancel: Option<&crate::domain_jobs::Cancellation>,
    ) -> Result<()> {
        let mut bytes = serde_json::to_vec(value)
            .map_err(|_| ApiError::new("emoji_ai_protocol", "Cannot encode request"))?;
        bytes.push(b'\n');
        let (ack, received) = mpsc::sync_channel(1);
        self.writer.try_send((bytes, ack)).map_err(|_| {
            ApiError::new(
                "emoji_ai_unavailable",
                "Local emoji AI worker is unavailable",
            )
        })?;
        if !wait_for(&received, timeout, cancel)? {
            return Err(ApiError::new(
                "emoji_ai_unavailable",
                "Local emoji AI worker did not accept request",
            ));
        }
        Ok(())
    }
}
// Poll cancellation without adding a thread for each request. Deadlines cover both
// startup and requests; dropping a failed Worker kills and reaps the child.
fn wait_for<T>(
    receiver: &Receiver<T>,
    timeout: Duration,
    cancel: Option<&crate::domain_jobs::Cancellation>,
) -> Result<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ApiError::new(
                "emoji_ai_unavailable",
                "Local emoji AI worker did not respond",
            ));
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(25))) {
            Ok(value) => return Ok(value),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(ApiError::new(
                    "emoji_ai_unavailable",
                    "Local emoji AI worker is unavailable",
                ));
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Close/kill on any timeout or protocol error; always reap the child.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Backend for Resident {
    fn invalidate(&mut self) {
        self.process = None;
    }
    fn search(&mut self, query: &str, offset: u32, limit: u32) -> Result<Value> {
        self.search_inner(query, offset, limit, None)
    }
    fn search_cancellable(
        &mut self,
        query: &str,
        offset: u32,
        limit: u32,
        cancel: &crate::domain_jobs::Cancellation,
    ) -> Result<Value> {
        self.search_inner(query, offset, limit, Some(cancel))
    }
}
impl Resident {
    pub fn new(profile_root: std::path::PathBuf) -> Self {
        Self {
            profile_root,
            process: None,
            next_id: 0,
        }
    }
    fn search_inner(
        &mut self,
        query: &str,
        offset: u32,
        limit: u32,
        cancel: Option<&crate::domain_jobs::Cancellation>,
    ) -> Result<Value> {
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        if self.process.is_none() {
            self.process = Some(Worker::start(&self.profile_root, cancel)?);
        }
        self.next_id = self.next_id % 9_007_199_254_740_991 + 1;
        let id = self.next_id;
        let worker = self.process.as_ref().expect("started");
        let response = (|| {
            worker.send(
                &json!({"id":id,"op":"search","query":query,"offset":offset,"limit":limit}),
                REQUEST_TIMEOUT,
                cancel,
            )?;
            let frame = worker.receive(REQUEST_TIMEOUT, cancel)?;
            if frame["id"].as_u64() != Some(id) || frame["ok"].as_bool().is_none() {
                return Err(ApiError::new(
                    "emoji_ai_protocol",
                    "Invalid local emoji AI response",
                ));
            }
            if frame["ok"] != true {
                return Err(ApiError::new(
                    "emoji_ai_search_failed",
                    "Local emoji AI search failed",
                ));
            }
            Ok(frame["result"].clone())
        })();
        if response.is_err() {
            self.process = None;
        }
        response
    }
}

pub(crate) fn validate(query: &str, offset: u32, limit: u32) -> Result<()> {
    let count = crate::pod_metadata::emoji_entries().len();
    if query.trim().is_empty()
        || query.chars().count() > 512
        || offset as usize >= count
        || !(1..=100).contains(&limit)
    {
        return Err(ApiError::new(
            "invalid_argument",
            "Invalid emoji AI search parameters",
        ));
    }
    Ok(())
}

pub(crate) fn search(
    backend: &mut dyn Backend,
    query: &str,
    offset: u32,
    limit: u32,
) -> Result<Value> {
    validate(query, offset, limit)?;
    let count = crate::pod_metadata::emoji_entries().len();
    let result = backend.search(query, offset, limit)?;
    let checked = validate_result(result, count, offset, limit);
    if checked.is_err() {
        backend.invalidate();
    }
    checked
}

pub(crate) fn search_cancellable(
    backend: &mut dyn Backend,
    query: &str,
    offset: u32,
    limit: u32,
    cancel: &crate::domain_jobs::Cancellation,
) -> Result<Value> {
    validate(query, offset, limit)?;
    let result = backend.search_cancellable(query, offset, limit, cancel)?;
    cancel.check()?;
    let checked = validate_result(
        result,
        crate::pod_metadata::emoji_entries().len(),
        offset,
        limit,
    );
    if checked.is_err() {
        backend.invalidate();
    }
    checked
}

fn validate_result(result: Value, count: usize, offset: u32, limit: u32) -> Result<Value> {
    let invalid = || ApiError::new("emoji_ai_protocol", "Invalid local emoji AI result");
    let rows = result["results"].as_array().ok_or_else(invalid)?;
    let expected_len = (count - offset as usize).min(limit as usize);
    if result["offset"].as_u64() != Some(offset as u64) || rows.len() != expected_len {
        return Err(invalid());
    }
    let expected_next =
        ((offset as usize + rows.len()) < count).then_some((offset as usize + rows.len()) as u64);
    if result["next_offset"].as_u64() != expected_next
        || (!result["next_offset"].is_null() && expected_next.is_none())
    {
        return Err(invalid());
    }
    let mut seen = std::collections::HashSet::new();
    let mut events = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let text = row["emoji"].as_str().ok_or_else(invalid)?;
        let id = crate::pods::emoji_id_for_text(text).ok_or_else(invalid)?;
        let score = row["score"]
            .as_f64()
            .filter(|s| s.is_finite())
            .ok_or_else(invalid)?;
        let rank = offset as u64 + index as u64 + 1;
        if !seen.insert(id) || row["rank"].as_u64() != Some(rank) {
            return Err(invalid());
        }
        let mut event =
            serde_json::to_value(crate::pods::emoji_event(id)?).map_err(|_| invalid())?;
        let entry = crate::pods::emoji_for_id(id).ok_or_else(invalid)?;
        event["category"] = json!({"group":entry.group,"subgroup":entry.subgroup});
        event["rank"] = json!(rank);
        event["score"] = json!(score);
        events.push(event);
    }
    Ok(
        json!({"events":events,"offset":offset,"next_offset":expected_next,"unicode_version":"16.0"}),
    )
}

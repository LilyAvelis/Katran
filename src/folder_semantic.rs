//! Local semantic folder-name search. No path or query is logged or sent over a network.
use crate::{ApiError, Result, domain_jobs::Cancellation};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

const MAX_FRAME: usize = 4 * 1024 * 1024;
const MAX_ITEMS: usize = 8192;
const START_TIMEOUT: Duration = Duration::from_secs(45);
const COLD_TIMEOUT: Duration = Duration::from_secs(180);
const WARM_TIMEOUT: Duration = Duration::from_secs(30);

pub fn new_session() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{now}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub fn validate(query: &str, limit: u32) -> Result<()> {
    if query.trim().is_empty()
        || query.chars().count() > 512
        || query.contains('\0')
        || !(1..=100).contains(&limit)
    {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected a nonempty query of at most 512 characters and limit 1–100",
        ));
    }
    Ok(())
}

pub trait Backend: Send {
    /// Optional diagnostics contain counts/status only, never paths or queries.
    fn cache_status(&self) -> Option<Value> {
        None
    }
    fn search(
        &mut self,
        query: &str,
        paths: &[PathBuf],
        limit: u32,
        gpu: bool,
        cancel: &Cancellation,
    ) -> Result<Vec<usize>>;
}
pub struct Resident {
    root: PathBuf,
    cache_directory: PathBuf,
    cache_stats: Option<Value>,
    worker: Option<Worker>,
    next_id: u64,
    snapshot: Option<Vec<PathBuf>>,
    gpu: Option<bool>,
}
impl Resident {
    pub fn new(root: PathBuf) -> Self {
        let cache_directory = root.join("folder-ai-index");
        Self::with_cache_directory(root, cache_directory)
    }
    /// Keep model/runtime resolution separate from derived-cache storage.
    /// Tests and integration clients may use an isolated cache directory.
    pub fn with_cache_directory(root: PathBuf, cache_directory: PathBuf) -> Self {
        Self {
            root,
            cache_directory,
            cache_stats: None,
            worker: None,
            next_id: 0,
            snapshot: None,
            gpu: None,
        }
    }
}
struct Worker {
    child: Child,
    writer: SyncSender<(Vec<u8>, SyncSender<bool>)>,
    reader: Receiver<Option<Value>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn unavailable(message: &'static str) -> ApiError {
    ApiError::new("folder_ai_unavailable", message)
}
fn wait<T>(receiver: &Receiver<T>, timeout: Duration, cancel: &Cancellation) -> Result<T> {
    let deadline = Instant::now() + timeout;
    loop {
        cancel.check()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(unavailable("Local folder AI timed out"));
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(25))) {
            Ok(v) => return Ok(v),
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(unavailable("Local folder AI worker stopped"));
            }
        }
    }
}
impl Worker {
    fn start(
        root: &std::path::Path,
        cache_directory: &std::path::Path,
        gpu: bool,
        cancel: &Cancellation,
    ) -> Result<Self> {
        // An installed but invalid Folder runtime fails explicitly: only a truly
        // absent one may use the previously verified emoji runtime.
        let (cpu_python, model, tokenizer) = match crate::folder_cpu_runtime::resolve(
            root,
            Some(cancel),
        )? {
            Some(runtime) => (runtime.python, runtime.model, runtime.tokenizer),
            None => {
                let runtime = crate::emoji_ai::runtime::resolve(root, Some(cancel)).map_err(|error| {
                    if error.code == "model_unavailable" { ApiError::new("model_unavailable", "Local Folder AI model is not installed. Install it from Settings → AI.") } else { error }
                })?;
                let base = runtime
                    .worker
                    .parent()
                    .ok_or_else(|| unavailable("Invalid local AI runtime"))?;
                (
                    runtime.python,
                    base.join("model/onnx/model.onnx"),
                    base.join("model/onnx/tokenizer.json"),
                )
            }
        };
        let python = if gpu {
            crate::folder_gpu_runtime::resolve(root)?
        } else {
            cpu_python
        };
        let mut command = Command::new(python);
        command
            .arg("-c")
            .arg(include_str!("../scripts/folder-semantic/worker.py"))
            .arg(model)
            .arg(tokenizer)
            .arg(if gpu { "gpu" } else { "cpu" })
            .arg(cache_directory.join(if gpu { "cuda.sqlite3" } else { "cpu.sqlite3" }))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|_| unavailable("Local folder AI worker could not start"))?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (writer, writes) = mpsc::sync_channel::<(Vec<u8>, SyncSender<bool>)>(1);
        std::thread::spawn(move || {
            for (data, ack) in writes {
                let ok = stdin.write_all(&data).and_then(|_| stdin.flush()).is_ok();
                let _ = ack.send(ok);
                if !ok {
                    break;
                }
            }
        });
        let (frames, reader) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                let mut data = Vec::new();
                let result = (&mut stdout)
                    .take(MAX_FRAME as u64 + 1)
                    .read_until(b'\n', &mut data);
                if result.is_err()
                    || data.is_empty()
                    || data.len() > MAX_FRAME
                    || data.last() != Some(&b'\n')
                {
                    let _ = frames.send(None);
                    break;
                }
                if frames
                    .send(serde_json::from_slice::<Value>(&data).ok())
                    .is_err()
                {
                    break;
                }
            }
        });
        let worker = Self {
            child,
            writer,
            reader,
        };
        let ready = wait(&worker.reader, START_TIMEOUT, cancel)?
            .ok_or_else(|| unavailable("Local folder AI startup failed"))?;
        if ready["type"] != "ready" || ready["protocol"] != 1 {
            return Err(if gpu {
                ApiError::new(
                    "folder_gpu_unavailable",
                    "GPU acceleration could not start; check the local GPU runtime",
                )
            } else {
                unavailable("Local folder AI startup failed")
            });
        }
        Ok(worker)
    }
    fn request(&self, value: &Value, timeout: Duration, cancel: &Cancellation) -> Result<Value> {
        let mut bytes =
            serde_json::to_vec(value).map_err(|_| unavailable("Invalid folder search request"))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_FRAME {
            return Err(ApiError::new(
                "folder_ai_input_limit",
                "Folder search snapshot is too large",
            ));
        }
        let (ack, rx) = mpsc::sync_channel(1);
        self.writer
            .try_send((bytes, ack))
            .map_err(|_| unavailable("Local folder AI worker is busy"))?;
        if !wait(&rx, timeout, cancel)? {
            return Err(unavailable("Local folder AI input failed"));
        }
        wait(&self.reader, timeout, cancel)?
            .ok_or_else(|| unavailable("Invalid local folder AI response"))
    }
}
impl Backend for Resident {
    fn cache_status(&self) -> Option<Value> {
        self.cache_stats.clone()
    }
    fn search(
        &mut self,
        query: &str,
        paths: &[PathBuf],
        limit: u32,
        gpu: bool,
        cancel: &Cancellation,
    ) -> Result<Vec<usize>> {
        self.cache_stats = None;
        if query.trim().is_empty()
            || query.chars().count() > 512
            || query.contains('\0')
            || !(1..=MAX_ITEMS as u32).contains(&limit)
        {
            return Err(ApiError::new(
                "invalid_argument",
                "Invalid semantic search request",
            ));
        }
        if paths.len() > MAX_ITEMS || paths.iter().any(|p| p.to_str().is_none()) {
            return Err(ApiError::new(
                "folder_ai_input_limit",
                "Folder snapshot is invalid or too large",
            ));
        }
        if self.gpu != Some(gpu) {
            self.worker = None;
            self.snapshot = None;
        }
        if self.worker.is_none() {
            self.worker = Some(
                Worker::start(&self.root, &self.cache_directory, gpu, cancel).map_err(|error| {
                    if gpu && error.code == "folder_ai_unavailable" {
                        ApiError::new(
                            "folder_gpu_unavailable",
                            "GPU folder search could not start; check the local GPU runtime",
                        )
                    } else {
                        error
                    }
                })?,
            );
            self.snapshot = None;
            self.gpu = Some(gpu);
        }
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        let warm = self.snapshot.as_deref() == Some(paths);
        self.cache_stats = None;
        let request = json!({"id":id,"query":query,"paths":paths,"limit":limit});
        let result = self
            .worker
            .as_ref()
            .expect("started")
            .request(
                &request,
                if warm { WARM_TIMEOUT } else { COLD_TIMEOUT },
                cancel,
            )
            .and_then(|reply| {
                if reply["id"].as_u64() != Some(id) {
                    return Err(unavailable("Invalid local folder AI response"));
                }
                if let Some(cache) = reply.get("cache").filter(|cache| cache.is_object()) {
                    let mut stats = serde_json::Map::new();
                    for name in ["loaded", "reused", "encoded"] {
                        if let Some(count) = cache[name]
                            .as_u64()
                            .filter(|count| *count <= MAX_ITEMS as u64)
                        {
                            stats.insert(name.into(), json!(count));
                        }
                    }
                    if let Some(persisted) = cache["persisted"].as_bool() {
                        stats.insert("persisted".into(), json!(persisted));
                    }
                    if let Some(status) = cache["status"].as_str().filter(|text| {
                        text.len() <= 64
                            && text.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    }) {
                        stats.insert("status".into(), json!(status));
                    }
                    self.cache_stats = Some(Value::Object(stats));
                }
                let values = reply["indices"]
                    .as_array()
                    .ok_or_else(|| unavailable("Invalid local folder AI response"))?;
                if values.len() > limit as usize {
                    return Err(unavailable("Invalid local folder AI response"));
                }
                let mut seen = HashSet::new();
                let mut indices = Vec::new();
                for v in values {
                    let i = v
                        .as_u64()
                        .and_then(|x| usize::try_from(x).ok())
                        .filter(|i| *i < paths.len())
                        .ok_or_else(|| unavailable("Invalid local folder AI response"))?;
                    if !seen.insert(i) {
                        return Err(unavailable("Invalid local folder AI response"));
                    }
                    indices.push(i);
                }
                Ok(indices)
            });
        match result {
            Ok(indices) => {
                self.snapshot = Some(paths.to_vec());
                Ok(indices)
            }
            Err(error) => {
                self.cache_stats = None;
                self.worker = None;
                self.snapshot = None;
                self.gpu = None;
                Err(if gpu && error.code == "folder_ai_unavailable" {
                    ApiError::new(
                        "folder_gpu_unavailable",
                        "GPU folder search failed; check the local GPU runtime",
                    )
                } else {
                    error
                })
            }
        }
    }
}

/// Injectable discovery keeps synthetic API checks independent of a real Everything index.
pub trait Source: Send {
    fn snapshot(&mut self, query: &str, cancel: &Cancellation) -> Result<Snapshot>;
}
pub struct Snapshot {
    pub indexed: Vec<PathBuf>,
    pub exact_candidates: Vec<PathBuf>,
    pub incomplete: bool,
}
pub struct EverythingSource;
impl Source for EverythingSource {
    fn snapshot(&mut self, query: &str, cancel: &Cancellation) -> Result<Snapshot> {
        cancel.check()?;
        let indexed = crate::everything::list_indexed_folders(MAX_ITEMS)
            .map_err(|e| ApiError::new(&e.code, e.message))?;
        cancel.check()?;
        let exact = crate::everything::search_folders(query, 100)
            .map_err(|e| ApiError::new(&e.code, e.message))?;
        cancel.check()?;
        Ok(Snapshot {
            indexed: indexed.items.into_iter().map(|item| item.path).collect(),
            exact_candidates: exact.items.into_iter().map(|item| item.path).collect(),
            incomplete: indexed.incomplete || exact.incomplete,
        })
    }
}
pub struct PageRequest<'a> {
    pub query: &'a str,
    pub offset: u32,
    pub limit: u32,
    pub expected_snapshot: Option<&'a str>,
    pub gpu: bool,
    pub session: &'a str,
}
pub fn search(
    backend: &mut dyn Backend,
    source: &mut dyn Source,
    request: PageRequest<'_>,
    saved: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Value> {
    let PageRequest {
        query,
        offset,
        limit,
        expected_snapshot,
        gpu,
        session,
    } = request;
    validate(query, limit)?;
    if offset as usize >= MAX_ITEMS || (offset > 0 && expected_snapshot.is_none()) {
        return Err(ApiError::new(
            "invalid_argument",
            "A valid snapshot is required for subsequent pages",
        ));
    }
    let snapshot = source.snapshot(query, cancel)?;

    let mut incomplete = snapshot.incomplete;
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    // Reserve space for query, JSON fields and framing; oversized snapshots are partial.
    let mut frame_bytes = 4096usize;
    // Saved and lexical candidates precede the bounded index so relevant folders
    // outside the broad sample can still be ranked by the model. The worker
    // reuses cached embeddings for unchanged paths across queries.
    for path in saved
        .iter()
        .chain(&snapshot.exact_candidates)
        .chain(&snapshot.indexed)
    {
        let text = path.to_str().ok_or_else(|| {
            ApiError::new(
                "folder_ai_input_limit",
                "Folder snapshot contains an invalid path",
            )
        })?;
        let key = if cfg!(windows) {
            text.to_lowercase()
        } else {
            text.to_owned()
        };
        if seen.insert(key) {
            let encoded = serde_json::to_vec(path).map_err(|_| {
                ApiError::new(
                    "folder_ai_input_limit",
                    "Folder snapshot contains an invalid path",
                )
            })?;
            if paths.len() == MAX_ITEMS || frame_bytes.saturating_add(encoded.len() + 1) > MAX_FRAME
            {
                incomplete = true;
                break;
            }
            frame_bytes += encoded.len() + 1;
            paths.push(path.clone());
        }
    }
    use sha2::{Digest, Sha256};
    let token_data = serde_json::to_vec(&(session, query, gpu, &paths, &snapshot.exact_candidates))
        .map_err(|_| unavailable("Invalid folder search snapshot"))?;
    let token = format!("{:x}", Sha256::digest(token_data));
    if expected_snapshot.is_some_and(|expected| expected != token) {
        return Err(ApiError::new(
            "folder_ai_snapshot_changed",
            "Folder search changed; start a new search",
        ));
    }
    let prefix = (offset as usize + limit as usize + 1).min(MAX_ITEMS);
    cancel.check()?;
    // This call must succeed even when there are exact matches: no lexical-only success.
    let indices = backend.search(query, &paths, prefix as u32, gpu, cancel)?;
    cancel.check()?;
    if indices.len() > prefix
        || indices.iter().copied().collect::<HashSet<_>>().len() != indices.len()
    {
        return Err(unavailable("Invalid local folder AI response"));
    }
    let indexed_count = paths.len();
    let mut result = Vec::new();
    let mut used = HashSet::new();
    let needle = query.trim().to_lowercase();
    for path in snapshot.exact_candidates.iter().chain(paths.iter()) {
        let Some(text) = path.to_str() else {
            return Err(ApiError::new(
                "folder_ai_input_limit",
                "Folder snapshot contains an invalid path",
            ));
        };
        let key = if cfg!(windows) {
            text.to_lowercase()
        } else {
            text.to_owned()
        };
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.to_lowercase() == needle)
            && used.insert(key)
        {
            result.push(json!({"path":path}));
            if result.len() == prefix {
                break;
            }
        }
    }
    for i in indices {
        if i >= paths.len() {
            return Err(unavailable("Invalid local folder AI response"));
        }
        let text = paths[i].to_string_lossy();
        let key = if cfg!(windows) {
            text.to_lowercase()
        } else {
            text.into_owned()
        };
        if used.insert(key) && result.len() < prefix {
            result.push(json!({"path":paths[i]}));
        }
    }
    let offset = offset as usize;
    let has_more = result.len() > offset.saturating_add(limit as usize);
    let items: Vec<_> = result
        .into_iter()
        .skip(offset)
        .take(limit as usize)
        .collect();
    let next_offset = has_more.then_some(offset + items.len());
    Ok(
        json!({"query":query,"offset":offset,"items":items,"next_offset":next_offset,"has_more":has_more,"snapshot":token,"incomplete":incomplete,"indexed_count":indexed_count,"scope":"bounded_everything_index_and_saved_folders","cache":backend.cache_status()}),
    )
}

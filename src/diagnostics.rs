//! Best-effort local diagnostics. Producers never wait for disk or queue capacity.
//! Only record application events: no desktop hooks, clipboard reads, or raw blobs.
use serde::Serialize;
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, SyncSender, TrySendError},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const QUEUE_CAPACITY: usize = 1024;
const FILE_LIMIT: u64 = 2 * 1024 * 1024;
const DATA_LIMIT: usize = 8192;

#[derive(Clone)]
pub struct Logger {
    inner: Arc<Inner>,
}
struct Inner {
    sender: SyncSender<Message>,
    counters: Arc<Counters>,
    started: Instant,
    session: String,
    sequence: AtomicU64,
}
#[derive(Default)]
struct Counters {
    queued: AtomicU64,
    dropped: AtomicU64,
    write_errors: AtomicU64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsStatus {
    /// Cumulative number of events accepted by the queue, not its current depth.
    pub queued: u64,
    pub dropped: u64,
    pub write_errors: u64,
}
enum Message {
    Record(Vec<u8>),
    Flush(mpsc::Sender<bool>),
}

impl Logger {
    pub fn start(data_root: &Path) -> io::Result<Self> {
        Self::start_with_limits(data_root, QUEUE_CAPACITY, FILE_LIMIT)
    }
    fn start_with_limits(data_root: &Path, capacity: usize, file_limit: u64) -> io::Result<Self> {
        let directory = data_root.join("logs");
        fs::create_dir_all(&directory)?;
        let writer = RotatingWriter::open(directory, file_limit)?;
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let counters = Arc::new(Counters::default());
        let worker_counters = counters.clone();
        std::thread::Builder::new()
            .name("katran-diagnostics".into())
            .spawn(move || {
                let mut writer = writer;
                for message in receiver {
                    match message {
                        Message::Record(bytes) => {
                            if writer.write_record(&bytes).is_err() {
                                worker_counters.write_errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Message::Flush(reply) => {
                            let _ = reply.send(writer.flush().is_ok());
                        }
                    }
                }
                let _ = writer.flush();
            })?;
        Ok(Self {
            inner: Arc::new(Inner {
                sender,
                counters,
                started: Instant::now(),
                session: format!("{}-{}", wall_ms(), std::process::id()),
                sequence: AtomicU64::new(0),
            }),
        })
    }
    /// Enqueues one event without waiting. Large values are replaced by a size summary.
    /// Recognizable credential tokens are redacted; callers must still omit other
    /// secrets and byte payloads before passing data here.
    pub fn record(&self, kind: &str, mut data: Value) {
        sanitize_value(&mut data);
        let data = match serde_json::to_vec(&data) {
            Ok(encoded) if encoded.len() <= DATA_LIMIT => data,
            Ok(encoded) => json!({"truncated":true,"original_bytes":encoded.len()}),
            Err(_) => Value::Null,
        };
        let event = json!({"session":self.inner.session,"sequence":self.inner.sequence.fetch_add(1, Ordering::Relaxed),"wall_time_ms":wall_ms(),"monotonic_us":self.inner.started.elapsed().as_micros().min(u64::MAX as u128) as u64,"kind":redact_tokens(kind).chars().take(128).collect::<String>(),"data":data,"dropped_total":self.inner.counters.dropped.load(Ordering::Relaxed)});
        let Ok(mut bytes) = serde_json::to_vec(&event) else {
            return;
        };
        bytes.push(b'\n');
        match self.inner.sender.try_send(Message::Record(bytes)) {
            Ok(()) => {
                self.inner.counters.queued.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.inner.counters.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    pub fn status(&self) -> DiagnosticsStatus {
        DiagnosticsStatus {
            queued: self.inner.counters.queued.load(Ordering::Relaxed),
            dropped: self.inner.counters.dropped.load(Ordering::Relaxed),
            write_errors: self.inner.counters.write_errors.load(Ordering::Relaxed),
        }
    }
    /// Explicit blocking barrier for tests or orderly CLI teardown; never use in UI input handling.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (sender, receiver) = mpsc::channel();
        self.inner.sender.try_send(Message::Flush(sender)).is_ok()
            && receiver.recv_timeout(timeout).unwrap_or(false)
    }
}
fn sanitize_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = redact_tokens(text),
        Value::Array(items) => items.iter_mut().for_each(sanitize_value),
        Value::Object(fields) => fields.values_mut().for_each(sanitize_value),
        _ => {}
    }
}

/// Deliberately narrow recognition preserves ordinary search and pasted text.
fn redact_tokens(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        if [b"ghp_".as_slice(), b"github_pat_", b"sk-"]
            .iter()
            .any(|prefix| bytes[index..].starts_with(prefix))
        {
            let mut end = index;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-'))
            {
                end += 1;
            }
            if end - index > 16 {
                result.push_str(&text[copied..index]);
                result.push_str("[REDACTED]");
                copied = end;
                index = end;
                continue;
            }
        }
        index += 1;
    }
    result.push_str(&text[copied..]);
    result
}
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
struct RotatingWriter {
    directory: PathBuf,
    file: Option<File>,
    size: u64,
    limit: u64,
}
impl RotatingWriter {
    fn open(directory: PathBuf, limit: u64) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("events.jsonl"))?;
        let size = file.metadata()?.len();
        Ok(Self {
            directory,
            file: Some(file),
            size,
            limit,
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostic file unavailable"))?
            .flush()
    }
    fn write_record(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.size + bytes.len() as u64 > self.limit {
            self.flush()?;
            // Close before renaming on Windows.
            drop(self.file.take());
            let oldest = self.directory.join("events.3.jsonl");
            if oldest.exists() {
                fs::remove_file(oldest)?;
            }
            for index in (1..=2).rev() {
                let source = self.directory.join(format!("events.{index}.jsonl"));
                if source.exists() {
                    fs::rename(
                        source,
                        self.directory.join(format!("events.{}.jsonl", index + 1)),
                    )?;
                }
            }
            let current = self.directory.join("events.jsonl");
            if current.exists() {
                fs::rename(&current, self.directory.join("events.1.jsonl"))?;
            }
            self.file = Some(OpenOptions::new().create(true).append(true).open(current)?);
            self.size = 0;
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostic file unavailable"))?
            .write_all(bytes)?;
        self.size += bytes.len() as u64;
        Ok(())
    }
}

/// Summarizes commands without serializing image bytes, including nested ingestion.
pub fn summarize_command(command: &crate::Command) -> Value {
    match command {
        crate::Command::IngestLink { url, source } => {
            json!({"type":"ingest_link","byte_count":url.len(),"source":source})
        }
        crate::Command::IngestTable { rows, source } => {
            json!({"type":"ingest_table","row_count":rows.len(),"source":source})
        }
        crate::Command::IngestTableTsv { text, source } => {
            json!({"type":"ingest_table_tsv","byte_count":text.len(),"source":source})
        }
        crate::Command::IngestTableHtml { html, source } => {
            json!({"type":"ingest_table_html","byte_count":html.len(),"source":source})
        }
        crate::Command::IngestText { text, source } => {
            json!({"type":"ingest_text","byte_count":text.len(),"source":source})
        }
        crate::Command::BeginJob { command } => {
            json!({"type":"begin_job","command":summarize_command(command)})
        }
        crate::Command::CommitFolderCandidates { items } => {
            json!({"type":"commit_folder_candidates","count":items.len()})
        }
        crate::Command::CommitScreenshotMatches { pairs } => {
            json!({"type":"commit_screenshot_matches","count":pairs.len()})
        }
        crate::Command::IngestBytes {
            name,
            bytes,
            source,
        } => json!({"type":"ingest_bytes","name":name,"byte_count":bytes.len(),"source":source}),
        crate::Command::IngestWithOrigin { origin, command } => {
            json!({"type":"ingest_with_origin","origin":origin,"command":summarize_command(command)})
        }
        _ => serde_json::to_value(command).unwrap_or(Value::Null),
    }
}

/// Hot read clients are silent on success; slow operations and failures still log.
pub(crate) fn hot_command(command: &crate::Command) -> Option<&'static str> {
    use crate::Command::*;
    Some(match command {
        Interaction => "interaction",
        GetSettings => "get_settings",
        Tabs => "tabs",
        Shortcuts => "shortcuts",
        SetVisible { .. } => "set_visible",
        Status => "status",
        PollSources { .. } => "poll_sources",
        SourceStatus => "source_status",
        PollJob { .. } => "poll_job",
        Actions => "actions",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn link_diagnostics_omit_url_in_direct_and_nested_commands() {
        let secret = "https://example.test/private?signature=sensitive-value#personal-fragment";
        for command in [
            crate::Command::IngestLink {
                url: secret.into(),
                source: "test".into(),
            },
            crate::Command::BeginJob {
                command: Box::new(crate::Command::IngestLink {
                    url: secret.into(),
                    source: "test".into(),
                }),
            },
        ] {
            let summary = summarize_command(&command).to_string();
            assert!(!summary.contains("example.test"));
            assert!(!summary.contains("sensitive-value"));
            assert!(!summary.contains("personal-fragment"));
            assert!(summary.contains("byte_count"));
        }
    }
    #[test]
    fn rotation_is_bounded_and_json_lines_are_complete() {
        let root = tempfile::tempdir().unwrap();
        let logger = Logger::start_with_limits(root.path(), 1024, 1024).unwrap();
        for n in 0..80 {
            logger.record("synthetic", json!({"n":n,"padding":"x".repeat(150),"nested":[{"text":"Обычный поиск ghp_SYNTHETIC0123456789 github_pat_SYNTHETIC0123456789 sk-SYNTHETIC0123456789 конец"}]}));
        }
        assert!(logger.flush(Duration::from_secs(5)));
        let files: Vec<_> = fs::read_dir(root.path().join("logs"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(files.len(), 4);
        for path in files {
            let bytes = fs::read(&path).unwrap();
            assert!(bytes.len() <= 1024);
            for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
                let value: Value = serde_json::from_slice(line).unwrap();
                assert_eq!(value["kind"], "synthetic");
                assert_eq!(
                    value["data"]["nested"][0]["text"],
                    "Обычный поиск [REDACTED] [REDACTED] [REDACTED] конец"
                );
            }
        }
        assert_eq!(logger.status().write_errors, 0);
    }
    #[test]
    fn full_queue_drops_without_waiting() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let logger = Logger {
            inner: Arc::new(Inner {
                sender,
                counters: Arc::new(Counters::default()),
                started: Instant::now(),
                session: "test".into(),
                sequence: AtomicU64::new(0),
            }),
        };
        logger.record("first", json!({}));
        logger.record("second", json!({}));
        assert_eq!(logger.status().queued, 1);
        assert_eq!(logger.status().dropped, 1);
        assert!(!logger.flush(Duration::from_millis(1)));
    }
}

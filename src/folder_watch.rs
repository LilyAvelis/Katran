//! Native folder signals and synthetic-safe discovery, hashing outside the dispatcher.
use crate::{ApiError, Result, downloads::DownloadsScanner, sharex::SharexScanner};
use std::{
    collections::VecDeque,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderSource {
    Sharex,
    Downloads,
}
#[derive(Clone, Debug)]
pub struct FolderConfig {
    pub source: FolderSource,
    pub root: PathBuf,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedCandidate {
    pub source: FolderSource,
    pub root: PathBuf,
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    pub content_hash: String,
}
#[derive(Debug)]
pub enum FolderEvent {
    Candidate(PreparedCandidate),
    Refresh {
        source: FolderSource,
    },
    Error {
        source: FolderSource,
        message: String,
    },
}

pub struct FolderWatcher {
    stop: Arc<AtomicBool>,
    receiver: mpsc::Receiver<FolderEvent>,
}
impl FolderWatcher {
    pub fn start(configs: Vec<FolderConfig>) -> Result<Self> {
        if configs.len() > 2 {
            return Err(ApiError::new(
                "invalid_argument",
                "At most two configured folder roots",
            ));
        }
        let mut roots = Vec::new();
        for config in configs {
            if fs::symlink_metadata(&config.root)?.file_type().is_symlink() {
                return Err(ApiError::new(
                    "invalid_directory",
                    "Folder root cannot be a symbolic link",
                ));
            }
            let root = fs::canonicalize(&config.root)?;
            if !root.is_dir() {
                return Err(ApiError::new(
                    "invalid_directory",
                    "Folder root must be a directory",
                ));
            }
            roots.push(Root {
                source: config.source,
                native: None,
                root,
                shots: SharexScanner::default(),
                downloads: DownloadsScanner::default(),
                due: Some(Instant::now()),
                settling: false,
                pending: VecDeque::new(),
                outgoing: VecDeque::new(),
            });
        }
        // At most one dispatcher batch can wait ahead of a fresh notification.
        let (sender, receiver) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        for mut root in roots {
            let worker_stop = Arc::clone(&stop);
            let sender = sender.clone();
            if let Err(error) = std::thread::Builder::new()
                .name(format!("katran-folder-{:?}", root.source))
                .spawn(move || {
                    // Windows can cancel I/O when its issuing thread exits.
                    // Arm and rearm on this long-lived thread, never the setup caller.
                    match native::Watch::new(&root.root) {
                        Ok(watch) => root.native = Some(watch),
                        Err(error) => {
                            let _ = sender.try_send(FolderEvent::Error {
                                source: root.source,
                                message: error.message,
                            });
                            return;
                        }
                    }
                    while !worker_stop.load(Ordering::Relaxed) {
                        root.tick(&sender, &worker_stop);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                })
            {
                stop.store(true, Ordering::Relaxed);
                return Err(ApiError::new("watcher_error", error.to_string()));
            }
        }
        Ok(Self { stop, receiver })
    }
    pub fn try_recv(&self) -> Option<FolderEvent> {
        self.receiver.try_recv().ok()
    }
}
impl Drop for FolderWatcher {
    fn drop(&mut self) {
        // Never wait on filesystem I/O during GUI shutdown/reconfiguration.
        // The worker owns all native handles and cancels its overlapped reads.
        self.stop.store(true, Ordering::Relaxed);
    }
}
struct Root {
    source: FolderSource,
    root: PathBuf,
    native: Option<native::Watch>,
    shots: SharexScanner,
    downloads: DownloadsScanner,
    due: Option<Instant>,
    settling: bool,
    pending: VecDeque<PathBuf>,
    outgoing: VecDeque<FolderEvent>,
}
impl Root {
    fn tick(&mut self, sender: &mpsc::SyncSender<FolderEvent>, stop: &AtomicBool) {
        match self
            .native
            .as_mut()
            .expect("worker initializes native watch")
            .changed()
        {
            Ok(true) => {
                // Keep an in-flight stability interval: a burst of native
                // notifications must not turn two observations into back-to-back
                // scans before the writer has settled.
                if self.due.is_none() {
                    self.due = Some(Instant::now());
                }
                self.settling = false;
                // Coalesce refresh notifications; queue memory stays bounded.
                if !self
                    .outgoing
                    .iter()
                    .any(|e| matches!(e, FolderEvent::Refresh { .. }))
                {
                    self.outgoing.push_back(FolderEvent::Refresh {
                        source: self.source,
                    });
                }
            }
            Ok(false) => {}
            Err(error) => {
                // Report failure once, do not silently switch to periodic polling.
                if self.outgoing.is_empty() {
                    self.outgoing.push_back(FolderEvent::Error {
                        source: self.source,
                        message: error.message,
                    });
                }
                self.due = None;
            }
        }
        if self.due.is_some_and(|due| Instant::now() >= due) {
            let paths = match self.source {
                FolderSource::Sharex => self.shots.scan(&self.root),
                FolderSource::Downloads => self.downloads.scan(&self.root),
            };
            match paths {
                Ok(paths) => {
                    // Newly settled files overtake initial historical discovery.
                    self.pending.retain(|path| path.is_file());
                    let existing: std::collections::HashSet<_> =
                        self.pending.iter().cloned().collect();
                    for path in paths.into_iter().rev() {
                        if !existing.contains(&path) {
                            self.pending.push_front(path);
                        }
                    }
                }
                Err(error) => self.outgoing.push_back(FolderEvent::Error {
                    source: self.source,
                    message: error.message,
                }),
            }
            self.due = if self.settling {
                None
            } else {
                Some(Instant::now() + Duration::from_millis(50))
            };
            self.settling = !self.settling;
        }
        // One hash per turn lets stop and the second configured root progress.
        if self.outgoing.is_empty()
            && let Some(path) = self.pending.pop_front()
        {
            match self.prepare(path, stop) {
                Ok(candidate) => self.outgoing.push_back(FolderEvent::Candidate(candidate)),
                Err(error) => {
                    self.outgoing.push_back(FolderEvent::Error {
                        source: self.source,
                        message: error.message,
                    });
                    self.shots = SharexScanner::default();
                    self.downloads = DownloadsScanner::default();
                    self.due = Some(Instant::now() + Duration::from_millis(50));
                    self.settling = false;
                }
            }
        }
        while let Some(event) = self.outgoing.pop_front() {
            match sender.try_send(event) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(event)) => {
                    self.outgoing.push_front(event);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
        }
    }
    fn prepare(&self, path: PathBuf, stop: &AtomicBool) -> Result<PreparedCandidate> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let kind = fs::symlink_metadata(&path)?;
        let path = fs::canonicalize(&path)?;
        if kind.file_type().is_symlink() || !path.starts_with(&self.root) || !kind.is_file() {
            return Err(ApiError::new(
                "invalid_path",
                "Candidate escaped configured folder",
            ));
        }
        let before = fs::metadata(&path)?;
        let mut file = fs::File::open(&path)?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(ApiError::new("cancelled", "Folder discovery stopped"));
            }
            let length = file.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            digest.update(&buffer[..length]);
        }
        let content_hash = format!("{:x}", digest.finalize());
        let after = fs::metadata(&path)?;
        if before.len() != after.len() || before.modified()? != after.modified()? {
            return Err(ApiError::new(
                "source_changed",
                "Folder candidate changed while hashing",
            ));
        }
        Ok(PreparedCandidate {
            source: self.source,
            root: self.root.clone(),
            path,
            size: after.len(),
            modified: after.modified()?,
            content_hash,
        })
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path};
    type Handle = *mut c_void;
    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: Handle,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            creation: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn CreateEventW(
            security: *const c_void,
            manual: i32,
            initial: i32,
            name: *const u16,
        ) -> Handle;
        fn ReadDirectoryChangesW(
            directory: Handle,
            buffer: *mut c_void,
            length: u32,
            subtree: i32,
            filter: u32,
            returned: *mut u32,
            overlapped: *mut Overlapped,
            completion: *const c_void,
        ) -> i32;
        fn GetOverlappedResult(
            file: Handle,
            overlapped: *mut Overlapped,
            transferred: *mut u32,
            wait: i32,
        ) -> i32;
        fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
        fn ResetEvent(handle: Handle) -> i32;
        fn CancelIoEx(file: Handle, overlapped: *mut Overlapped) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    pub struct Watch {
        directory: Handle,
        overlap: Box<Overlapped>,
        buffer: Box<[u8; 32768]>,
        failed: bool,
    }
    // Owned handles/buffers move to exactly one discovery thread.
    unsafe impl Send for Watch {}
    impl Watch {
        pub fn new(root: &Path) -> Result<Self> {
            let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
            let directory = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    1,
                    7,
                    std::ptr::null(),
                    3,
                    0x42000000,
                    std::ptr::null_mut(),
                )
            };
            if directory as isize == -1 {
                return Err(error());
            }
            let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            if event.is_null() {
                unsafe {
                    CloseHandle(directory);
                }
                return Err(error());
            }
            let mut watch = Self {
                directory,
                overlap: Box::new(Overlapped {
                    internal: 0,
                    internal_high: 0,
                    offset: 0,
                    offset_high: 0,
                    event,
                }),
                buffer: Box::new([0; 32768]),
                failed: false,
            };
            watch.arm()?;
            Ok(watch)
        }
        fn arm(&mut self) -> Result<()> {
            unsafe {
                ResetEvent(self.overlap.event);
            }
            let success = unsafe {
                ReadDirectoryChangesW(
                    self.directory,
                    self.buffer.as_mut_ptr().cast(),
                    self.buffer.len() as u32,
                    1,
                    0x1 | 0x2 | 0x8 | 0x10 | 0x40,
                    std::ptr::null_mut(),
                    &mut *self.overlap,
                    std::ptr::null(),
                )
            };
            if success == 0 {
                self.failed = true;
                return Err(error());
            }
            Ok(())
        }
        pub fn changed(&mut self) -> Result<bool> {
            if self.failed {
                return Ok(false);
            }
            match unsafe { WaitForSingleObject(self.overlap.event, 0) } {
                258 => Ok(false),
                0 => {
                    let mut transferred = 0;
                    let success = unsafe {
                        GetOverlappedResult(self.directory, &mut *self.overlap, &mut transferred, 0)
                    };
                    if success == 0 {
                        if std::io::Error::last_os_error().raw_os_error() == Some(1022) {
                            self.arm()?;
                            return Ok(true);
                        }
                        self.failed = true;
                        return Err(error());
                    }
                    self.arm()?;
                    // Empty completion means buffer overflow; full rescan repairs it.
                    Ok(true)
                }
                _ => {
                    self.failed = true;
                    Err(error())
                }
            }
        }
    }
    impl Drop for Watch {
        fn drop(&mut self) {
            unsafe {
                CancelIoEx(self.directory, &mut *self.overlap);
                // Cancellation must complete before releasing the buffer/OVERLAPPED.
                let mut transferred = 0;
                GetOverlappedResult(self.directory, &mut *self.overlap, &mut transferred, 1);
                CloseHandle(self.directory);
                CloseHandle(self.overlap.event);
            }
        }
    }
    fn error() -> ApiError {
        ApiError::new("watcher_error", std::io::Error::last_os_error().to_string())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn candidate(watcher: &FolderWatcher) -> PreparedCandidate {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match watcher.try_recv() {
                Some(FolderEvent::Candidate(candidate)) => return candidate,
                Some(FolderEvent::Error { message, .. }) => panic!("{message}"),
                _ => {}
            }
            assert!(
                Instant::now() < deadline,
                "synthetic native watcher timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn native_initial_and_created_file_discovery_never_changes_sources() {
        let temp = tempfile::tempdir().unwrap();
        let initial = temp.path().join("initial.png");
        fs::write(&initial, b"synthetic initial bytes").unwrap();
        let config = FolderConfig {
            source: FolderSource::Sharex,
            root: temp.path().into(),
        };
        // Setup is allowed on a short-lived helper: native I/O must survive it.
        let watcher = std::thread::spawn(move || FolderWatcher::start(vec![config]).unwrap())
            .join()
            .unwrap();
        let first = candidate(&watcher);
        assert_eq!(first.path, fs::canonicalize(&initial).unwrap());
        assert_eq!(
            first.content_hash,
            crate::preview::content_hash(&initial).unwrap()
        );
        let created = temp.path().join("created.png");
        fs::write(&created, b"synthetic created bytes").unwrap();
        let next = candidate(&watcher);
        assert_eq!(next.path, fs::canonicalize(&created).unwrap());
        assert_eq!(next.root, fs::canonicalize(temp.path()).unwrap());
        assert_eq!(fs::read(&initial).unwrap(), b"synthetic initial bytes");
        assert_eq!(fs::read(&created).unwrap(), b"synthetic created bytes");
        drop(watcher);
    }
}
#[cfg(not(windows))]
mod native {
    use super::*;
    pub struct Watch;
    impl Watch {
        pub fn new(_: &std::path::Path) -> Result<Self> {
            Err(ApiError::new(
                "unsupported_platform",
                "Native folder watching requires Windows",
            ))
        }
        pub fn changed(&mut self) -> Result<bool> {
            Ok(false)
        }
    }
}

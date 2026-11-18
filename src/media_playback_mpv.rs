//! libmpv playback, optionally using an offscreen GPU renderer with CPU readback.
//! libmpv owns audio. No native resources are created until `open`.
#[path = "media_playback_mpv_gpu.rs"]
mod gpu;
use crate::{
    ApiError, Result,
    media_playback::{Acceleration, Action, Backend, Frame, PlaybackState, Snapshot, Source},
};
use libloading::{
    Library,
    os::windows::{
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, Library as WinLibrary,
    },
};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    io::Read,
    path::{Component, Path, PathBuf, Prefix},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const DLL_SHA256: &str = "c91949577ae9cc2d500208da51f2c4a24cfa7a02d0ce6e130bf67ec738b450bb";
type Handle = *mut c_void;
#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
impl Param {
    fn new<T>(kind: c_int, value: &mut T) -> Self {
        Self {
            kind,
            data: (value as *mut T).cast(),
        }
    }
    fn end() -> Self {
        Self {
            kind: 0,
            data: std::ptr::null_mut(),
        }
    }
}
#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: Handle,
}
#[repr(C)]
struct Property {
    name: *const c_char,
    format: c_int,
    data: Handle,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}
struct Api {
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    option: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(Handle, u64, *const *const c_char) -> c_int,
    observe: unsafe extern "C" fn(Handle, u64, *const c_char, c_int) -> c_int,
    wait: unsafe extern "C" fn(Handle, f64) -> *const Event,
    error: unsafe extern "C" fn(c_int) -> *const c_char,
    destroy: unsafe extern "C" fn(Handle),
    render_create: unsafe extern "C" fn(*mut Handle, Handle, *mut Param) -> c_int,
    render_callback: unsafe extern "C" fn(Handle, Option<unsafe extern "C" fn(Handle)>, Handle),
    render_update: unsafe extern "C" fn(Handle) -> u64,
    render: unsafe extern "C" fn(Handle, *mut Param) -> c_int,
    render_free: unsafe extern "C" fn(Handle),
    _library: Library,
}
impl Api {
    unsafe fn load(path: &Path) -> std::result::Result<Self, String> {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        // Deny writes/deletion until the verified image has been loaded.
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x0020_0000)
            .open(path)
            .map_err(|e| format!("Cannot open video engine: {e}"))?;
        let metadata = file
            .metadata()
            .map_err(|e| format!("Cannot inspect video engine: {e}"))?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err("The video engine must be an ordinary file".into());
        }
        let mut hash = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        if format!("{:x}", hash.finalize()) != DLL_SHA256 {
            return Err("The video engine checksum is not trusted".into());
        }
        let library: Library = unsafe {
            WinLibrary::load_with_flags(
                path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .map_err(|e| format!("Cannot load video engine: {e}"))?
        .into();
        macro_rules! sym {
            ($name:literal) => {
                unsafe {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|e| format!("Missing video engine function: {e}"))?
                }
            };
        }
        Ok(Self {
            create: sym!("mpv_create"),
            initialize: sym!("mpv_initialize"),
            option: sym!("mpv_set_option_string"),
            command: sym!("mpv_command_async"),
            observe: sym!("mpv_observe_property"),
            wait: sym!("mpv_wait_event"),
            error: sym!("mpv_error_string"),
            destroy: sym!("mpv_terminate_destroy"),
            render_create: sym!("mpv_render_context_create"),
            render_callback: sym!("mpv_render_context_set_update_callback"),
            render_update: sym!("mpv_render_context_update"),
            render: sym!("mpv_render_context_render"),
            render_free: sym!("mpv_render_context_free"),
            _library: library,
        })
    }
    fn check(&self, code: c_int) -> std::result::Result<(), String> {
        if code >= 0 {
            Ok(())
        } else {
            let p = unsafe { (self.error)(code) };
            Err(if p.is_null() {
                format!("libmpv error {code}")
            } else {
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            })
        }
    }
}
fn err(code: &str, message: impl ToString) -> ApiError {
    ApiError::new(code, message)
}
fn empty() -> Snapshot {
    Snapshot {
        state: PlaybackState::Stopped,
        duration_ms: None,
        position_ms: 0,
        volume: 1.0,
        has_audio: false,
        width: 0,
        height: 0,
        error: None,
        acceleration: None,
    }
}
struct Shared {
    snapshot: Snapshot,
    frame: Option<Frame>,
    last_poll: Instant,
}
enum Command {
    Control(Action, SyncSender<std::result::Result<(), String>>),
    Close,
}
#[derive(Default)]
pub struct MpvBackend {
    worker: Option<JoinHandle<()>>,
    tx: Option<SyncSender<Command>>,
    shared: Option<Arc<Mutex<Shared>>>,
    cancel: Option<Arc<AtomicBool>>,
    requested_gpu: bool,
    #[cfg(test)]
    force_gpu_failure: bool,
}
impl MpvBackend {
    pub fn with_gpu(requested_gpu: bool) -> Self {
        let mut backend = Self::default();
        backend.requested_gpu = requested_gpu;
        backend
    }
    #[cfg(test)]
    fn with_forced_fallback() -> Self {
        let mut backend = Self::with_gpu(true);
        backend.force_gpu_failure = true;
        backend
    }
}

fn library_path() -> Result<PathBuf> {
    let path = match std::env::var_os("KATRAN_MPV_DLL") {
        Some(p) => PathBuf::from(p),
        None => std::env::current_exe()
            .map_err(|e| err("media_unavailable", e))?
            .with_file_name("libmpv-2.dll"),
    };
    if !path.is_absolute() {
        return Err(err(
            "media_unavailable",
            "The video engine path must be absolute",
        ));
    }
    Ok(path)
}
impl Backend for MpvBackend {
    fn open(&mut self, source: Source) -> Result<()> {
        self.close();
        if !source.path.is_absolute()
            || !matches!(source.path.components().next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        {
            return Err(err(
                "media_invalid_source",
                "A local absolute file path is required",
            ));
        }
        let metadata =
            std::fs::metadata(&source.path).map_err(|e| err("media_source_unavailable", e))?;
        if !metadata.is_file() {
            return Err(err(
                "media_invalid_source",
                "The media source is not a file",
            ));
        }
        let path = source.path.to_str().ok_or_else(|| {
            err(
                "media_invalid_source",
                "The media path is not valid Unicode",
            )
        })?;
        let path = path.strip_prefix(r"\\?\").unwrap_or(path).to_owned();
        if path.contains('\0') {
            return Err(err("media_invalid_source", "Invalid media path"));
        }
        let dll = library_path()?;
        let shared = Arc::new(Mutex::new(Shared {
            snapshot: Snapshot {
                state: PlaybackState::Loading,
                ..empty()
            },
            frame: None,
            last_poll: Instant::now(),
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(8);
        let data = Arc::clone(&shared);
        let stop = Arc::clone(&cancel);
        #[cfg(test)]
        let force_gpu_failure = self.force_gpu_failure;
        #[cfg(not(test))]
        let force_gpu_failure = false;
        let worker = thread::Builder::new()
            .name("katran-mpv".into())
            .spawn({
                let requested_gpu = self.requested_gpu;
                move || run(dll, path, rx, data, stop, requested_gpu, force_gpu_failure)
            })
            .map_err(|e| err("media_unavailable", e))?;
        self.worker = Some(worker);
        self.tx = Some(tx);
        self.shared = Some(shared);
        self.cancel = Some(cancel);
        Ok(())
    }
    fn control(&mut self, action: Action) -> Result<()> {
        if matches!(action, Action::Stop) {
            self.close();
            return Ok(());
        }
        if let Some(s) = &self.shared {
            s.lock().map_err(|e| err("media_unavailable", e))?.last_poll = Instant::now();
        }
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| err("media_stopped", "No active playback"))?;
        let (reply, recv) = mpsc::sync_channel(1);
        tx.try_send(Command::Control(action, reply))
            .map_err(|_| err("media_busy", "Playback worker is busy or stopped"))?;
        match recv.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(err("media_control_failed", e)),
            Err(e) => {
                let shared = self.shared.clone();
                self.close();
                let detail =
                    shared.and_then(|s| s.lock().ok().and_then(|s| s.snapshot.error.clone()));
                Err(err(
                    "media_unavailable",
                    detail.unwrap_or_else(|| format!("Playback worker did not respond: {e}")),
                ))
            }
        }
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        match &self.shared {
            Some(s) => {
                let mut s = s.lock().map_err(|e| err("media_unavailable", e))?;
                s.last_poll = Instant::now();
                Ok(s.snapshot.clone())
            }
            None => Ok(empty()),
        }
    }
    fn frame(&mut self, after_serial: u64) -> Result<Option<Frame>> {
        match &self.shared {
            Some(s) => {
                let mut s = s.lock().map_err(|e| err("media_unavailable", e))?;
                s.last_poll = Instant::now();
                Ok(s.frame
                    .as_ref()
                    .filter(|f| f.serial > after_serial)
                    .cloned())
            }
            None => Ok(None),
        }
    }
    fn close(&mut self) {
        if let Some(c) = self.cancel.take() {
            c.store(true, Ordering::Release)
        }
        if let Some(tx) = self.tx.take() {
            let _ = tx.try_send(Command::Close);
            drop(tx)
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        self.shared = None;
    }
}
impl Drop for MpvBackend {
    fn drop(&mut self) {
        self.close()
    }
}

struct Core {
    api: Api,
    handle: Handle,
    render: Handle,
    notify: Box<AtomicBool>,
    gpu: Option<gpu::Gpu>,
}
impl Drop for Core {
    fn drop(&mut self) {
        unsafe {
            if !self.render.is_null() {
                (self.api.render_callback)(self.render, None, std::ptr::null_mut());
                (self.api.render_free)(self.render);
            }
            if !self.handle.is_null() {
                (self.api.destroy)(self.handle);
            }
        }
    }
}
unsafe extern "C" fn render_notify(data: Handle) {
    if !data.is_null() {
        unsafe { &*data.cast::<AtomicBool>() }.store(true, Ordering::Release);
    }
}
fn issue(api: &Api, handle: Handle, parts: &[&str]) -> std::result::Result<(), String> {
    let args: Vec<CString> = parts
        .iter()
        .map(|s| CString::new(*s).map_err(|e| e.to_string()))
        .collect::<std::result::Result<_, _>>()?;
    let mut ptrs: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    api.check(unsafe { (api.command)(handle, 1, ptrs.as_ptr()) })
}
fn run(
    dll: PathBuf,
    path: String,
    rx: Receiver<Command>,
    shared: Arc<Mutex<Shared>>,
    cancel: Arc<AtomicBool>,
    requested_gpu: bool,
    force_gpu_failure: bool,
) {
    let result = worker(
        dll,
        path,
        rx,
        &shared,
        &cancel,
        requested_gpu,
        force_gpu_failure,
    );
    if let Ok(mut s) = shared.lock() {
        s.frame = None;
        match result {
            Ok(()) => {
                s.snapshot.state = PlaybackState::Stopped;
                s.snapshot.error = None
            }
            Err(e) => {
                s.snapshot.state = PlaybackState::Error;
                s.snapshot.error = Some(e)
            }
        }
    }
}
fn worker(
    dll: PathBuf,
    path: String,
    rx: Receiver<Command>,
    shared: &Mutex<Shared>,
    cancel: &AtomicBool,
    requested_gpu: bool,
    force_gpu_failure: bool,
) -> std::result::Result<(), String> {
    unsafe {
        let api = Api::load(&dll)?;
        let handle = (api.create)();
        if handle.is_null() {
            return Err("Cannot create video engine".into());
        }
        let mut core = Core {
            api,
            handle,
            render: std::ptr::null_mut(),
            notify: Box::new(AtomicBool::new(false)),
            gpu: None,
        };
        for (key, value) in [
            ("vo", "libmpv"),
            ("config", "no"),
            ("terminal", "no"),
            ("input-default-bindings", "no"),
            ("osc", "no"),
            ("load-scripts", "no"),
            ("ytdl", "no"),
            ("autoload-files", "no"),
            ("sub-auto", "no"),
            ("audio-file-auto", "no"),
            ("access-references", "no"),
            ("demuxer-lavf-o", "protocol_whitelist=file"),
            ("pause", "yes"),
            ("keep-open", "yes"),
            ("idle", "yes"),
            ("video-timing-offset", "0"),
        ] {
            let k = CString::new(key).unwrap();
            let v = CString::new(value).unwrap();
            core.api
                .check((core.api.option)(handle, k.as_ptr(), v.as_ptr()))
                .map_err(|e| format!("Video engine option {key}: {e}"))?;
        }
        let hwdec = if requested_gpu { c"auto-safe" } else { c"no" };
        core.api
            .check((core.api.option)(handle, c"hwdec".as_ptr(), hwdec.as_ptr()))
            .map_err(|e| format!("Video decoding option: {e}"))?;
        core.api
            .check((core.api.initialize)(handle))
            .map_err(|e| format!("Video engine initialization: {e}"))?;
        for (id, name, format) in [
            (1, "time-pos", 5),
            (2, "duration", 5),
            (3, "pause", 3),
            (4, "volume", 5),
            (5, "video-params/w", 4),
            (6, "video-params/h", 4),
            (7, "audio-params/samplerate", 4),
            (8, "eof-reached", 3),
            (9, "hwdec-current", 1),
        ] {
            let name = CString::new(name).unwrap();
            core.api
                .check((core.api.observe)(handle, id, name.as_ptr(), format))?;
        }
        let mut fallback_reason = None;
        if requested_gpu {
            match if force_gpu_failure {
                Err("GPU initialization deliberately unavailable in test".to_owned())
            } else {
                gpu::Gpu::new()
            } {
                Ok(gpu) => {
                    let mut init = gpu::Gpu::init();
                    let mut params = [
                        Param {
                            kind: 1,
                            data: c"opengl".as_ptr().cast_mut().cast(),
                        },
                        Param::new(2, &mut init),
                        Param::end(),
                    ];
                    let result = core.api.check((core.api.render_create)(
                        &mut core.render,
                        handle,
                        params.as_mut_ptr(),
                    ));
                    match result {
                        Ok(()) => core.gpu = Some(gpu),
                        Err(e) => {
                            if !core.render.is_null() {
                                (core.api.render_free)(core.render);
                                core.render = std::ptr::null_mut();
                            }
                            fallback_reason = Some(format!("OpenGL video renderer: {e}"));
                            drop(gpu);
                        }
                    }
                }
                Err(e) => fallback_reason = Some(e),
            }
        }
        if core.gpu.is_none() {
            let mut params = [
                Param {
                    kind: 1,
                    data: c"sw".as_ptr().cast_mut().cast(),
                },
                Param::end(),
            ];
            core.api
                .check((core.api.render_create)(
                    &mut core.render,
                    handle,
                    params.as_mut_ptr(),
                ))
                .map_err(|e| format!("Software video renderer: {e}"))?;
            if requested_gpu {
                issue(&core.api, handle, &["set", "hwdec", "no"])?;
            }
        }
        shared
            .lock()
            .map_err(|_| "Playback state unavailable")?
            .snapshot
            .acceleration = Some(Acceleration {
            requested_gpu,
            renderer: if core.gpu.is_some() { "gpu" } else { "cpu" }.into(),
            decoder: if core.gpu.is_some() { "unknown" } else { "no" }.into(),
            fallback_reason,
        });
        (core.api.render_callback)(
            core.render,
            Some(render_notify),
            (&mut *core.notify as *mut AtomicBool).cast(),
        );
        issue(&core.api, handle, &["loadfile", &path, "replace"])?;
        if cancel.load(Ordering::Acquire) {
            return Ok(());
        }
        shared
            .lock()
            .map_err(|_| "Playback state unavailable")?
            .last_poll = Instant::now();
        let mut serial = 0u64;
        let mut next_frame = Instant::now();
        let mut loaded = false;
        let mut paused = true;
        let mut ended = false;
        let mut frame_pending = false;
        while !cancel.load(Ordering::Acquire) {
            let expired = shared
                .lock()
                .map_err(|_| "Playback state unavailable")
                .map(|s| {
                    matches!(
                        s.snapshot.state,
                        PlaybackState::Loading | PlaybackState::Playing
                    ) && s.last_poll.elapsed() >= Duration::from_millis(1500)
                })?;
            if expired {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(12)) {
                Ok(Command::Close) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Command::Control(action, reply)) => {
                    let result = match action {
                        Action::Play => issue(&core.api, handle, &["set", "pause", "no"]),
                        Action::Pause => issue(&core.api, handle, &["set", "pause", "yes"]),
                        Action::Seek { position_ms } => issue(
                            &core.api,
                            handle,
                            &[
                                "seek",
                                &(position_ms as f64 / 1000.0).to_string(),
                                "absolute+exact",
                            ],
                        ),
                        Action::SetVolume { volume } => issue(
                            &core.api,
                            handle,
                            &["set", "volume", &(volume * 100.0).to_string()],
                        ),
                        Action::Stop => break,
                    };
                    let _ = reply.send(result);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if cancel.load(Ordering::Acquire) {
                break;
            }
            for _ in 0..128 {
                let event = &*(core.api.wait)(handle, 0.0);
                if event.id == 0 {
                    break;
                }
                if event.id == 24 {
                    return Err("Video engine event queue overflow".into());
                }
                if event.id == 5 && event.error < 0 {
                    return Err(format!(
                        "Video command failed: {}",
                        core.api.check(event.error).unwrap_err()
                    ));
                }
                if event.id == 8 {
                    loaded = true;
                    ended = false
                }
                if event.id == 7 && !event.data.is_null() {
                    let e = &*event.data.cast::<EndFile>();
                    if e.reason == 4 {
                        return Err(format!(
                            "Video load failed: {}",
                            core.api
                                .check(e.error)
                                .err()
                                .unwrap_or_else(|| "unknown error".into())
                        ));
                    }
                    if e.reason == 0 {
                        ended = true;
                    }
                }
                if event.id == 22 && !event.data.is_null() {
                    let p = &*event.data.cast::<Property>();
                    let mut s = shared.lock().map_err(|_| "Playback state unavailable")?;
                    if p.data.is_null() {
                        if event.userdata == 7 {
                            s.snapshot.has_audio = false;
                        }
                        if event.userdata == 9
                            && let Some(a) = s.snapshot.acceleration.as_mut()
                        {
                            a.decoder = "no".into();
                        }
                        continue;
                    }
                    match (event.userdata, p.format) {
                        (1, 5) => {
                            s.snapshot.position_ms =
                                (*p.data.cast::<f64>() * 1000.0).max(0.0) as u64
                        }
                        (2, 5) => {
                            let d = *p.data.cast::<f64>();
                            s.snapshot.duration_ms = if d.is_finite() && d >= 0.0 {
                                Some((d * 1000.0) as u64)
                            } else {
                                None
                            }
                        }
                        (3, 3) => paused = *p.data.cast::<c_int>() != 0,
                        (4, 5) => {
                            s.snapshot.volume = (*p.data.cast::<f64>() / 100.0).clamp(0.0, 1.0)
                        }
                        (5, 4) => {
                            s.snapshot.width =
                                (*p.data.cast::<i64>()).clamp(0, u32::MAX as i64) as u32
                        }
                        (6, 4) => {
                            s.snapshot.height =
                                (*p.data.cast::<i64>()).clamp(0, u32::MAX as i64) as u32
                        }
                        (7, 4) => s.snapshot.has_audio = *p.data.cast::<i64>() > 0,
                        (8, 3) => ended = *p.data.cast::<c_int>() != 0,
                        (9, 1) => {
                            let text = *p.data.cast::<*const c_char>();
                            if !text.is_null()
                                && let Some(a) = s.snapshot.acceleration.as_mut()
                            {
                                a.decoder = CStr::from_ptr(text).to_string_lossy().into_owned();
                            }
                        }
                        _ => {}
                    }
                }
            }
            let (w, h) = {
                let mut s = shared.lock().map_err(|_| "Playback state unavailable")?;
                s.snapshot.state = if ended {
                    PlaybackState::Ended
                } else if !loaded {
                    PlaybackState::Loading
                } else if paused {
                    PlaybackState::Paused
                } else {
                    PlaybackState::Playing
                };
                (s.snapshot.width, s.snapshot.height)
            };
            if core.notify.swap(false, Ordering::AcqRel) {
                frame_pending |= (core.api.render_update)(core.render) & 1 != 0;
            }
            if !frame_pending || w == 0 || h == 0 || Instant::now() < next_frame {
                continue;
            }
            frame_pending = false;
            next_frame = Instant::now() + Duration::from_millis(34);
            let scale = (1280.0 / w as f64).min(720.0 / h as f64).min(1.0);
            let w = ((w as f64 * scale) as u32).max(1);
            let h = ((h as f64 * scale) as u32).max(1);
            if let Some(gpu) = core.gpu.as_mut() {
                let mut fbo = gpu.prepare(w, h)?;
                // read_pixels starts at the bottom GL row; mpv's unflipped FBO
                // places the source top row there for our top-down RGBA buffer.
                let mut flip = 0i32;
                let mut block = 0i32;
                let mut params = [
                    Param::new(3, &mut fbo),
                    Param::new(4, &mut flip),
                    Param::new(12, &mut block),
                    Param::end(),
                ];
                core.api
                    .check((core.api.render)(core.render, params.as_mut_ptr()))
                    .map_err(|e| format!("GPU video frame render: {e}"))?;
                let rgba = gpu.read(w, h)?;
                serial = serial.saturating_add(1);
                if let Ok(mut s) = shared.lock() {
                    s.frame = Some(Frame {
                        serial,
                        width: w,
                        height: h,
                        rgba,
                    });
                }
                continue;
            }
            let stride = ((w as usize * 4) + 63) & !63;
            let len = stride * h as usize;
            let mut buffer = vec![0u8; len + 63];
            let offset = (64 - (buffer.as_ptr() as usize % 64)) % 64;
            let ptr = buffer.as_mut_ptr().add(offset);
            let mut size = [w as c_int, h as c_int];
            let mut stride_param = stride;
            let mut block = 0i32;
            let mut params = [
                Param::new(17, &mut size),
                Param {
                    kind: 18,
                    data: c"rgb0".as_ptr().cast_mut().cast(),
                },
                Param::new(19, &mut stride_param),
                Param {
                    kind: 20,
                    data: ptr.cast(),
                },
                Param::new(12, &mut block),
                Param::end(),
            ];
            core.api
                .check((core.api.render)(core.render, params.as_mut_ptr()))
                .map_err(|e| format!("Video frame render: {e}"))?;
            let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
            for y in 0..h as usize {
                let row = &buffer[offset + y * stride..offset + y * stride + w as usize * 4];
                rgba.extend_from_slice(row)
            }
            for p in rgba.chunks_exact_mut(4) {
                p[3] = 255
            }
            serial = serial.saturating_add(1);
            if let Ok(mut s) = shared.lock() {
                s.frame = Some(Frame {
                    serial,
                    width: w,
                    height: h,
                    rgba,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires pinned KATRAN_MPV_DLL and synthetic target fixtures"]
    fn synthetic_mpv_muted_unicode() {
        let source = std::env::var_os("KATRAN_MEDIA_SYNTHETIC_MP4").expect("set synthetic fixture");
        exercise(PathBuf::from(source), true, false, false);
        if let Some(mkv) = std::env::var_os("KATRAN_MEDIA_SYNTHETIC_MKV") {
            exercise(PathBuf::from(mkv), false, false, false)
        }
    }
    #[test]
    #[ignore = "requires a real GPU and pinned synthetic fixtures"]
    fn synthetic_mpv_gpu_muted_unicode() {
        let source = std::env::var_os("KATRAN_MEDIA_SYNTHETIC_MP4").expect("set synthetic fixture");
        exercise(PathBuf::from(source), true, true, false);
        if let Some(mkv) = std::env::var_os("KATRAN_MEDIA_SYNTHETIC_MKV") {
            exercise(PathBuf::from(mkv), false, true, false)
        }
    }
    #[test]
    #[ignore = "requires pinned synthetic fixture"]
    fn synthetic_mpv_forced_gpu_fallback() {
        let source = std::env::var_os("KATRAN_MEDIA_SYNTHETIC_MP4").expect("set synthetic fixture");
        exercise(PathBuf::from(source), false, true, true);
    }
    #[test]
    #[ignore = "requires oriented synthetic fixture and a real GPU"]
    fn synthetic_mpv_gpu_orientation() {
        let source = PathBuf::from(
            std::env::var_os("KATRAN_MEDIA_ORIENTATION_MP4").expect("orientation fixture"),
        );
        let target = std::fs::canonicalize("target").unwrap();
        let source = std::fs::canonicalize(source).unwrap();
        assert!(source.starts_with(&target));
        let temp = tempfile::tempdir_in(&target).unwrap();
        let path = temp.path().join("Ориентация.mp4");
        std::fs::copy(source, &path).unwrap();
        for use_gpu in [false, true] {
            let mut backend = MpvBackend::with_gpu(use_gpu);
            backend.open(Source { path: path.clone() }).unwrap();
            backend.control(Action::SetVolume { volume: 0.0 }).unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let s = backend.snapshot().unwrap();
                assert_ne!(s.state, PlaybackState::Error, "{:?}", s.error);
                if s.duration_ms.is_some() && s.width > 0 {
                    break;
                }
                assert!(Instant::now() < deadline, "metadata timeout: {s:?}");
                thread::sleep(Duration::from_millis(25));
            }
            backend.control(Action::Play).unwrap();
            let frame = loop {
                let s = backend.snapshot().unwrap();
                assert_ne!(s.state, PlaybackState::Error, "{:?}", s.error);
                if let Some(frame) = backend.frame(0).unwrap()
                    && frame.rgba.chunks_exact(4).any(|p| p[0] > 100 || p[2] > 100)
                {
                    let a = s.acceleration.unwrap();
                    assert_eq!(
                        a.renderer,
                        if use_gpu { "gpu" } else { "cpu" },
                        "{:?}",
                        a.fallback_reason
                    );
                    break frame;
                }
                assert!(Instant::now() < deadline, "frame timeout");
                thread::sleep(Duration::from_millis(25));
            };
            let sample = |y: usize| {
                let offset = (y * frame.width as usize + frame.width as usize / 2) * 4;
                &frame.rgba[offset..offset + 4]
            };
            let top = sample(frame.height as usize / 4);
            let bottom = sample(frame.height as usize * 3 / 4);
            assert!(
                top[0] as i16 > top[2] as i16 + 100
                    && bottom[2] as i16 > bottom[0] as i16 + 100
                    && top[3] == 255
                    && bottom[3] == 255,
                "GPU={use_gpu} top={top:?} bottom={bottom:?}"
            );
            backend.close();
        }
    }
    fn exercise(source: PathBuf, watchdog: bool, requested_gpu: bool, force_fallback: bool) {
        let target = std::fs::canonicalize("target").unwrap();
        let source = std::fs::canonicalize(source).unwrap();
        assert!(source.starts_with(&target));
        let tmp = tempfile::tempdir_in(&target).unwrap();
        let suffix = source.extension().unwrap().to_str().unwrap();
        let path = tmp.path().join(format!("Кириллица видео.{suffix}"));
        std::fs::copy(source, path.clone()).unwrap();
        let mut backend = if force_fallback {
            MpvBackend::with_forced_fallback()
        } else {
            MpvBackend::with_gpu(requested_gpu)
        };
        backend.open(Source { path: path.clone() }).unwrap();
        backend.control(Action::SetVolume { volume: 0.0 }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let s = backend.snapshot().unwrap();
            assert_ne!(s.state, PlaybackState::Error, "{:?}", s.error);
            if s.duration_ms.is_some() && s.width > 0 && s.height > 0 {
                break;
            }
            assert!(Instant::now() < deadline, "metadata timed out: {s:?}");
            thread::sleep(Duration::from_millis(25));
        }
        assert_ne!(
            backend.snapshot().unwrap().state,
            PlaybackState::Playing,
            "unexpected autoplay"
        );
        backend.control(Action::Play).unwrap();
        loop {
            let s = backend.snapshot().unwrap();
            assert_ne!(s.state, PlaybackState::Error, "{:?}", s.error);
            if let Some(f) = backend.frame(0).unwrap()
                && s.has_audio
                && f.rgba
                    .chunks_exact(4)
                    .any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
            {
                let a = s.acceleration.as_ref().expect("acceleration status");
                assert_eq!(a.requested_gpu, requested_gpu);
                assert_eq!(
                    a.renderer,
                    if requested_gpu && !force_fallback {
                        "gpu"
                    } else {
                        "cpu"
                    },
                    "fallback: {:?}",
                    a.fallback_reason
                );
                if force_fallback {
                    assert!(a.fallback_reason.is_some());
                }
                eprintln!("synthetic renderer={}, decoder={}", a.renderer, a.decoder);
                assert!(f.width <= 1280 && f.height <= 720);
                assert_eq!(f.rgba.len(), f.width as usize * f.height as usize * 4);
                break;
            }
            assert!(Instant::now() < deadline, "frame/audio timed out: {s:?}");
            thread::sleep(Duration::from_millis(25));
        }
        backend.control(Action::Pause).unwrap();
        wait_for(&mut backend, |s| s.state == PlaybackState::Paused, "pause");
        backend.control(Action::Seek { position_ms: 100 }).unwrap();
        wait_for(&mut backend, |s| s.position_ms < 600, "seek");
        if watchdog {
            let duration = backend.snapshot().unwrap().duration_ms.unwrap();
            assert!(duration >= 2800);
            backend
                .control(Action::Seek {
                    position_ms: duration - 500,
                })
                .unwrap();
            wait_for(
                &mut backend,
                |s| s.position_ms >= duration - 850,
                "seek near end",
            );
            backend.control(Action::Play).unwrap();
            wait_for(
                &mut backend,
                |s| s.state == PlaybackState::Ended,
                "end of file",
            );
            backend.control(Action::Seek { position_ms: 0 }).unwrap();
            backend.control(Action::Play).unwrap();
            wait_for(
                &mut backend,
                |s| {
                    s.state == PlaybackState::Playing && s.position_ms > 150 && s.position_ms < 1800
                },
                "replay",
            );
            thread::sleep(Duration::from_millis(2300));
            let s = backend.snapshot().unwrap();
            assert_eq!(s.state, PlaybackState::Stopped, "watchdog: {s:?}");
            assert!(backend.frame(0).unwrap().is_none());
        }
        let start = Instant::now();
        backend.close();
        assert!(start.elapsed() < Duration::from_secs(5));
        std::fs::rename(&path, tmp.path().join("released.video")).expect("file released");
    }
    fn wait_for(backend: &mut MpvBackend, predicate: impl Fn(&Snapshot) -> bool, label: &str) {
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            let s = backend.snapshot().unwrap();
            assert_ne!(s.state, PlaybackState::Error, "{:?}", s.error);
            if predicate(&s) {
                break;
            }
            assert!(Instant::now() < until, "{label} timed out: {s:?}");
            thread::sleep(Duration::from_millis(25));
        }
    }
}

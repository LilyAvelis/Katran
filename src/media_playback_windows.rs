//! Media Foundation frame-server adapter. The worker owns COM, the engine and
//! its audio stream. Only the latest bounded CPU frame crosses the boundary.
use crate::{
    ApiError, Result,
    media_playback::{Action, Backend, Frame, PlaybackState, Snapshot, Source},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HANDLE, RECT},
        Graphics::{
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Imaging::{
                CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
                WICBitmapCacheOnLoad,
            },
        },
        Media::MediaFoundation::*,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Threading::SetEvent,
        },
    },
    core::{BSTR, IUnknown, Interface, implement},
};

fn failure(code: &str, message: impl ToString) -> ApiError {
    ApiError::new(code, message)
}
fn unavailable(e: impl ToString) -> ApiError {
    failure("media_unavailable", e)
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
pub struct NativeBackend {
    worker: Option<JoinHandle<()>>,
    tx: Option<SyncSender<Command>>,
    shared: Option<Arc<Mutex<Shared>>>,
    cancel: Option<Arc<AtomicBool>>,
}
impl Backend for NativeBackend {
    fn open(&mut self, source: Source) -> Result<()> {
        self.close();
        // Only absolute local paths reach Media Engine; never accept a URL or UNC.
        use std::path::Component;
        if !source.path.is_absolute()
            || !matches!(source.path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
        {
            return Err(failure(
                "media_invalid_source",
                "A local absolute file path is required",
            ));
        }
        let metadata = std::fs::metadata(&source.path).map_err(|e| {
            failure(
                "media_source_unavailable",
                format!("Cannot access the media file: {e}"),
            )
        })?;
        if !metadata.is_file() {
            return Err(failure(
                "media_invalid_source",
                "The media source is not a file",
            ));
        }
        let url = url::Url::from_file_path(&source.path)
            .map_err(|_| failure("media_invalid_source", "Cannot form a local file URL"))?
            .to_string();
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
        let worker = thread::Builder::new()
            .name("katran-media-engine".into())
            .spawn(move || run(url, rx, data, stop))
            .map_err(unavailable)?;
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
        if let Some(shared) = &self.shared {
            shared.lock().map_err(unavailable)?.last_poll = Instant::now();
        }
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| failure("media_stopped", "No active playback"))?;
        let (reply, received) = mpsc::sync_channel(1);
        tx.try_send(Command::Control(action, reply))
            .map_err(|_| failure("media_busy", "Playback worker is busy or stopped"))?;
        match received.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(failure("media_control_failed", e)),
            Err(_) => {
                self.close();
                Err(failure(
                    "media_unavailable",
                    "Playback worker did not respond",
                ))
            }
        }
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        match &self.shared {
            Some(s) => {
                let mut shared = s.lock().map_err(unavailable)?;
                shared.last_poll = Instant::now();
                Ok(shared.snapshot.clone())
            }
            None => Ok(empty()),
        }
    }
    fn frame(&mut self, after_serial: u64) -> Result<Option<Frame>> {
        match &self.shared {
            Some(s) => {
                let mut shared = s.lock().map_err(unavailable)?;
                shared.last_poll = Instant::now();
                Ok(shared
                    .frame
                    .as_ref()
                    .filter(|f| f.serial > after_serial)
                    .cloned())
            }
            None => Ok(None),
        }
    }
    fn close(&mut self) {
        if let Some(stop) = self.cancel.take() {
            stop.store(true, Ordering::Release);
        }
        if let Some(tx) = self.tx.take() {
            let _ = tx.try_send(Command::Close);
            drop(tx);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.shared = None;
    }
}
impl Drop for NativeBackend {
    fn drop(&mut self) {
        self.close();
    }
}

#[implement(IMFMediaEngineNotify)]
struct Notify {
    failed: Arc<AtomicU64>,
}
impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, param1: usize, param2: u32) -> windows::core::Result<()> {
        if event == MF_MEDIA_ENGINE_EVENT_ERROR.0 as u32 {
            self.failed.store(
                ((param1 as u64) << 32) | u64::from(param2) | (1u64 << 63),
                Ordering::Release,
            );
        }
        // Normally not sent: CreateInstance uses flags=0. Honour it if Windows sends it.
        if event == MF_MEDIA_ENGINE_EVENT_NOTIFYSTABLESTATE.0 as u32 {
            unsafe {
                SetEvent(HANDLE(param1 as *mut _))?;
            }
        }
        Ok(())
    }
}
struct Lifetime;
impl Drop for Lifetime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
struct Engine(IMFMediaEngine);
impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.Pause();
            let _ = self.0.Shutdown();
        }
    }
}

fn run(url: String, rx: Receiver<Command>, shared: Arc<Mutex<Shared>>, cancel: Arc<AtomicBool>) {
    let result = worker(url, rx, &shared, &cancel);
    if let Ok(mut s) = shared.lock() {
        s.frame = None;
        match result {
            Ok(()) => {
                s.snapshot.state = PlaybackState::Stopped;
                s.snapshot.error = None;
            }
            Err(e) => {
                s.snapshot.state = PlaybackState::Error;
                s.snapshot.error = Some(e);
            }
        }
    }
}
fn worker(
    url: String,
    rx: Receiver<Command>,
    shared: &Mutex<Shared>,
    cancel: &AtomicBool,
) -> std::result::Result<(), String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
            CoUninitialize();
            return Err(e.to_string());
        }
        let _lifetime = Lifetime;
        let failed = Arc::new(AtomicU64::new(0));
        let notify: IMFMediaEngineNotify = Notify {
            failed: Arc::clone(&failed),
        }
        .into();
        let mut attr = None;
        MFCreateAttributes(&mut attr, 2).map_err(|e| e.to_string())?;
        let attr = attr.ok_or("Media Foundation attributes unavailable")?;
        attr.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)
            .map_err(|e| e.to_string())?;
        attr.SetUINT32(
            &MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT,
            DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32,
        )
        .map_err(|e| e.to_string())?;
        let factory: IMFMediaEngineClassFactory =
            CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        let engine = Engine(
            factory
                .CreateInstance(0, &attr)
                .map_err(|e| e.to_string())?,
        );
        engine.0.SetAutoPlay(false).map_err(|e| e.to_string())?;
        engine
            .0
            .SetSource(&BSTR::from(url))
            .map_err(|e| e.to_string())?;
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        let mut serial = 0u64;
        let mut next_frame = Instant::now();
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
            match rx.recv_timeout(Duration::from_millis(34)) {
                Ok(Command::Close) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Command::Control(action, reply)) => {
                    let result = match action {
                        Action::Play => engine.0.Play(),
                        Action::Pause => engine.0.Pause(),
                        Action::Seek { position_ms } => {
                            engine.0.SetCurrentTime(position_ms as f64 / 1000.0)
                        }
                        Action::SetVolume { volume } => engine.0.SetVolume(volume),
                        Action::Stop => break,
                    };
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let event_error = failed.load(Ordering::Acquire);
            if event_error != 0 {
                let event_code = ((event_error >> 32) & 0x7fff_ffff) as u32;
                let native_code = engine
                    .0
                    .GetError()
                    .ok()
                    .map(|e| e.GetErrorCode())
                    .unwrap_or(event_code as u16);
                let extended = event_error as u32;
                return Err(format!(
                    "Media load failed (engine code {native_code}, HRESULT 0x{extended:08X})"
                ));
            }
            let ready = engine.0.GetReadyState() >= MF_MEDIA_ENGINE_READY_HAVE_METADATA.0 as u16;
            let duration = engine.0.GetDuration();
            let mut width = 0;
            let mut height = 0;
            if ready {
                let _ = engine
                    .0
                    .GetNativeVideoSize(Some(&mut width), Some(&mut height));
            }
            let state = if !ready {
                PlaybackState::Loading
            } else if engine.0.IsEnded().as_bool() {
                PlaybackState::Ended
            } else if engine.0.IsPaused().as_bool() {
                PlaybackState::Paused
            } else {
                PlaybackState::Playing
            };
            if let Ok(mut s) = shared.lock() {
                s.snapshot = Snapshot {
                    state,
                    duration_ms: if duration.is_finite() && duration >= 0.0 {
                        Some((duration * 1000.0) as u64)
                    } else {
                        None
                    },
                    position_ms: (engine.0.GetCurrentTime().max(0.0) * 1000.0) as u64,
                    volume: engine.0.GetVolume(),
                    has_audio: engine.0.HasAudio().as_bool(),
                    width,
                    height,
                    error: None,
                };
            }
            if width == 0 || height == 0 || Instant::now() < next_frame {
                continue;
            }
            next_frame = Instant::now() + Duration::from_millis(34);
            // The generated Result wrapper loses S_FALSE, which means no new frame.
            let mut timestamp = 0i64;
            let tick = (Interface::vtable(&engine.0).OnVideoStreamTick)(
                Interface::as_raw(&engine.0),
                &mut timestamp,
            );
            if tick.0 == 1 {
                continue;
            }
            tick.ok()
                .map_err(|e| format!("Video frame tick failed: {e}"))?;
            let scale = (1280.0 / width as f64).min(720.0 / height as f64).min(1.0);
            let w = ((width as f64 * scale) as u32).max(1);
            let h = ((height as f64 * scale) as u32).max(1);
            let bitmap = wic
                .CreateBitmap(w, h, &GUID_WICPixelFormat32bppBGRA, WICBitmapCacheOnLoad)
                .map_err(|e| e.to_string())?;
            let surface: IUnknown = bitmap.cast().map_err(|e| e.to_string())?;
            let rect = RECT {
                left: 0,
                top: 0,
                right: w as i32,
                bottom: h as i32,
            };
            engine
                .0
                .TransferVideoFrame(&surface, None, &rect, None)
                .map_err(|e| e.to_string())?;
            let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
            bitmap
                .CopyPixels(std::ptr::null(), w * 4, &mut rgba)
                .map_err(|e| e.to_string())?;
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                pixel[3] = 255;
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an explicitly supplied synthetic MP4 with an audio track"]
    fn synthetic_muted_playback_and_release() {
        let source =
            std::env::var_os("KATRAN_MEDIA_SYNTHETIC_MP4").expect("set synthetic MP4 path");
        exercise(std::path::PathBuf::from(source), true);
        if let Some(source) = std::env::var_os("KATRAN_MEDIA_SYNTHETIC_MKV") {
            exercise(std::path::PathBuf::from(source), false);
        }
    }

    fn exercise(source: std::path::PathBuf, watchdog: bool) {
        let target = std::fs::canonicalize("target").unwrap();
        let source = std::fs::canonicalize(source).unwrap();
        assert!(source.starts_with(&target), "fixture must be in target");
        let temp = tempfile::tempdir_in(&target).unwrap();
        let path = temp.path().join(source.file_name().unwrap());
        std::fs::copy(source, &path).unwrap();
        let mut backend = NativeBackend::default();
        backend.open(Source { path: path.clone() }).unwrap();
        backend.control(Action::SetVolume { volume: 0.0 }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let snap = backend.snapshot().unwrap();
            assert_ne!(snap.state, PlaybackState::Error, "{:?}", snap.error);
            if snap.duration_ms.is_some() && snap.width > 0 {
                assert!(snap.has_audio);
                break;
            }
            assert!(Instant::now() < deadline, "metadata timed out: {snap:?}");
            thread::sleep(Duration::from_millis(25));
        }
        backend.control(Action::Play).unwrap();
        let frame = loop {
            let snap = backend.snapshot().unwrap();
            assert_ne!(snap.state, PlaybackState::Error, "{:?}", snap.error);
            if let Some(frame) = backend.frame(0).unwrap() {
                break frame;
            }
            assert!(Instant::now() < deadline, "frame timed out: {snap:?}");
            thread::sleep(Duration::from_millis(25));
        };
        assert!(frame.width <= 1280 && frame.height <= 720);
        assert_eq!(
            frame.rgba.len(),
            frame.width as usize * frame.height as usize * 4
        );
        assert!(
            frame
                .rgba
                .chunks_exact(4)
                .any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0),
            "synthetic video frame is black"
        );
        backend.control(Action::Pause).unwrap();
        backend.control(Action::Seek { position_ms: 100 }).unwrap();
        if watchdog {
            assert!(
                backend.snapshot().unwrap().duration_ms.unwrap() >= 2800,
                "fixture must be at least 2.8 seconds"
            );
            backend.control(Action::Play).unwrap();
            // The abandoned client performs no media polls during this interval.
            thread::sleep(Duration::from_millis(2300));
            let stopped = backend.snapshot().unwrap();
            assert_eq!(
                stopped.state,
                PlaybackState::Stopped,
                "watchdog did not stop: {stopped:?}"
            );
            assert!(
                backend.frame(0).unwrap().is_none(),
                "stopped worker retained a frame"
            );
        }
        let started = Instant::now();
        backend.close();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "normal close was slow"
        );
        std::fs::rename(&path, temp.path().join("released.mp4")).expect("file handle released");
    }
}

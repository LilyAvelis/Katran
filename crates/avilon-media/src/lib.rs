//! One bounded streaming decoder. Frames are staged in CPU memory for egui upload.
#[cfg(feature = "egui")]
use egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::path::PathBuf;
#[cfg(feature = "egui")]
use std::{
    io::Read,
    process::Command,
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoSource {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
    pub smooth_loop: bool,
}
const CACHE_LIMIT: u64 = 512 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackPolicy {
    Cache { bytes: u64, frames: u32 },
    Streaming { bytes: u64 },
}
fn output_size(source: &VideoSource) -> (usize, usize) {
    let scale = (1280.0 / source.width as f64)
        .min(720.0 / source.height as f64)
        .min(1.0);
    let w = (((source.width as f64 * scale).floor() as usize) & !1).max(2);
    let h = (((source.height as f64 * scale).floor() as usize) & !1).max(2);
    (w, h)
}
pub fn playback_policy(source: &VideoSource) -> PlaybackPolicy {
    if source.width == 0
        || source.height == 0
        || source.width > 3840
        || source.height > 2160
        || source.duration_ms == 0
        || source.duration_ms > 120_000
    {
        return PlaybackPolicy::Streaming { bytes: u64::MAX };
    }
    let (w, h) = output_size(source);
    let frames = ((u64::from(source.duration_ms) * 24).div_ceil(1000) + 2) as u32;
    // FFmpeg frame buffers may have aligned rows and heights.
    let stride = (w as u64 + 63) & !63;
    let rows = (h as u64 + 31) & !31;
    let bytes = stride * rows * 3 / 2 * u64::from(frames);
    if bytes <= CACHE_LIMIT {
        PlaybackPolicy::Cache { bytes, frames }
    } else {
        PlaybackPolicy::Streaming { bytes }
    }
}
#[cfg(feature = "egui")]
enum Message {
    Frame { pixels: Vec<u8>, index: u64 },
    Error(String),
}
// Keep only the first and preceding RGBA frames. At a repeated first frame near
// the expected cycle boundary, blend from the last frame into four new frames.
#[cfg(feature = "egui")]
struct SeamSmoother {
    first: Option<Arc<Vec<u8>>>,
    previous: Vec<u8>,
    anchor: Vec<u8>,
    since_start: u32,
    earliest_repeat: u32,
    latest_repeat: u32,
    blend_step: u32,
}
#[cfg(feature = "egui")]
impl SeamSmoother {
    fn new(duration_ms: u32) -> Self {
        let expected = (u64::from(duration_ms) * 24).div_ceil(1000) as u32;
        Self {
            first: None,
            previous: Vec::new(),
            anchor: Vec::new(),
            since_start: 0,
            earliest_repeat: expected.saturating_sub(4).max(1),
            latest_repeat: expected.saturating_add(4),
            blend_step: 0,
        }
    }
    fn apply(&mut self, frame: &mut [u8]) {
        if self.first.is_none() {
            self.first = Some(Arc::new(frame.to_vec()));
        } else if self.since_start >= self.earliest_repeat
            && self.since_start <= self.latest_repeat
            && self.first.as_deref().map(Vec::as_slice) == Some(frame)
        {
            self.anchor.clone_from(&self.previous);
            self.blend_step = 1;
            self.since_start = 0;
        }
        // Preserve the unblended frame for the next boundary.
        if self.since_start >= self.earliest_repeat.saturating_sub(1)
            && self.since_start <= self.latest_repeat
        {
            self.previous.clear();
            self.previous.extend_from_slice(frame);
        }
        if self.blend_step > 0 && self.anchor.len() == frame.len() {
            let weight = self.blend_step;
            for (new, old) in frame.iter_mut().zip(&self.anchor) {
                *new = ((u32::from(*old) * (5 - weight) + u32::from(*new) * weight + 2) / 5) as u8;
            }
            self.blend_step += 1;
            if self.blend_step > 4 {
                self.blend_step = 0;
            }
        }
        self.since_start = self.since_start.saturating_add(1);
    }
}
#[cfg(feature = "egui")]
fn seam_smoother(source: &VideoSource, policy: PlaybackPolicy) -> Option<SeamSmoother> {
    if source.smooth_loop && matches!(policy, PlaybackPolicy::Cache { .. }) {
        Some(SeamSmoother::new(source.duration_ms))
    } else {
        None
    }
}
#[cfg(feature = "egui")]
struct Worker {
    child: Arc<Mutex<Option<Child>>>,
    stopped: Arc<AtomicBool>,
    rx: Receiver<Message>,
    latest: Arc<Mutex<Option<Message>>>,
    #[cfg(test)]
    finished: Arc<AtomicBool>,
}
#[cfg(feature = "egui")]
impl Worker {
    fn stop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock()
            && let Some(c) = child.as_mut()
        {
            let _ = c.kill();
        }
    }
}
#[cfg(feature = "egui")]
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}
#[cfg(feature = "egui")]
#[derive(Default)]
pub struct State {
    source: Option<VideoSource>,
    worker: Option<Worker>,
    texture: Option<TextureHandle>,
    error: Option<String>,
    size: [usize; 2],
    next_frame: u64,
    first_frame: Option<Arc<Vec<u8>>>,
    frame_serial: u64,
}
#[cfg(feature = "egui")]
impl State {
    /// Changes only after a decoded frame has been accepted for presentation.
    pub fn frame_serial(&self) -> u64 {
        self.frame_serial
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        source: Option<VideoSource>,
        active: bool,
        command_factory: impl FnOnce() -> Result<Command, String> + Send + 'static,
    ) -> Option<TextureHandle> {
        if self.source != source {
            self.worker = None;
            let same_video = match (&self.source, &source) {
                (Some(a), Some(b)) => {
                    a.path == b.path
                        && a.width == b.width
                        && a.height == b.height
                        && a.duration_ms == b.duration_ms
                }
                _ => false,
            };
            if !same_video {
                self.texture = None;
                self.next_frame = 0;
                self.first_frame = None;
            }
            self.error = None;
            self.source = source;
        }
        if !active {
            self.worker = None;
            return self.texture.clone();
        }
        if self.worker.is_none()
            && self.error.is_none()
            && let Some(src) = self.source.clone()
        {
            if src.width == 0
                || src.height == 0
                || src.width > 3840
                || src.height > 2160
                || src.duration_ms == 0
                || src.duration_ms > 120_000
            {
                self.error = Some("Invalid video dimensions".into());
            } else {
                let (w, h) = output_size(&src);
                self.size = [w, h];
                self.worker = Some(start(
                    src,
                    w,
                    h,
                    self.next_frame,
                    self.first_frame.clone(),
                    ctx.clone(),
                    command_factory,
                ));
            }
        }
        if let Some(worker) = self.worker.as_ref() {
            let mut messages = Vec::new();
            loop {
                match worker.rx.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        messages.push(Message::Error("Video decoder stopped unexpectedly".into()));
                        break;
                    }
                }
            }
            if let Ok(mut latest) = worker.latest.lock()
                && let Some(message) = latest.take()
            {
                // The queued first frame preserves seam state; the newest
                // hidden frame then replaces it before the window is painted.
                messages.push(message);
            }
            let mut last_index = None;
            for message in messages {
                match message {
                    Message::Frame { pixels, index } => {
                        if last_index.is_some_and(|previous| index < previous) {
                            continue;
                        }
                        last_index = Some(index);
                        if index == 0 && self.first_frame.is_none() {
                            self.first_frame = Some(Arc::new(pixels.clone()));
                        }
                        self.frame_serial = self.frame_serial.wrapping_add(1);
                        self.next_frame =
                            (index + 1) % estimated_cycle_frames(self.source.as_ref().unwrap());
                        let image = ColorImage::from_rgba_unmultiplied(self.size, &pixels);
                        if let Some(t) = self.texture.as_mut() {
                            t.set(image, TextureOptions::LINEAR);
                        } else {
                            self.texture = Some(ctx.load_texture(
                                "video-background",
                                image,
                                TextureOptions::LINEAR,
                            ));
                        }
                    }
                    Message::Error(e) => {
                        self.error = Some(e);
                        break;
                    }
                }
            }
        }
        if self.error.is_some() {
            self.worker = None;
        }
        self.texture.clone()
    }
}
#[cfg(feature = "egui")]
fn estimated_cycle_frames(source: &VideoSource) -> u64 {
    (u64::from(source.duration_ms) * 24).div_ceil(1000).max(1)
}
// Trim after the complete source is looped: seeking before the loop would
// turn the remaining tail into the entire cached cycle. The absolute frame
// cursor is bounded by the estimated frame count from the supplied duration.
// Container duration and actual frame count can differ, so resumption is approximate.
#[cfg(feature = "egui")]
fn video_filter(policy: PlaybackPolicy, w: usize, h: usize, resume_frame: u64) -> String {
    let base = match policy {
        PlaybackPolicy::Cache { frames, .. } => format!(
            "hwdownload,format=nv12,fps=24,scale={w}:{h},format=nv12,loop=loop=-1:size={frames}:start=0"
        ),
        PlaybackPolicy::Streaming { .. } => {
            format!("hwdownload,format=nv12,fps=24,scale={w}:{h}")
        }
    };
    format!("{base},trim=start_frame={resume_frame},setpts=N/(24*TB)")
}
#[cfg(feature = "egui")]
fn start(
    source: VideoSource,
    w: usize,
    h: usize,
    resume_frame: u64,
    first_frame: Option<Arc<Vec<u8>>>,
    ctx: egui::Context,
    command_factory: impl FnOnce() -> Result<Command, String> + Send + 'static,
) -> Worker {
    let (tx, rx) = mpsc::sync_channel(1);
    let latest = Arc::new(Mutex::new(None));
    let latest_writer = latest.clone();
    let child = Arc::new(Mutex::new(None::<Child>));
    let shared = child.clone();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop_flag = stopped.clone();
    #[cfg(test)]
    let finished = Arc::new(AtomicBool::new(false));
    #[cfg(test)]
    let finish_flag = finished.clone();
    thread::spawn(move || {
        #[cfg(test)]
        let _finished = Finished(finish_flag);
        let mut cmd = match command_factory() {
            Ok(command) => command,
            Err(error) => {
                let _ = tx.try_send(Message::Error(error));
                ctx.request_repaint();
                return;
            }
        };
        cmd.args([
            "-v",
            "error",
            "-nostdin",
            "-hwaccel",
            "d3d11va",
            "-hwaccel_output_format",
            "d3d11",
        ]);
        let policy = playback_policy(&source);
        if matches!(policy, PlaybackPolicy::Streaming { .. }) {
            cmd.args(["-stream_loop", "-1"]);
        }
        let filter = video_filter(policy, w, h, resume_frame);
        cmd.arg("-i").arg(&source.path).args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-vf",
            &filter,
            "-fps_mode",
            "passthrough",
            "-pix_fmt",
            "rgba",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut proc = match cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.try_send(Message::Error(format!("FFmpeg unavailable: {e}")));
                ctx.request_repaint();
                return;
            }
        };
        let mut stdout = proc.stdout.take().unwrap();
        // Check cancellation while holding the same lock used by stop().
        let mut unpublished = Some(proc);
        let published = if let Ok(mut slot) = shared.lock() {
            if !stop_flag.load(Ordering::SeqCst) {
                *slot = unpublished.take();
                true
            } else {
                false
            }
        } else {
            false
        };
        if !published {
            // The child was not published, so stop() cannot reach it.
            // Its handle is killed and reaped after the lock is released.
            if let Some(mut p) = unpublished {
                let _ = p.kill();
                let _ = p.wait();
            }
            return;
        }
        let frame_len = w * h * 4;
        let mut smoother = seam_smoother(&source, policy);
        if let (Some(smoother), Some(first)) = (smoother.as_mut(), first_frame) {
            smoother.first = Some(first);
            smoother.since_start = resume_frame as u32;
        }
        let interval = Duration::from_nanos(1_000_000_000 / 24);
        let mut next = Instant::now();
        let mut frame_index = resume_frame;
        while !stop_flag.load(Ordering::SeqCst) {
            while Instant::now() < next && !stop_flag.load(Ordering::SeqCst) {
                thread::sleep(
                    next.saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(10)),
                );
            }
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }
            let mut frame = vec![0; frame_len];
            if stdout.read_exact(&mut frame).is_err() {
                break;
            }
            if let Some(smoother) = smoother.as_mut() {
                smoother.apply(&mut frame);
            }
            next += interval;
            let now = Instant::now();
            if next <= now {
                next = now + interval;
            }
            let index = frame_index;
            frame_index = frame_index.saturating_add(1);
            match tx.try_send(Message::Frame {
                pixels: frame,
                index,
            }) {
                Ok(()) => ctx.request_repaint(),
                Err(mpsc::TrySendError::Full(message)) => {
                    if let Ok(mut slot) = latest_writer.lock() {
                        *slot = Some(message);
                    }
                }
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
        }
        let owned = shared.lock().ok().and_then(|mut slot| slot.take());
        if let Some(mut p) = owned {
            let _ = p.kill();
            let _ = p.wait();
        }
        if !stop_flag.load(Ordering::SeqCst) {
            let _ = tx.try_send(Message::Error(
                "Hardware video decoding failed; check D3D11VA and FFmpeg".into(),
            ));
            ctx.request_repaint();
        }
    });
    Worker {
        child,
        stopped,
        rx,
        latest,
        #[cfg(test)]
        finished,
    }
}

#[cfg(all(test, feature = "egui"))]
struct Finished(Arc<AtomicBool>);
#[cfg(all(test, feature = "egui"))]
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[cfg(all(test, feature = "egui"))]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn hidden_playback_presenter_uses_latest_frame_after_queued_first() {
        let source = VideoSource {
            path: PathBuf::from("synthetic.mp4"),
            width: 2,
            height: 2,
            duration_ms: 1000,
            smooth_loop: false,
        };
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(Message::Frame {
            pixels: vec![10; 16],
            index: 0,
        })
        .unwrap();
        let latest = Arc::new(Mutex::new(Some(Message::Frame {
            pixels: vec![90; 16],
            index: 12,
        })));
        let mut state = State {
            source: Some(source.clone()),
            worker: Some(Worker {
                child: Arc::new(Mutex::new(None)),
                stopped: Arc::new(AtomicBool::new(false)),
                rx,
                latest,
                finished: Arc::new(AtomicBool::new(false)),
            }),
            size: [2, 2],
            ..Default::default()
        };
        let ctx = egui::Context::default();
        assert!(
            state
                .tick(&ctx, Some(source), true, || unreachable!())
                .is_some()
        );
        assert_eq!(state.frame_serial(), 2);
        assert_eq!(state.next_frame, 13);
        assert_eq!(state.first_frame.as_deref().unwrap()[0], 10);
    }

    #[test]
    fn seam_blends_first_four_repeated_frames_without_black_frame() {
        let mut s = SeamSmoother::new(1000);
        let mut first = [20, 20, 20, 255];
        s.apply(&mut first);
        for _ in 1..24 {
            let mut f = [100, 100, 100, 255];
            s.apply(&mut f);
        }
        let mut f = [20, 20, 20, 255];
        s.apply(&mut f);
        assert_eq!(f, [84, 84, 84, 255]);
        let mut f = [30, 30, 30, 255];
        s.apply(&mut f);
        assert_eq!(f, [72, 72, 72, 255]);
        let mut f = [40, 40, 40, 255];
        s.apply(&mut f);
        assert_eq!(f, [64, 64, 64, 255]);
        let mut f = [50, 50, 50, 255];
        s.apply(&mut f);
        assert_eq!(f, [60, 60, 60, 255]);
        let mut f = [60, 60, 60, 255];
        s.apply(&mut f);
        assert_eq!(f, [60, 60, 60, 255]);
    }
    #[test]
    fn resume_smooths_at_original_cycle_boundary() {
        let mut s = SeamSmoother::new(1000);
        s.first = Some(Arc::new(vec![20, 20, 20, 255]));
        s.since_start = 20;
        for _ in 20..24 {
            let mut f = [100, 100, 100, 255];
            s.apply(&mut f);
            assert_eq!(f, [100, 100, 100, 255]);
        }
        let mut first = [20, 20, 20, 255];
        s.apply(&mut first);
        assert_eq!(first, [84, 84, 84, 255]);
    }
    #[test]
    fn changing_video_discards_saved_first_frame() {
        let ctx = egui::Context::default();
        let mut state = State {
            first_frame: Some(Arc::new(vec![1])),
            ..State::default()
        };
        let src = VideoSource {
            path: PathBuf::from("other.mp4"),
            width: 2,
            height: 2,
            duration_ms: 1000,
            smooth_loop: true,
        };
        state.tick(&ctx, Some(src), false, || unreachable!());
        assert!(state.first_frame.is_none());
    }
    #[test]
    fn disabled_smoothing_leaves_frames_unchanged() {
        let source = VideoSource {
            path: PathBuf::new(),
            width: 160,
            height: 90,
            duration_ms: 1000,
            smooth_loop: false,
        };
        let mut smoother = seam_smoother(&source, playback_policy(&source));
        assert!(smoother.is_none());
        let original = vec![20, 80, 130, 255];
        let mut frame = original.clone();
        if let Some(s) = smoother.as_mut() {
            s.apply(&mut frame);
        }
        assert_eq!(frame, original);
    }
    #[test]
    fn smoothing_toggle_keeps_existing_texture() {
        let ctx = egui::Context::default();
        let src = VideoSource {
            path: PathBuf::from("video.mp4"),
            width: 160,
            height: 90,
            duration_ms: 1000,
            smooth_loop: true,
        };
        let texture = ctx.load_texture(
            "test-seam",
            ColorImage::from_rgba_unmultiplied([1, 1], &[10, 20, 30, 255]),
            TextureOptions::LINEAR,
        );
        let id = texture.id();
        let mut state = State {
            source: Some(src.clone()),
            texture: Some(texture),
            ..State::default()
        };
        let changed = VideoSource {
            smooth_loop: false,
            ..src
        };
        assert_eq!(
            state
                .tick(&ctx, Some(changed), false, || Ok(Command::new("ffmpeg")))
                .unwrap()
                .id(),
            id
        );
    }

    #[cfg(windows)]
    #[test]
    fn hardware_decoder_pause_kills_and_reaps() {
        if std::env::var_os("KATRAN_TEST_VIDEO_D3D11VA").is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.mp4");
        assert!(
            Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=160x90:rate=24",
                    "-t",
                    "2",
                    "-c:v",
                    "libx264",
                    "-y"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let mut state = State::default();
        let ctx = egui::Context::default();
        let src = Some(VideoSource {
            path,
            width: 160,
            height: 90,
            duration_ms: 2000,
            smooth_loop: true,
        });
        assert!(matches!(
            playback_policy(src.as_ref().unwrap()),
            PlaybackPolicy::Cache { .. }
        ));
        state.tick(&ctx, src.clone(), true, || Ok(Command::new("ffmpeg")));
        let finished = state.worker.as_ref().unwrap().finished.clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        while state
            .worker
            .as_ref()
            .unwrap()
            .child
            .lock()
            .unwrap()
            .is_none()
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            state
                .worker
                .as_ref()
                .unwrap()
                .child
                .lock()
                .unwrap()
                .is_some(),
            "decoder did not start"
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut decoded = false;
        while Instant::now() < deadline {
            match state.worker.as_ref().unwrap().rx.try_recv() {
                Ok(Message::Frame { pixels: bytes, .. }) => {
                    assert_eq!(bytes.len(), 160 * 90 * 4);
                    decoded = true;
                    break;
                }
                Ok(Message::Error(error)) => panic!("{error}"),
                Err(TryRecvError::Disconnected) => panic!("decoder exited"),
                Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(10)),
            }
        }
        assert!(decoded, "hardware decoder did not produce a frame");
        let pid = state
            .worker
            .as_ref()
            .unwrap()
            .child
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .id();
        let began = Instant::now();
        let deadline = began + Duration::from_secs(9);
        let mut frames = 1;
        while frames < 110 && Instant::now() < deadline {
            match state.worker.as_ref().unwrap().rx.try_recv() {
                Ok(Message::Frame { .. }) => frames += 1,
                Ok(Message::Error(error)) => panic!("{error}"),
                Err(TryRecvError::Disconnected) => panic!("decoder exited during loop"),
                Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(5)),
            }
        }
        assert_eq!(frames, 110, "cached video did not loop beyond two cycles");
        assert!(
            began.elapsed() > Duration::from_secs(4),
            "output was not paced"
        );
        assert_eq!(
            state
                .worker
                .as_ref()
                .unwrap()
                .child
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .id(),
            pid
        );
        state.next_frame = 20;
        let start = Instant::now();
        state.tick(&ctx, src.clone(), false, || Ok(Command::new("ffmpeg")));
        assert!(start.elapsed() < Duration::from_secs(1), "pause blocked UI");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !finished.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(finished.load(Ordering::SeqCst), "decoder was not reaped");
        state.tick(&ctx, src, true, || Ok(Command::new("ffmpeg")));
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match state.worker.as_ref().unwrap().rx.try_recv() {
                Ok(Message::Frame { index, .. }) => {
                    assert_eq!(index, 20);
                    break;
                }
                Ok(Message::Error(error)) => panic!("{error}"),
                Err(TryRecvError::Disconnected) => panic!("resume decoder exited"),
                Err(TryRecvError::Empty) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(TryRecvError::Empty) => panic!("resumed decoder produced no frame"),
            }
        }
    }
    #[test]
    fn resume_uses_delivered_decoder_index_and_stops_when_inactive() {
        let ctx = egui::Context::default();
        let src = VideoSource {
            path: PathBuf::from("synthetic.mp4"),
            width: 2,
            height: 2,
            duration_ms: 1000,
            smooth_loop: false,
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let mut state = State {
            source: Some(src.clone()),
            size: [2, 2],
            worker: Some(Worker {
                child: Arc::new(Mutex::new(None)),
                stopped: stopped.clone(),
                rx,
                latest: Arc::new(Mutex::new(None)),
                finished: Arc::new(AtomicBool::new(false)),
            }),
            ..State::default()
        };
        // Index 43 represents frames already skipped when the queue was full.
        tx.send(Message::Frame {
            pixels: vec![255; 16],
            index: 43,
        })
        .unwrap();
        let texture = state
            .tick(&ctx, Some(src.clone()), true, || unreachable!())
            .unwrap();
        assert_eq!(state.next_frame, 20);
        let id = texture.id();
        let paused = state
            .tick(&ctx, Some(src), false, || unreachable!())
            .unwrap();
        assert_eq!(paused.id(), id);
        assert!(state.worker.is_none());
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn resume_trim_follows_loop_and_is_bounded_by_duration() {
        let src = VideoSource {
            path: PathBuf::new(),
            width: 160,
            height: 90,
            duration_ms: 1000,
            smooth_loop: true,
        };
        let filter = video_filter(playback_policy(&src), 160, 90, 19);
        assert!(filter.find("loop=loop=-1").unwrap() < filter.find("trim=start_frame=19").unwrap());
        assert_eq!(estimated_cycle_frames(&src), 24);
        let stream = video_filter(PlaybackPolicy::Streaming { bytes: 0 }, 160, 90, 19);
        assert!(stream.contains("trim=start_frame=19"));
    }

    #[test]
    fn policy_enforces_cache_budget() {
        let src = VideoSource {
            path: PathBuf::new(),
            width: 1168,
            height: 784,
            duration_ms: 15042,
            smooth_loop: true,
        };
        assert!(matches!(
            playback_policy(&src),
            PlaybackPolicy::Cache { .. }
        ));
        let long = VideoSource {
            duration_ms: 120_000,
            ..src
        };
        assert!(matches!(
            playback_policy(&long),
            PlaybackPolicy::Streaming { .. }
        ));
    }
}

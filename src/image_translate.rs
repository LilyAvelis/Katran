//! Isolated, bounded Japanese page translation preparation. No installation or downloads.
use crate::{
    ApiError, ImageTranslationSource, Result,
    domain_jobs::Cancellation,
    translate_components::{Components, MODEL},
};
use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const OCR: &str = include_str!("../scripts/image-translate/ocr_page.py");
const MANGA: &str = include_str!("../scripts/image-translate/manga_recognize.py");
const TRANSLATE: &str = include_str!("../scripts/image-translate/translate_page.py");
const RENDER: &str = include_str!("../scripts/image-translate/render_page.py");
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_PNG: u64 = 256 * 1024 * 1024;
const MAX_PIXELS: u64 = 32_000_000;
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct StagedTranslation {
    dir: PathBuf,
    path: PathBuf,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) report: RenderReport,
}
#[derive(Clone, Copy, Default, serde::Deserialize)]
pub(crate) struct RenderReport {
    pub(crate) rendered: u32,
    pub(crate) skipped: u32,
    pub(crate) fallback: u32,
    pub(crate) side: u32,
    pub(crate) left_pad: u32,
}
impl StagedTranslation {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for StagedTranslation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
struct OwnedProcess {
    child: Child,
    #[cfg(windows)]
    job: Option<std::os::windows::io::OwnedHandle>,
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        #[cfg(windows)]
        drop(self.job.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl OwnedProcess {
    fn spawn(mut command: Command, stage: &str) -> Result<Self> {
        let child = command
            .spawn()
            .map_err(|e| error(format!("Cannot start {stage}: {e}")))?;
        let mut process = Self {
            child,
            #[cfg(windows)]
            job: None,
        };
        #[cfg(windows)]
        {
            process.job = Some(
                contain_server(&process.child)
                    .map_err(|e| error(format!("Cannot contain {stage}: {e}")))?,
            );
        }
        Ok(process)
    }
}
#[cfg(windows)]
fn contain_server(child: &Child) -> std::io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    // SAFETY: the unnamed job handle is owned here until transferred to OwnedHandle.
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: the struct and both kernel handles are valid for these calls.
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) == 0
    } {
        return Err(std::io::Error::last_os_error());
    }
    Ok(job)
}
fn error(message: impl ToString) -> ApiError {
    ApiError::new("image_translate_error", message)
}
fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(data)?;
    Ok(())
}
fn create_stage(root: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)?;
    let mut safe = metadata.is_dir() && !metadata.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        safe &= metadata.file_attributes() & 0x400 == 0;
    }
    if !safe {
        return Err(error("Unsafe translation staging directory"));
    }
    let root = root.canonicalize()?;
    for _ in 0..128 {
        let dir = root.join(format!(
            "translate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(error("Could not create translation staging directory"))
}
fn hidden(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}
fn reachable(port: u16) -> bool {
    TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(250)).is_ok()
}
fn server(executable: &Path, models: &Path, cancel: &Cancellation) -> Result<(u16, OwnedProcess)> {
    let socket = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| error(format!("Cannot reserve local translation port: {e}")))?;
    let port = socket.local_addr()?.port();
    drop(socket);
    let mut command = Command::new(executable);
    command
        .arg("serve")
        .env("OLLAMA_MODELS", models)
        .env("OLLAMA_HOST", format!("127.0.0.1:{port}"))
        .env("OLLAMA_NO_CLOUD", "1")
        .env("OLLAMA_MAX_LOADED_MODELS", "1")
        .env("OLLAMA_NUM_PARALLEL", "1")
        .env("OLLAMA_KEEP_ALIVE", "-1");
    hidden(&mut command);
    let mut owned = OwnedProcess::spawn(command, "local translation service")?;
    let start = Instant::now();
    loop {
        cancel.check()?;
        if let Some(status) = owned.child.try_wait()? {
            return Err(error(format!("Local translation service exited: {status}")));
        }
        if reachable(port) {
            return Ok((port, owned));
        }
        if start.elapsed() > Duration::from_secs(30) {
            return Err(error("Local translation service did not start"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
#[derive(Default)]
pub(crate) struct Session {
    runtime: Option<Runtime>,
}

struct Runtime {
    components: Components,
    dir: PathBuf,
    paddle: Option<OwnedProcess>,
    manga: Option<OwnedProcess>,
    ollama: Option<(u16, OwnedProcess)>,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        drop(self.paddle.take());
        drop(self.manga.take());
        drop(self.ollama.take());
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Runtime {
    fn new(root: &Path, components: Components) -> Result<Self> {
        let runtime = Self {
            components,
            dir: create_stage(root)?,
            paddle: None,
            manga: None,
            ollama: None,
        };
        for (name, script) in [
            ("ocr_page.py", OCR),
            ("manga_recognize.py", MANGA),
            ("translate_page.py", TRANSLATE),
            ("render_page.py", RENDER),
        ] {
            write_new(&runtime.dir.join(name), script.as_bytes())?;
        }
        Ok(runtime)
    }

    fn translation_port(&mut self, cancel: &Cancellation) -> Result<u16> {
        if let Some((port, process)) = &mut self.ollama {
            if process.child.try_wait()?.is_none() && reachable(*port) {
                return Ok(*port);
            }
        }
        drop(self.ollama.take());
        let (port, process) = server(
            &self.components.ollama_exe,
            &self.components.ollama_models,
            cancel,
        )?;
        self.ollama = Some((port, process));
        Ok(port)
    }
}

fn recognize(
    worker: &mut Option<OwnedProcess>,
    mut command: Command,
    dir: &Path,
    request: &impl serde::Serialize,
    marker: &str,
    stage: &str,
    cancel: &Cancellation,
) -> Result<()> {
    cancel.check()?;
    if let Some(process) = worker {
        if process.child.try_wait()?.is_some() {
            drop(worker.take());
        }
    }
    if worker.is_none() {
        hidden(&mut command);
        command.stdin(Stdio::piped());
        *worker = Some(OwnedProcess::spawn(command, stage)?);
    }
    let process = worker.as_mut().expect("OCR worker started");
    let result = (|| {
        let mut request = serde_json::to_vec(request).map_err(error)?;
        request.push(b'\n');
        if request.len() > 4096 {
            return Err(error("Translation staging path is too long"));
        }
        process
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| error("OCR request pipe is missing"))?
            .write_all(&request)
            .map_err(|_| error(format!("{stage} worker failed")))?;
        let marker = dir.join(marker);
        let start = Instant::now();
        loop {
            cancel.check()?;
            if marker.try_exists()? {
                return Ok(());
            }
            if let Some(status) = process.child.try_wait()? {
                return Err(error(format!("{stage} failed ({status})")));
            }
            if start.elapsed() > Duration::from_secs(300) {
                return Err(error(format!("{stage} timed out")));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })();
    if result.is_err() {
        drop(worker.take());
    }
    result
}
fn run(mut command: Command, cancel: &Cancellation, stage: &str, timeout: Duration) -> Result<()> {
    cancel.check()?;
    hidden(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| error(format!("Cannot start {stage}: {e}")))?;
    let start = Instant::now();
    loop {
        if let Err(e) = cancel.check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        match child.try_wait() {
            Ok(Some(s)) if s.success() => return Ok(()),
            Ok(Some(_)) => {
                return Err(error(format!(
                    "{stage} failed; check local components and image"
                )));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error(format!("{stage} failed: {e}")));
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error(format!("{stage} timed out")));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}
fn bounded_file(path: &Path, max: u64) -> Result<()> {
    let m = fs::metadata(path)?;
    if m.len() == 0 || m.len() > max {
        return Err(error(
            "Translation stage output exceeds size limit or is empty",
        ));
    }
    Ok(())
}

impl Session {
    /// Prepares one local image translation. Source bytes are immutable.
    pub(crate) fn translate(
        &mut self,
        source: &[u8],
        staging_root: &Path,
        components: Components,
        language: ImageTranslationSource,
        target: &str,
        numbered_only: bool,
        cancel: &Cancellation,
    ) -> Result<StagedTranslation> {
        cancel.check()?;
        if source.is_empty() || source.len() > MAX_BYTES {
            return Err(error("Image exceeds input size limit"));
        }
        if target != "en" {
            return Err(error("Local image translation outputs English"));
        }
        let font = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .map(|path| path.join("Fonts").join("arial.ttf"))
            .ok_or_else(|| error("Windows font directory is unavailable"))?;
        if !font.is_file() {
            return Err(error("Translation font is missing"));
        }
        let reader = image::ImageReader::new(Cursor::new(source))
            .with_guessed_format()
            .map_err(|e| error(e.to_string()))?;
        if reader.format() == Some(image::ImageFormat::Gif) {
            return Err(error("Use a static PNG or JPEG instead of GIF"));
        }
        let (width, height) = reader
            .into_dimensions()
            .map_err(|e| error(format!("Invalid image: {e}")))?;
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || u64::from(width) * u64::from(height) > MAX_PIXELS
        {
            return Err(error("Image dimensions exceed translation limits"));
        }
        let decoded =
            image::load_from_memory(source).map_err(|e| error(format!("Invalid image: {e}")))?;
        let mut png = Cursor::new(Vec::new());
        decoded
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|e| error(e.to_string()))?;
        if png.get_ref().len() as u64 > MAX_PNG {
            return Err(error("Prepared PNG exceeds translation limit"));
        }
        let dir = create_stage(staging_root)?;
        let mut staged = StagedTranslation {
            path: dir.join("result.png"),
            dir,
            width,
            height,
            report: RenderReport::default(),
        };
        let input = staged.dir.join("input.png");
        let ocr = staged.dir.join("ocr.json");
        let grouped = staged.dir.join("grouped.json");
        let translation = staged.dir.join("translation.json");
        let report_path = staged.dir.join("render-report.json");
        write_new(&input, png.get_ref())?;
        if self
            .runtime
            .as_ref()
            .is_none_or(|runtime| runtime.components != components)
        {
            drop(self.runtime.take());
            self.runtime = Some(Runtime::new(staging_root, components)?);
        }
        let runtime = self
            .runtime
            .as_mut()
            .expect("translation runtime initialized");
        let mut cmd = Command::new(&runtime.components.paddle_python);
        cmd.arg(runtime.dir.join("ocr_page.py"))
            .arg("--multilingual-worker")
            .env("KATRAN_PADDLE_MODELS", &runtime.components.paddle_models)
            .env("KATRAN_PROCESS_CACHE", &runtime.dir)
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONUTF8", "1")
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("TEMP", &runtime.dir)
            .env("TMP", &runtime.dir);
        recognize(
            &mut runtime.paddle,
            cmd,
            &staged.dir,
            &serde_json::json!({"dir": staged.dir, "language": language.code()}),
            "paddle.done",
            "Text detection or recognition",
            cancel,
        )?;
        bounded_file(&ocr, 1_000_000)?;
        let japanese = language == ImageTranslationSource::Japanese;
        if japanese {
            let model = runtime
                .components
                .manga_model
                .as_ref()
                .ok_or_else(|| error("Manga OCR model is missing"))?;
            let mut cmd = Command::new(&runtime.components.manga_python);
            cmd.arg(runtime.dir.join("manga_recognize.py"))
                .arg("--worker")
                .env("KATRAN_MANGA_MODEL", model)
                .env("HF_HOME", &runtime.dir)
                .env("HF_HUB_DISABLE_PROGRESS_BARS", "1")
                .env("PYTHONNOUSERSITE", "1")
                .env("PYTHONUTF8", "1")
                .env("TEMP", &runtime.dir)
                .env("TMP", &runtime.dir);
            recognize(
                &mut runtime.manga,
                cmd,
                &staged.dir,
                &staged.dir,
                "manga.done",
                "Japanese text recognition",
                cancel,
            )?;
            bounded_file(&grouped, 1_000_000)?;
        }
        let port = runtime.translation_port(cancel)?;
        let mut cmd = Command::new(&runtime.components.manga_python);
        cmd.arg(runtime.dir.join("translate_page.py"))
            .arg(if japanese { &grouped } else { &ocr })
            .arg(&translation)
            .args([
                "--source",
                language.code(),
                "--target",
                target,
                "--model",
                MODEL,
                "--port",
                &port.to_string(),
            ])
            .env("PYTHONNOUSERSITE", "1");
        if japanese {
            cmd.arg("--ordered");
        }
        run(cmd, cancel, "Local translation", Duration::from_secs(240))?;
        bounded_file(&translation, 1_000_000)?;
        let mut cmd = Command::new(&runtime.components.manga_python);
        cmd.arg(runtime.dir.join("render_page.py"))
            .arg(&input)
            .arg(&translation)
            .arg(&staged.path)
            .arg(&font)
            .arg("--report")
            .arg(&report_path)
            .env("PYTHONNOUSERSITE", "1");
        if numbered_only {
            cmd.arg("--numbered-only");
        }
        run(
            cmd,
            cancel,
            "Translated image rendering",
            Duration::from_secs(120),
        )?;
        bounded_file(&report_path, 1024)?;
        let report: RenderReport =
            serde_json::from_slice(&fs::read(report_path)?).map_err(error)?;
        if report.rendered == 0
            || report.fallback > report.rendered
            || report.side > report.fallback
            || report.rendered.saturating_add(report.skipped) > 128
        {
            return Err(error("Invalid translation rendering report"));
        }
        staged.report = report;
        bounded_file(&staged.path, MAX_PNG)?;
        cancel.check()?;
        let bytes = fs::read(&staged.path)?;
        let reader = image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Png);
        let (output_width, output_height) = reader
            .into_dimensions()
            .map_err(|e| error(format!("Invalid translated PNG: {e}")))?;
        if output_width < width
            || output_height < height
            || (output_width > width) != (report.side > 0)
            || report.left_pad > output_width.saturating_sub(width)
            || (output_height > height) != (report.fallback > report.side)
            || output_width > 8192
            || output_height > 8192
            || u64::from(output_width) * u64::from(output_height) > MAX_PIXELS
        {
            return Err(error("Translated PNG has invalid dimensions"));
        }
        let reader = image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Png);
        let output = reader
            .decode()
            .map_err(|e| error(format!("Invalid translated PNG: {e}")))?;
        if output.width() != output_width || output.height() != output_height {
            return Err(error("Translated PNG dimensions changed"));
        }
        staged.width = output_width;
        staged.height = output_height;
        Ok(staged)
    }
}

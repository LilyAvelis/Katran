//! Killable, bounded native Shell boundary. No domain service runs in the helper.
use crate::file_visual::{NativeShell, Provider, ShellVisual};
use base64::Engine;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const FLAG: &str = "--katran-shell-visual-worker";
const MAX_REQUEST: usize = 128 * 1024;
const MAX_RESPONSE: usize = 1_400_000;
const DEADLINE: Duration = Duration::from_millis(1200);
#[derive(Serialize, Deserialize)]
struct Request {
    path: PathBuf,
    edge: u32,
    kind: ShellVisual,
}
#[derive(Serialize, Deserialize)]
struct Response {
    width: u32,
    height: u32,
    rgba: String,
    error: Option<String>,
}

/// Must run before GUI, logging or single-instance initialization.
pub fn run_if_requested() -> Option<i32> {
    if !std::env::args_os().skip(1).any(|arg| arg == FLAG) {
        return None;
    }
    Some(
        if serve(
            &mut std::io::stdin().lock(),
            &mut std::io::stdout().lock(),
            &NativeShell,
        )
        .is_ok()
        {
            0
        } else {
            2
        },
    )
}
fn read_frame(reader: &mut impl Read, maximum: usize) -> Result<Vec<u8>, String> {
    let mut length = [0; 4];
    reader
        .read_exact(&mut length)
        .map_err(|_| "worker_frame_header")?;
    let length = u32::from_le_bytes(length) as usize;
    if length > maximum {
        return Err("worker_frame_limit".into());
    }
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| "worker_frame_body")?;
    Ok(bytes)
}
fn frame(bytes: &[u8]) -> Vec<u8> {
    let mut output = (bytes.len() as u32).to_le_bytes().to_vec();
    output.extend_from_slice(bytes);
    output
}
fn serve(
    reader: &mut impl Read,
    writer: &mut impl Write,
    provider: &dyn Provider,
) -> Result<(), String> {
    let request: Request =
        serde_json::from_slice(&read_frame(reader, MAX_REQUEST)?).map_err(|_| "worker_request")?;
    if !(32..=512).contains(&request.edge) {
        return Err("invalid_visual_edge".into());
    }
    let response = match provider.extract(&request.path, request.edge, request.kind) {
        Ok(image)
            if image.width() > 0
                && image.height() > 0
                && image.width() <= request.edge
                && image.height() <= request.edge =>
        {
            Response {
                width: image.width(),
                height: image.height(),
                rgba: base64::engine::general_purpose::STANDARD.encode(image.into_raw()),
                error: None,
            }
        }
        Ok(_) => Response {
            width: 0,
            height: 0,
            rgba: String::new(),
            error: Some("invalid_dimensions".into()),
        },
        Err(error) => Response {
            width: 0,
            height: 0,
            rgba: String::new(),
            error: Some(error.chars().take(256).collect()),
        },
    };
    let bytes = serde_json::to_vec(&response).map_err(|_| "worker_response")?;
    if bytes.len() > MAX_RESPONSE {
        return Err("worker_response_limit".into());
    }
    writer
        .write_all(&frame(&bytes))
        .map_err(|_| "worker_write".into())
}

pub struct IsolatedShell;
impl Provider for IsolatedShell {
    fn extract(&self, path: &Path, edge: u32, kind: ShellVisual) -> Result<RgbaImage, String> {
        let current = std::env::current_exe().map_err(|_| "worker_executable")?;
        let helper = if current.file_stem().is_some_and(|name| name == "katran-ui") {
            current
        } else {
            let directory = current.parent().ok_or("worker_executable")?;
            let directory = if directory.file_name().is_some_and(|name| name == "deps") {
                directory.parent().ok_or("worker_executable")?
            } else {
                directory
            };
            let sibling = directory.join(if cfg!(windows) {
                "katran-ui.exe"
            } else {
                "katran-ui"
            });
            verify_sibling(&sibling)?;
            sibling
        };
        extract_with_helper(&helper, path, edge, kind, DEADLINE)
    }
}
fn verify_sibling(helper: &Path) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    // Standard installed versions are immutable. Cache the successful attestation
    // so each thumbnail does not reread the executable.
    static VERIFIED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    if VERIFIED.get().is_some_and(|path| path == helper) {
        return Ok(());
    }
    let manifest = helper
        .parent()
        .ok_or("shell_worker_manifest")?
        .join("build-manifest.json");
    let mut bytes = Vec::new();
    std::fs::File::open(&manifest)
        .map_err(|_| "shell_worker_manifest")?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "shell_worker_manifest")?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("shell_worker_manifest".into());
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "shell_worker_manifest")?;
    if value["shell_visual_protocol"] != 1 {
        return Err("shell_worker_protocol_unavailable".into());
    }
    let expected = value["executable_sha256"]
        .as_str()
        .ok_or("shell_worker_manifest")?;
    let mut file = std::fs::File::open(helper).map_err(|_| "shell_worker_unavailable")?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "shell_worker_integrity")?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if format!("{:x}", digest.finalize()) != expected {
        return Err("shell_worker_integrity".into());
    }
    let _ = VERIFIED.set(helper.into());
    Ok(())
}
/// Explicit, caller-verified helper path for headless clients; never falls back
/// to in-process COM. Automatic sibling discovery additionally verifies its
/// protocol declaration and executable digest before launching anything.
pub fn extract_with_helper(
    helper: &Path,
    path: &Path,
    edge: u32,
    kind: ShellVisual,
    deadline: Duration,
) -> Result<RgbaImage, String> {
    if !(32..=512).contains(&edge) {
        return Err("invalid_visual_edge".into());
    }
    let request = serde_json::to_vec(&Request {
        path: path.into(),
        edge,
        kind,
    })
    .map_err(|_| "worker_request")?;
    if request.len() > MAX_REQUEST {
        return Err("worker_request_limit".into());
    }
    let bytes = exchange(helper, &frame(&request), deadline)?;
    let response: Response =
        serde_json::from_slice(&read_frame(&mut bytes.as_slice(), MAX_RESPONSE)?)
            .map_err(|_| "worker_response")?;
    if let Some(error) = response.error {
        return Err(error);
    }
    if response.width == 0
        || response.height == 0
        || response.width > edge
        || response.height > edge
    {
        return Err("worker_dimensions".into());
    }
    let rgba = base64::engine::general_purpose::STANDARD
        .decode(response.rgba)
        .map_err(|_| "worker_pixels")?;
    RgbaImage::from_raw(response.width, response.height, rgba).ok_or_else(|| "worker_pixels".into())
}
#[cfg(not(windows))]
fn exchange(_: &Path, _: &[u8], _: Duration) -> Result<Vec<u8>, String> {
    Err("windows_shell_unavailable".into())
}
#[cfg(windows)]
fn exchange(helper: &Path, request: &[u8], deadline: Duration) -> Result<Vec<u8>, String> {
    let mut command = std::process::Command::new(helper);
    command.arg(FLAG);
    exchange_command(command, request, deadline)
}
#[cfg(windows)]
fn exchange_command(
    mut command: std::process::Command,
    request: &[u8],
    deadline: Duration,
) -> Result<Vec<u8>, String> {
    use std::{
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::Stdio,
        time::Instant,
    };
    // Closing Katran also closes this job, killing a hung COM helper even if
    // the normal timeout/reap path cannot run (for example, parent crash).
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        process_time: i64,
        job_time: i64,
        flags: u32,
        minimum: usize,
        maximum: usize,
        active: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    unsafe extern "system" {
        fn CreateJobObjectW(
            attributes: *const std::ffi::c_void,
            name: *const u16,
        ) -> *mut std::ffi::c_void;
        fn SetInformationJobObject(
            job: *mut std::ffi::c_void,
            class: i32,
            information: *const std::ffi::c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(
            job: *mut std::ffi::c_void,
            process: *mut std::ffi::c_void,
        ) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn PeekNamedPipe(
            pipe: *mut std::ffi::c_void,
            buffer: *mut std::ffi::c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
    }
    struct Job(*mut std::ffi::c_void);
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let job = Job(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) });
    if job.0.is_null() {
        return Err("shell_worker_job_unavailable".into());
    }
    let mut limits = ExtendedLimits::default();
    limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    if unsafe {
        SetInformationJobObject(
            job.0,
            9,
            (&limits as *const ExtendedLimits).cast(),
            std::mem::size_of::<ExtendedLimits>() as u32,
        )
    } == 0
    {
        return Err("shell_worker_job_unavailable".into());
    }
    let mut child = command
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "shell_worker_unavailable")?;
    let started = Instant::now();
    let mut writer = None;
    let result = (|| {
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } == 0 {
            return Err("shell_worker_job_assignment".into());
        }
        let mut stdin = child.stdin.take().ok_or("worker_stdin")?;
        let request = request.to_vec();
        writer = Some(std::thread::spawn(move || stdin.write_all(&request)));
        let mut stdout = child.stdout.take().ok_or("worker_stdout")?;
        let mut bytes = Vec::new();
        loop {
            if started.elapsed() >= deadline {
                return Err("shell_worker_timeout".into());
            }
            let mut available = 0;
            let _connected = unsafe {
                PeekNamedPipe(
                    stdout.as_raw_handle(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            } != 0;
            if available > 0 {
                if bytes.len() + available as usize > MAX_RESPONSE + 4 {
                    return Err("worker_response_limit".into());
                }
                let previous = bytes.len();
                bytes.resize(previous + available as usize, 0);
                stdout
                    .read_exact(&mut bytes[previous..])
                    .map_err(|_| "worker_stdout")?;
            }
            if let Some(status) = child.try_wait().map_err(|_| "worker_wait")? {
                if !status.success() {
                    return Err("shell_worker_failed".into());
                }
                if available == 0 {
                    return Ok(bytes);
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    // Always reap, including malformed output and timeout; no detached reader threads.
    let _ = child.kill();
    // Kill inherited pipe readers too, then join the single bounded writer.
    drop(job);
    let _ = child.wait();
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_and_tampered_siblings_are_rejected_without_launch() {
        let directory = tempfile::tempdir().unwrap();
        let helper = directory.path().join("katran-ui.exe");
        std::fs::write(&helper, b"synthetic old executable, never launched").unwrap();
        let manifest = directory.path().join("build-manifest.json");
        std::fs::write(&manifest, br#"{"executable_sha256":"irrelevant"}"#).unwrap();
        assert_eq!(
            verify_sibling(&helper).unwrap_err(),
            "shell_worker_protocol_unavailable"
        );
        std::fs::write(
            &manifest,
            br#"{"shell_visual_protocol":1,"executable_sha256":"wrong"}"#,
        )
        .unwrap();
        assert_eq!(
            verify_sibling(&helper).unwrap_err(),
            "shell_worker_integrity"
        );
    }
    #[cfg(windows)]
    #[test]
    fn hung_and_crashed_helpers_are_reaped_and_next_call_recovers() {
        fn powershell(script: &str) -> std::process::Command {
            let mut command = std::process::Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
            command
        }
        let started = std::time::Instant::now();
        for _ in 0..3 {
            assert_eq!(
                exchange_command(
                    powershell("Start-Sleep -Seconds 30"),
                    &vec![0; MAX_REQUEST],
                    Duration::from_millis(100)
                )
                .unwrap_err(),
                "shell_worker_timeout"
            );
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            exchange_command(powershell("exit 9"), &[], Duration::from_secs(5)).unwrap_err(),
            "shell_worker_failed"
        );
        assert!(
            exchange_command(powershell("exit 0"), &[], Duration::from_secs(5))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn rejects_oversized_frame_before_allocation() {
        assert!(read_frame(&mut u32::MAX.to_le_bytes().as_slice(), MAX_REQUEST).is_err());
    }
    #[test]
    fn provider_protocol_is_bounded_and_roundtrips() {
        struct Fake;
        impl Provider for Fake {
            fn extract(&self, _: &Path, _: u32, _: ShellVisual) -> Result<RgbaImage, String> {
                Ok(RgbaImage::new(32, 32))
            }
        }
        let request = serde_json::to_vec(&Request {
            path: "synthetic".into(),
            edge: 32,
            kind: ShellVisual::Icon,
        })
        .unwrap();
        let mut output = vec![];
        serve(&mut frame(&request).as_slice(), &mut output, &Fake).unwrap();
        let response: Response =
            serde_json::from_slice(&read_frame(&mut output.as_slice(), MAX_RESPONSE).unwrap())
                .unwrap();
        assert_eq!(response.width, 32);
        assert!(response.error.is_none());
    }
}

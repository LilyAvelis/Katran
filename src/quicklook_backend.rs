//! QuickLook lifecycle and launch policy. GUI clients use dispatcher-owned jobs.
use crate::{ApiError, Result, quicklook_transport as transport};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
static GATE: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(1);
const IO_TIMEOUT: Duration = Duration::from_millis(1200);
const START_TIMEOUT: Duration = Duration::from_secs(8);
fn api_error(e: transport::TransportError) -> ApiError {
    ApiError::new(e.code, e.message)
}
fn request_id() -> String {
    format!(
        "katran-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
fn fork_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("KATRAN_QUICKLOOK_EXE") {
        return Ok(PathBuf::from(path));
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("Programs/QuickLook/QuickLook.exe"))
        .ok_or_else(|| ApiError::new("quicklook_not_found", "LOCALAPPDATA is unavailable"))
}
fn canonical_fork() -> Result<PathBuf> {
    let p = fork_path()?;
    if !p.is_absolute() || !p.is_file() {
        return Err(ApiError::new(
            "quicklook_not_found",
            "Maintained QuickLook fork is not installed; expected LOCALAPPDATA/Programs/QuickLook/QuickLook.exe or KATRAN_QUICKLOOK_EXE",
        ));
    }
    std::fs::canonicalize(p).map_err(Into::into)
}
fn valid_status(value: &Value) -> bool {
    value["protocol"] == 1
        && value["ok"] == true
        && value["state"] == "ready"
        && value["instance"].as_str().is_some_and(|s| !s.is_empty())
}
fn verify_identity(value: &Value, expected: &Path) -> Result<()> {
    let pid = value["pid"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| ApiError::new("quicklook_identity_mismatch", "Missing server PID"))?;
    let actual = std::fs::canonicalize(transport::process_image(pid).map_err(api_error)?)?;
    if !actual
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        return Err(ApiError::new(
            "quicklook_identity_mismatch",
            "Bridge server executable differs from configured maintained QuickLook",
        ));
    }
    Ok(())
}
fn hello(endpoint: &str) -> std::result::Result<Value, transport::TransportError> {
    transport::exchange(
        endpoint,
        &json!({"protocol":1,"command":"status"}),
        IO_TIMEOUT,
    )
}
// The shared QuickLook mutex does not identify the process holding it. When
// a maintained package is configured, an absent bridge may simply mean that
// its process has not created the endpoint yet; sending a CLI toggle then is
// unsafe. An adjacent manifest is enough to make that state ambiguous, even if
// the package subsequently fails verification.
fn maintained_package_configured() -> bool {
    fork_path().ok().is_some_and(|p| {
        p.parent()
            .is_some_and(|dir| dir.join("katran-bridge-manifest.json").is_file())
    })
}
/// Read-only; reports stock compatibility without probing the legacy pipe.
pub fn status() -> Result<Value> {
    let endpoint = transport::endpoint().map_err(api_error)?;
    match hello(&endpoint) {
        Ok(value) => {
            let path = canonical_fork()?;
            verify_identity(&value, &path)?;
            if value["protocol"] == 1
                && value["ok"] == true
                && value["state"] == "initializing"
                && value["instance"].as_str().is_some_and(|s| !s.is_empty())
            {
                return Ok(
                    json!({"backend":"quicklook","transport":"fast_bridge_v1","ready":false,"state":"initializing","pid":value["pid"],"instance":value["instance"],"version":value["version"]}),
                );
            }
            if !valid_status(&value) {
                return Err(ApiError::new(
                    "quicklook_incompatible",
                    "Bridge is not ready or protocol is incompatible",
                ));
            }
            Ok(
                json!({"backend":"quicklook","transport":"fast_bridge_v1","ready":true,"pid":value["pid"],"instance":value["instance"],"version":value["version"]}),
            )
        }
        Err(e) => {
            if e.may_have_written || e.code != "quicklook_bridge_absent" {
                return Err(api_error(e));
            }
            if transport::instance_exists().map_err(api_error)? {
                if maintained_package_configured() {
                    return Ok(
                        json!({"backend":"quicklook","ready":false,"state":"bridge_pending"}),
                    );
                }
                Ok(
                    json!({"backend":"quicklook","transport":"desktop_cli","ready":true,"compatibility":"running_stock_or_bridge_unavailable"}),
                )
            } else {
                Ok(json!({"backend":"quicklook","ready":false,"state":"absent"}))
            }
        }
    }
}
/// Co-start only a verified configured fork when no standard instance exists.
/// This never terminates a process and never opens the old QuickLook pipe.
pub fn warmup() -> Result<Value> {
    let _guard = GATE
        .lock()
        .map_err(|_| ApiError::new("quicklook_busy", "Backend gate poisoned"))?;
    ensure()
}
trait StartupIo {
    fn current(&mut self) -> Result<Value>;
    /// False means another normal instance appeared; no child was spawned.
    fn start(&mut self) -> Result<bool>;
    fn poll_ready(&mut self) -> Result<Option<Value>>;
    fn exited(&mut self) -> Result<Option<String>>;
    fn expired(&self) -> bool;
    fn pause(&mut self);
}
fn is_initializing(value: &Value) -> bool {
    (value["transport"] == "fast_bridge_v1"
        && value["state"] == "initializing"
        && value["ready"] == false)
        || (value["state"] == "bridge_pending" && value["ready"] == false)
}
fn ensure_with(io: &mut impl StartupIo) -> Result<Value> {
    let current = io.current()?;
    if current["ready"] == true {
        return Ok(current);
    }
    if !io.start()? {
        let current = io.current()?;
        if current["ready"] == true {
            return Ok(current);
        }
        if !is_initializing(&current) {
            return Err(ApiError::new(
                "quicklook_backend_unavailable",
                "Existing QuickLook instance has no confirmed ready or initializing maintained backend",
            ));
        }
        // A maintained server already owns the mutex and is loading plugins.
        // Join its bounded readiness wait; never report a ready:false success.
    }
    loop {
        if let Some(ready) = io.poll_ready()? {
            return Ok(ready);
        }
        if let Some(code) = io.exited()? {
            let state = io.current()?;
            if state["transport"] == "fast_bridge_v1" && state["ready"] == true {
                return Ok(state);
            }
            if !is_initializing(&state) {
                return Err(ApiError::new(
                    "quicklook_launch_failed",
                    format!("Maintained QuickLook exited before readiness: {code}"),
                ));
            }
        }
        if io.expired() {
            return Err(ApiError::new(
                "quicklook_start_timeout",
                "QuickLook did not acknowledge readiness; no process was terminated",
            ));
        }
        io.pause();
    }
}
#[derive(Default)]
struct NativeStartup {
    child: Option<crate::quicklook_process::BackgroundProcess>,
    exe: Option<PathBuf>,
    deadline: Option<Instant>,
}
impl StartupIo for NativeStartup {
    fn current(&mut self) -> Result<Value> {
        status()
    }
    fn start(&mut self) -> Result<bool> {
        let exe = canonical_fork()?;
        verify_package(&exe)?;
        self.exe = Some(exe.clone());
        self.deadline = Some(Instant::now() + START_TIMEOUT);
        // The in-process gate prevents duplicate Katran launches; QuickLook's
        // own mutex arbitrates starts by other processes.
        if transport::instance_exists().map_err(api_error)? {
            return Ok(false);
        }
        self.child = Some(
            crate::quicklook_process::spawn(&exe, &[])
                .map_err(|e| ApiError::new("quicklook_launch_failed", e))?,
        );
        Ok(true)
    }
    fn poll_ready(&mut self) -> Result<Option<Value>> {
        let remaining = self
            .deadline
            .unwrap()
            .saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        if let Ok(value) = transport::exchange(
            &transport::endpoint().map_err(api_error)?,
            &json!({"protocol":1,"command":"status"}),
            remaining.min(IO_TIMEOUT),
        ) {
            verify_identity(&value, self.exe.as_ref().unwrap())?;
            if valid_status(&value) {
                return Ok(Some(
                    json!({"backend":"quicklook","transport":"fast_bridge_v1","ready":true,"started":self.child.is_some(),"pid":value["pid"],"instance":value["instance"],"version":value["version"]}),
                ));
            }
        }
        Ok(None)
    }
    fn exited(&mut self) -> Result<Option<String>> {
        self.child
            .as_mut()
            .map(|child| {
                child
                    .try_wait()
                    .map(|s| s.map(|code| code.to_string()))
                    .map_err(|e| ApiError::new("quicklook_launch_failed", e))
            })
            .transpose()
            .map(Option::flatten)
    }
    fn expired(&self) -> bool {
        Instant::now() >= self.deadline.unwrap()
    }
    fn pause(&mut self) {
        std::thread::sleep(Duration::from_millis(80));
    }
}
fn ensure() -> Result<Value> {
    ensure_with(&mut NativeStartup::default())
}
fn verify_package(exe: &Path) -> Result<()> {
    use std::io::Read;
    let manifest = exe.parent().unwrap().join("katran-bridge-manifest.json");
    let mut bytes = Vec::new();
    std::fs::File::open(manifest)
        .map_err(|e| ApiError::new("quicklook_unverified_package", e))?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(ApiError::new(
            "quicklook_unverified_package",
            "Package manifest exceeds limit",
        ));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| ApiError::new("quicklook_unverified_package", e))?;
    let hash = crate::preview::content_hash(exe)?;
    if value["protocol"] != 1
        || value["executable_sha256"].as_str() != Some(hash.as_str())
        || value["flavor"] != "normal"
    {
        return Err(ApiError::new(
            "quicklook_unverified_package",
            "Maintained QuickLook package manifest mismatch",
        ));
    }
    Ok(())
}
/// Acknowledged initiation. No CLI replay after a possibly submitted Toggle.
pub fn toggle(path: &Path) -> Result<Value> {
    let _guard = GATE
        .lock()
        .map_err(|_| ApiError::new("quicklook_busy", "Backend gate poisoned"))?;
    let text = crate::quicklook::protocol_path(&std::fs::canonicalize(path)?)?;
    let state = ensure()?;
    if state["transport"] == "desktop_cli" {
        return stock_cli(&text);
    }
    let endpoint = transport::endpoint().map_err(api_error)?;
    let id = request_id();
    let request = json!({"protocol":1,"command":"preview","request_id":id,"path":text});
    let response = submit(&request, &state, |message| {
        transport::exchange(&endpoint, message, IO_TIMEOUT)
    })?;
    if response["instance"] != state["instance"] {
        return Err(ApiError::new(
            "quicklook_outcome_unknown",
            "Bridge instance changed before acknowledgement",
        ));
    }
    if response["request_id"].as_str() != Some(id.as_str()) || response["protocol"] != 1 {
        return Err(ApiError::new(
            "quicklook_invalid_response",
            "Bridge acknowledgement ID/version mismatch",
        ));
    }
    match response["state"].as_str() {
        Some("accepted" | "dispatched") if response["ok"] == true => Ok(
            json!({"requested":true,"transport":"fast_bridge_v1","acknowledgement":response["state"],"request_id":id,"backend_pid":response["pid"]}),
        ),
        Some("unknown") => Err(ApiError::new(
            "quicklook_outcome_unknown",
            "Request result unavailable; no automatic replay was attempted",
        )),
        _ => Err(ApiError::new(
            "quicklook_request_rejected",
            response["error"]
                .as_str()
                .unwrap_or("Bridge rejected preview"),
        )),
    }
}
fn submit(
    request: &Value,
    state: &Value,
    mut exchange: impl FnMut(&Value) -> std::result::Result<Value, transport::TransportError>,
) -> Result<Value> {
    match exchange(request) {
        Ok(response) => Ok(response),
        Err(e) if e.may_have_written => {
            let recovered=exchange(&json!({"protocol":1,"command":"request_status","request_id":request["request_id"]})).map_err(|_|ApiError::new("quicklook_outcome_unknown","Preview may have been accepted; no automatic replay was attempted"))?;
            if recovered["instance"] != state["instance"] {
                return Err(ApiError::new(
                    "quicklook_outcome_unknown",
                    "Server instance changed after uncertain request",
                ));
            }
            Ok(recovered)
        }
        Err(e) => Err(api_error(e)),
    }
}
/// Explicit compatibility policy: an existing stock instance uses its own CLI.
fn stock_cli(path: &str) -> Result<Value> {
    let mut candidates = Vec::new();
    for (variable, suffix) in [
        ("LOCALAPPDATA", "Programs/QuickLook/QuickLook.exe"),
        ("ProgramFiles", "QuickLook/QuickLook.exe"),
        ("ProgramFiles(x86)", "QuickLook/QuickLook.exe"),
        ("USERPROFILE", "scoop/apps/quicklook/current/QuickLook.exe"),
    ] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(root).join(suffix));
        }
    }
    let executable = candidates
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            ApiError::new(
                "quicklook_not_found",
                "Running stock instance has no supported desktop CLI installation",
            )
        })?;
    crate::quicklook_process::spawn(&executable, &[std::ffi::OsStr::new(path)])
        .map_err(|e| ApiError::new("quicklook_launch_failed", e))?;
    Ok(
        json!({"requested":true,"transport":"desktop_cli","acknowledgement":"process_created","compatibility":"running_stock_or_bridge_unavailable"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    fn package(flavor: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("QuickLook.exe");
        std::fs::write(&exe, b"synthetic executable fixture; never executed").unwrap();
        let hash = format!("{:x}", Sha256::digest(std::fs::read(&exe).unwrap()));
        std::fs::write(
            dir.path().join("katran-bridge-manifest.json"),
            serde_json::to_vec(&json!({"protocol":1,"flavor":flavor,"executable_sha256":hash}))
                .unwrap(),
        )
        .unwrap();
        (dir, exe)
    }
    #[test]
    fn verified_package_cannot_be_replaced_or_be_the_lab_binary() {
        let (_dir, exe) = package("normal");
        assert!(verify_package(&exe).is_ok());
        std::fs::write(&exe, b"changed synthetic bytes").unwrap();
        assert_eq!(
            verify_package(&exe).unwrap_err().code,
            "quicklook_unverified_package"
        );
        let (_dir, exe) = package("lab");
        assert_eq!(
            verify_package(&exe).unwrap_err().code,
            "quicklook_unverified_package"
        );
    }
    #[test]
    fn missing_manifest_never_authorizes_start() {
        let (dir, exe) = package("normal");
        std::fs::remove_file(dir.path().join("katran-bridge-manifest.json")).unwrap();
        assert_eq!(
            verify_package(&exe).unwrap_err().code,
            "quicklook_unverified_package"
        );
    }
    #[test]
    fn readiness_requires_version_and_instance_identity() {
        assert!(!valid_status(
            &json!({"protocol":1,"ok":true,"state":"ready"})
        ));
        assert!(!valid_status(
            &json!({"protocol":2,"ok":true,"state":"ready","instance":"test"})
        ));
        assert!(!valid_status(
            &json!({"protocol":1,"ok":true,"state":"initializing","instance":"test"})
        ));
        assert!(valid_status(
            &json!({"protocol":1,"ok":true,"state":"ready","instance":"test"})
        ));
    }
    #[cfg(windows)]
    #[test]
    fn reported_pid_must_belong_to_configured_executable() {
        let current = std::env::current_exe().unwrap();
        let value = json!({"pid":std::process::id()});
        assert!(verify_identity(&value, &std::fs::canonicalize(current).unwrap()).is_ok());
        let (_dir, other) = package("normal");
        assert_eq!(
            verify_identity(&value, &other).unwrap_err().code,
            "quicklook_identity_mismatch"
        );
    }
    struct FakeStartup {
        states: std::collections::VecDeque<Value>,
        allow_start: bool,
        starts: usize,
        polls: usize,
        ready_after: Option<usize>,
        exit: Option<String>,
        ticks: usize,
        limit: usize,
    }
    impl FakeStartup {
        fn absent() -> Self {
            Self {
                states: std::collections::VecDeque::from([json!({"ready":false})]),
                allow_start: true,
                starts: 0,
                polls: 0,
                ready_after: Some(3),
                exit: None,
                ticks: 0,
                limit: 5,
            }
        }
    }
    impl StartupIo for FakeStartup {
        fn current(&mut self) -> Result<Value> {
            Ok(self.states.pop_front().unwrap_or(json!({"ready":false})))
        }
        fn start(&mut self) -> Result<bool> {
            if self.allow_start {
                self.starts += 1;
            }
            Ok(self.allow_start)
        }
        fn poll_ready(&mut self) -> Result<Option<Value>> {
            self.polls += 1;
            Ok(self
                .ready_after
                .filter(|n| self.polls >= *n)
                .map(|_| json!({"ready":true,"transport":"fast_bridge_v1"})))
        }
        fn exited(&mut self) -> Result<Option<String>> {
            Ok(self.exit.clone())
        }
        fn expired(&self) -> bool {
            self.ticks >= self.limit
        }
        fn pause(&mut self) {
            self.ticks += 1;
        }
    }
    #[test]
    fn existing_stock_is_not_restarted_and_concurrent_instance_wins() {
        let mut io = FakeStartup::absent();
        io.states =
            std::collections::VecDeque::from([json!({"ready":true,"transport":"desktop_cli"})]);
        assert_eq!(ensure_with(&mut io).unwrap()["transport"], "desktop_cli");
        assert_eq!(io.starts, 0);
        let mut io = FakeStartup::absent();
        io.allow_start = false;
        io.states
            .push_back(json!({"ready":true,"transport":"desktop_cli"}));
        assert_eq!(ensure_with(&mut io).unwrap()["transport"], "desktop_cli");
        assert_eq!(io.starts, 0);
    }
    #[test]
    fn cold_start_waits_for_readiness_and_never_respawns_on_timeout() {
        let mut io = FakeStartup::absent();
        assert_eq!(ensure_with(&mut io).unwrap()["ready"], true);
        assert_eq!(io.starts, 1);
        assert_eq!(io.polls, 3);
        let mut io = FakeStartup::absent();
        io.ready_after = None;
        io.limit = 2;
        assert_eq!(
            ensure_with(&mut io).unwrap_err().code,
            "quicklook_start_timeout"
        );
        assert_eq!(io.starts, 1);
    }
    #[test]
    fn child_exit_without_a_ready_bridge_is_an_explicit_failure() {
        let mut io = FakeStartup::absent();
        io.ready_after = None;
        io.exit = Some("exit code: 0".into());
        assert_eq!(
            ensure_with(&mut io).unwrap_err().code,
            "quicklook_launch_failed"
        );
        assert_eq!(io.starts, 1);
    }
    fn lost_ack(written: bool) -> transport::TransportError {
        transport::TransportError {
            code: "test_failure",
            message: "synthetic pipe failure".into(),
            may_have_written: written,
        }
    }
    #[test]
    fn lost_ack_queries_outcome_without_resubmitting_toggle() {
        let request = json!({"command":"preview","request_id":"test"});
        let state = json!({"instance":"one"});
        let mut calls = Vec::new();
        let result = submit(&request, &state, |r| {
            calls.push(r["command"].as_str().unwrap().to_string());
            if calls.len() == 1 {
                Err(lost_ack(true))
            } else {
                Ok(json!({"instance":"one","state":"accepted"}))
            }
        })
        .unwrap();
        assert_eq!(result["state"], "accepted");
        assert_eq!(calls, vec!["preview", "request_status"]);
        let mut calls = 0;
        assert_eq!(
            submit(&request, &state, |_| {
                calls += 1;
                Err(lost_ack(true))
            })
            .unwrap_err()
            .code,
            "quicklook_outcome_unknown"
        );
        assert_eq!(calls, 2);
        let mut calls = 0;
        assert_eq!(
            submit(&request, &state, |_| {
                calls += 1;
                Err(lost_ack(false))
            })
            .unwrap_err()
            .code,
            "test_failure"
        );
        assert_eq!(calls, 1);
    }
    #[test]
    fn a_restarted_server_cannot_resolve_an_uncertain_request() {
        let mut calls = 0;
        assert_eq!(
            submit(
                &json!({"request_id":"test"}),
                &json!({"instance":"old"}),
                |_| {
                    calls += 1;
                    if calls == 1 {
                        Err(lost_ack(true))
                    } else {
                        Ok(json!({"instance":"new","state":"unknown"}))
                    }
                }
            )
            .unwrap_err()
            .code,
            "quicklook_outcome_unknown"
        );
        assert_eq!(calls, 2);
    }
    #[test]
    fn existing_maintained_initialization_waits_without_respawning() {
        let initializing =
            json!({"ready":false,"transport":"fast_bridge_v1","state":"initializing"});
        let mut io = FakeStartup::absent();
        io.allow_start = false;
        io.states = std::collections::VecDeque::from([initializing.clone(), initializing.clone()]);
        assert_eq!(ensure_with(&mut io).unwrap()["ready"], true);
        assert_eq!(io.starts, 0);
        assert_eq!(io.polls, 3);
        let mut io = FakeStartup::absent();
        io.allow_start = false;
        io.ready_after = None;
        io.limit = 2;
        io.states = std::collections::VecDeque::from([initializing.clone(), initializing]);
        assert_eq!(
            ensure_with(&mut io).unwrap_err().code,
            "quicklook_start_timeout"
        );
        assert_eq!(io.starts, 0);
    }
    #[test]
    fn existing_mutex_before_bridge_waits_and_never_uses_stock_cli() {
        let pending = json!({"ready":false,"state":"bridge_pending"});
        let mut io = FakeStartup::absent();
        io.allow_start = false;
        io.states = std::collections::VecDeque::from([pending.clone(), pending.clone()]);
        assert_eq!(ensure_with(&mut io).unwrap()["transport"], "fast_bridge_v1");
        assert_eq!(io.starts, 0);
        let mut io = FakeStartup::absent();
        io.allow_start = false;
        io.ready_after = None;
        io.limit = 2;
        io.states = std::collections::VecDeque::from([pending.clone(), pending]);
        assert_eq!(
            ensure_with(&mut io).unwrap_err().code,
            "quicklook_start_timeout"
        );
        assert_eq!(io.starts, 0);
    }
    #[test]
    fn existing_unknown_state_is_not_successful_warmup() {
        let mut io = FakeStartup::absent();
        io.allow_start = false;
        assert_eq!(
            ensure_with(&mut io).unwrap_err().code,
            "quicklook_backend_unavailable"
        );
        assert_eq!(io.starts, 0);
    }
    #[test]
    fn exited_launch_client_waits_for_other_maintained_initialization() {
        let mut io = FakeStartup::absent();
        io.exit = Some("exit code: 0".into());
        let initializing =
            json!({"ready":false,"transport":"fast_bridge_v1","state":"initializing"});
        io.states.push_back(initializing.clone());
        io.states.push_back(initializing);
        assert_eq!(ensure_with(&mut io).unwrap()["ready"], true);
        assert_eq!(io.starts, 1);
        assert_eq!(io.polls, 3);
    }
}

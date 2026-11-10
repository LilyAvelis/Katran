#![cfg(windows)]
#[allow(dead_code)]
#[path = "../src/quicklook_transport.rs"]
mod quicklook_transport;
use serde_json::json;
use std::time::Duration;
#[test]
fn legacy_pipe_is_never_a_transport_target() {
    let e = quicklook_transport::exchange(
        "QuickLook.App.Pipe.forbidden",
        &json!({"protocol":1,"command":"status"}),
        Duration::from_millis(10),
    )
    .unwrap_err();
    assert_eq!(e.code, "invalid_argument");
    assert!(!e.may_have_written);
}
#[test]
fn oversize_frame_is_rejected_before_connecting() {
    let e = quicklook_transport::exchange(
        "QuickLook.KatranBridge.Lab.v1.unbound",
        &json!({"path":"x".repeat(quicklook_transport::MAX_FRAME)}),
        Duration::from_millis(10),
    )
    .unwrap_err();
    assert_eq!(e.code, "invalid_argument");
    assert!(!e.may_have_written);
}
#[test]
fn missing_endpoint_is_known_unsubmitted() {
    let e = quicklook_transport::exchange(
        &format!(
            "QuickLook.KatranBridge.Lab.v1.absent-{}",
            std::process::id()
        ),
        &json!({"protocol":1,"command":"status"}),
        Duration::from_millis(20),
    )
    .unwrap_err();
    assert!(!e.may_have_written);
}
#[test]
fn sid_and_process_identity_are_obtained_without_shell_children() {
    assert!(
        quicklook_transport::endpoint()
            .unwrap()
            .starts_with("QuickLook.KatranBridge.v1.S-1-")
    );
    assert_eq!(
        std::fs::canonicalize(quicklook_transport::process_image(std::process::id()).unwrap())
            .unwrap(),
        std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap()
    );
}
#[test]
#[ignore = "Only explicitly launched isolated bridge; never uses installed QuickLook"]
fn isolated_real_bridge_accepts_and_deduplicates() {
    let endpoint = std::env::var("KATRAN_QUICKLOOK_LAB_PIPE").unwrap();
    assert!(endpoint.starts_with("QuickLook.KatranBridge.Lab.v1."));
    let path = std::env::var("KATRAN_QUICKLOOK_LAB_FILE").unwrap();
    let status = quicklook_transport::exchange(
        &endpoint,
        &json!({"protocol":1,"command":"status"}),
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(status["state"], "ready");
    let id = format!("rust-integration-{}", std::process::id());
    let req = json!({"protocol":1,"command":"preview","request_id":id,"path":path});
    let first = quicklook_transport::exchange(&endpoint, &req, Duration::from_secs(2)).unwrap();
    assert_eq!(first["ok"], true);
    assert!(matches!(
        first["state"].as_str(),
        Some("accepted" | "dispatched")
    ));
    std::thread::sleep(Duration::from_millis(150));
    let again = quicklook_transport::exchange(&endpoint, &req, Duration::from_secs(2)).unwrap();
    assert_eq!(again["ok"], true);
    assert_eq!(again["instance"], status["instance"]);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let lookup = quicklook_transport::exchange(
            &endpoint,
            &json!({"protocol":1,"command":"request_status","request_id":id}),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(lookup["request_id"], id);
        if lookup["state"] == "dispatched" {
            break;
        }
        assert_eq!(lookup["state"], "accepted");
        assert!(
            std::time::Instant::now() < deadline,
            "UI dispatch did not finish: {lookup}"
        );
        std::thread::sleep(Duration::from_millis(30));
    }
}
#[cfg(windows)]
mod synthetic_server {
    use std::{
        ffi::c_void,
        ptr,
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };
    type H = *mut c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            mode: u32,
            pipe_mode: u32,
            max: u32,
            out: u32,
            input: u32,
            timeout: u32,
            security: *const c_void,
        ) -> H;
        fn ConnectNamedPipe(h: H, ov: *mut c_void) -> i32;
        fn ReadFile(h: H, b: *mut c_void, n: u32, done: *mut u32, ov: *mut c_void) -> i32;
        fn WriteFile(h: H, b: *const c_void, n: u32, done: *mut u32, ov: *mut c_void) -> i32;
        fn CloseHandle(h: H) -> i32;
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    pub fn start(reply: Option<Vec<u8>>, delay: Duration) -> (String, thread::JoinHandle<()>) {
        let endpoint = format!(
            "QuickLook.KatranBridge.Lab.v1.rust-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let name = format!(r"\\.\pipe\{endpoint}");
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || unsafe {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let pipe = CreateNamedPipeW(wide.as_ptr(), 3, 0, 1, 4096, 4096, 0, ptr::null());
            assert_ne!(pipe as isize, -1);
            tx.send(()).unwrap();
            ConnectNamedPipe(pipe, ptr::null_mut());
            let mut header = [0u8; 4];
            let mut done = 0;
            ReadFile(
                pipe,
                header.as_mut_ptr().cast(),
                4,
                &mut done,
                ptr::null_mut(),
            );
            let mut body = vec![0u8; u32::from_le_bytes(header) as usize];
            ReadFile(
                pipe,
                body.as_mut_ptr().cast(),
                body.len() as u32,
                &mut done,
                ptr::null_mut(),
            );
            thread::sleep(delay);
            if let Some(reply) = reply {
                // Fragment the frame to exercise repeated overlapped operations.
                for chunk in reply.chunks(3) {
                    if WriteFile(
                        pipe,
                        chunk.as_ptr().cast(),
                        chunk.len() as u32,
                        &mut done,
                        ptr::null_mut(),
                    ) == 0
                    {
                        break;
                    }
                }
                let mut eof = 0u8;
                ReadFile(
                    pipe,
                    (&mut eof as *mut u8).cast(),
                    1,
                    &mut done,
                    ptr::null_mut(),
                );
            }
            CloseHandle(pipe);
        });
        rx.recv().unwrap();
        (endpoint, worker)
    }
    pub fn frame(value: serde_json::Value) -> Vec<u8> {
        let payload = serde_json::to_vec(&value).unwrap();
        let mut bytes = (payload.len() as u32).to_le_bytes().to_vec();
        bytes.extend(payload);
        bytes
    }
}
#[test]
fn overlapped_read_timeout_cancels_without_replaying() {
    let (endpoint, worker) = synthetic_server::start(None, Duration::from_millis(100));
    let start = std::time::Instant::now();
    let e = quicklook_transport::exchange(
        &endpoint,
        &json!({"protocol":1,"command":"status"}),
        Duration::from_millis(25),
    )
    .unwrap_err();
    assert_eq!(e.code, "quicklook_timeout");
    assert!(e.may_have_written);
    assert!(start.elapsed() < Duration::from_secs(1));
    worker.join().unwrap();
}
#[test]
fn fragmented_response_and_real_server_pid_are_validated() {
    let response = json!({"protocol":1,"pid":std::process::id(),"state":"ready"});
    let (endpoint, worker) = synthetic_server::start(
        Some(synthetic_server::frame(response.clone())),
        Duration::ZERO,
    );
    assert_eq!(
        quicklook_transport::exchange(
            &endpoint,
            &json!({"protocol":1,"command":"status"}),
            Duration::from_secs(1)
        )
        .unwrap(),
        response
    );
    worker.join().unwrap();
}
#[test]
fn forged_response_pid_is_rejected_after_submission() {
    let (endpoint, worker) = synthetic_server::start(
        Some(synthetic_server::frame(json!({"pid":std::process::id()+1}))),
        Duration::ZERO,
    );
    let e = quicklook_transport::exchange(
        &endpoint,
        &json!({"protocol":1,"command":"status"}),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert_eq!(e.code, "quicklook_identity_mismatch");
    assert!(e.may_have_written);
    worker.join().unwrap();
}
#[test]
fn malformed_reply_cannot_become_success() {
    let (endpoint, worker) = synthetic_server::start(
        Some(
            (quicklook_transport::MAX_FRAME as u32 + 1)
                .to_le_bytes()
                .to_vec(),
        ),
        Duration::ZERO,
    );
    let e = quicklook_transport::exchange(
        &endpoint,
        &json!({"protocol":1,"command":"status"}),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert_eq!(e.code, "quicklook_invalid_response");
    assert!(e.may_have_written);
    worker.join().unwrap();
}

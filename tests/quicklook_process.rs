#![cfg(windows)]
use katran::quicklook_process;
use std::{
    ffi::OsStr,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
#[test]
fn relative_executables_and_nul_arguments_are_rejected() {
    assert_eq!(
        quicklook_process::spawn(Path::new("relative.exe"), &[])
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert_eq!(
        quicklook_process::spawn(
            &std::env::current_exe().unwrap(),
            &[OsStr::new("bad\0argument")]
        )
        .err()
        .unwrap()
        .kind(),
        std::io::ErrorKind::InvalidInput
    );
}
#[test]
fn background_child_does_not_keep_parent_stdio_alive() {
    // A separate tiny console helper imports the actual production spawn code.
    // Its parent inherits this test's captured pipes; its persistent child must
    // inherit no handles, including stdout handles not explicitly passed by us.
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("helper.rs");
    let exe = root.path().join("helper.exe");
    let module = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/quicklook_process.rs");
    let helper = format!(
        r#"
#[path={module:?}] mod quicklook_process;
fn main() {{
 let args:Vec<std::ffi::OsString>=std::env::args_os().skip(1).collect();
 if args[0]=="child" {{
  std::fs::write(&args[2],format!("{{:?}}",&args[4..])).unwrap();
  std::fs::write(&args[1],std::process::id().to_string()).unwrap();
  let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
  while !std::path::Path::new(&args[3]).exists() && std::time::Instant::now()<deadline {{std::thread::sleep(std::time::Duration::from_millis(5));}}
  return;
 }}
 let current=std::env::current_exe().unwrap();
 let refs:Vec<&std::ffi::OsStr>=args[1..].iter().map(|s|s.as_os_str()).collect();
 let mut child=quicklook_process::spawn(&current,&refs).unwrap();
 assert_eq!(child.try_wait().unwrap(),None);
 println!("child_pid={{}}",child.id());
 // Closing this owned handle must leave the synthetic child alive.
 drop(child);
}}
"#
    );
    std::fs::write(&source, helper).unwrap();
    let build = Command::new("rustc")
        .args([
            "--edition=2024",
            "--crate-name",
            "synthetic_background_helper",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let ready = root.path().join("ready.txt");
    let echoed = root.path().join("echoed-args.txt");
    let stop = root.path().join("stop.txt");
    let literal = [
        "",
        "Юникод space",
        "quoted \"file\".txt",
        "C:\\trailing slash\\",
        "\\\\server\\share with spaces\\",
        "& % PATH | < > $(no shell)",
    ];
    let mut parent = Command::new(&exe)
        .arg("parent")
        .arg("child")
        .arg(&ready)
        .arg(&echoed)
        .arg(&stop)
        .args(literal)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = parent.stdout.take().unwrap();
    let mut stderr = parent.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let mut err = String::new();
        let a = stdout.read_to_string(&mut out);
        let b = stderr.read_to_string(&mut err);
        tx.send((a, b, out, err)).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let eof = rx.recv_timeout(Duration::from_secs(1));
    let child_still_waiting = ready.exists() && !stop.exists();
    let echoed_result = std::fs::read_to_string(&echoed);
    // Release only our synthetic child, even if the EOF assertion fails.
    std::fs::write(&stop, b"release synthetic child").unwrap();
    let status = parent.wait().unwrap();
    reader.join().unwrap();
    assert!(status.success());
    assert!(
        child_still_waiting,
        "Synthetic background child did not reach its independent wait"
    );
    let (a, b, out, err) =
        eof.expect("API pipe EOF must arrive while the background child is alive");
    a.unwrap();
    b.unwrap();
    assert!(err.is_empty(), "{err}");
    assert!(out.starts_with("child_pid="));
    let expected: Vec<std::ffi::OsString> = literal.iter().map(std::ffi::OsString::from).collect();
    assert_eq!(echoed_result.unwrap(), format!("{expected:?}"));
    // Give the released child time to exit and relinquish its executable.
    std::thread::sleep(Duration::from_millis(100));
}

use katran::{DEFAULT_QUOTA, Request, Service};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("serve-stdio")) {
        return Err("Usage: katran serve-stdio <data-directory>".into());
    }
    let root = PathBuf::from(args.next().ok_or("Explicit data-directory required")?);
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let mut service = Service::open(&root, DEFAULT_QUOTA).map_err(|e| e.message)?;
    if let Ok(logger) = katran::diagnostics::Logger::start(&root) {
        service.attach_logger(logger);
    }
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => service.execute(request),
            Err(e) => {
                serde_json::json!({"api_version":1,"request_id":null,"ok":false,"error":{"code":"invalid_request","message":e.to_string()}})
            }
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}

//! Explicit successor startup: never terminates or manipulates the predecessor.
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

const MARKER: &str = "--restart-after";

/// Remove only a trailing restart marker, preserving all application arguments.
fn split_args(args: &[OsString]) -> Result<(Vec<OsString>, Option<u32>), String> {
    if args.last().is_some_and(|arg| arg == OsStr::new(MARKER)) {
        return Err("Маркер перезапуска не содержит PID".into());
    }
    if args.len() >= 2 && args[args.len() - 2] == OsStr::new(MARKER) {
        let pid = args
            .last()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|pid| *pid != 0)
            .ok_or("Некорректный PID перезапуска")?;
        return Ok((args[..args.len() - 2].to_vec(), Some(pid)));
    }
    Ok((args.to_vec(), None))
}

/// Start a new copy of this exact executable; caller closes the old instance
/// only after this returns success. No shell is involved.
pub(super) fn spawn_successor() -> Result<(), String> {
    let executable: PathBuf = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let (mut args, _) = split_args(&args)?;
    // Main uses the first argument as data root; keep its no-argument default
    // from accidentally becoming the restart marker.
    if args.is_empty() {
        args.push(OsString::from("katran-data"));
    }
    args.push(OsString::from(MARKER));
    args.push(OsString::from(std::process::id().to_string()));
    let mut command = std::process::Command::new(executable);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map_err(|e| format!("Не удалось запустить новую копию Katran: {e}"))?;
    Ok(())
}

/// Call before single-instance acquisition and before reading the data root.
/// An already exited PID is success; any other failure or timeout stops startup.
pub(super) fn wait_for_predecessor() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let (_, pid) = split_args(&args)?;
    let Some(pid) = pid else {
        return Ok(());
    };
    if pid == std::process::id() {
        return Err("Невозможно ждать собственный процесс".into());
    }
    wait_for_pid(pid)
}

#[cfg(windows)]
fn wait_for_pid(pid: u32) -> Result<(), String> {
    use windows::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let handle = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        Ok(handle) => handle,
        Err(error) if error.code().0 as u32 == 0x80070057 => return Ok(()), // ERROR_INVALID_PARAMETER: PID exited.
        Err(error) => return Err(format!("Не удалось дождаться предыдущего Katran: {error}")),
    };
    let status = unsafe { WaitForSingleObject(handle, 30_000) };
    let error = std::io::Error::last_os_error();
    unsafe {
        let _ = CloseHandle(handle);
    }
    if status == WAIT_OBJECT_0 {
        Ok(())
    } else if status == WAIT_TIMEOUT {
        Err("Предыдущий Katran не завершился за 30 секунд".into())
    } else {
        Err(format!("Ошибка ожидания предыдущего Katran: {error}"))
    }
}
#[cfg(not(windows))]
fn wait_for_pid(_: u32) -> Result<(), String> {
    Err("Автоматический перезапуск доступен только в Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn trailing_marker_is_removed_without_changing_data_path_or_other_args() {
        assert_eq!(
            split_args(&args(&["C:\\test folder", "--preview", MARKER, "42"])).unwrap(),
            (args(&["C:\\test folder", "--preview"]), Some(42))
        );
        assert_eq!(
            split_args(&args(&["root", "--other"])).unwrap(),
            (args(&["root", "--other"]), None)
        );
        assert_eq!(split_args(&[]).unwrap(), (vec![], None));
    }
    #[test]
    fn malformed_markers_are_rejected_without_starting_processes() {
        for input in [
            args(&["root", MARKER]),
            args(&["root", MARKER, "0"]),
            args(&["root", MARKER, "abc"]),
        ] {
            assert!(split_args(&input).is_err());
        }
    }
}

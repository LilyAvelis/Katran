//! Explicit successor startup: never terminates or manipulates the predecessor.
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    path::{Path, PathBuf},
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

/// Arguments for normal launch parsing, with a validated trailing restart marker removed.
pub(super) fn startup_args() -> Result<Vec<OsString>, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    Ok(split_args(&args)?.0)
}

/// Resolve only the known runtime/versions/<id>/katran-ui.exe installation layout.
fn successor_target(current: &Path) -> Result<PathBuf, String> {
    let Some(version_dir) = current.parent() else {
        return Ok(current.to_owned());
    };
    let Some(versions) = version_dir.parent() else {
        return Ok(current.to_owned());
    };
    let Some(runtime) = versions.parent() else {
        return Ok(current.to_owned());
    };
    if current.file_name() != Some(OsStr::new("katran-ui.exe"))
        || versions.file_name() != Some(OsStr::new("versions"))
        || runtime.file_name() != Some(OsStr::new("runtime"))
    {
        return Ok(current.to_owned());
    }
    let mut pointer = String::new();
    let file = std::fs::File::open(runtime.join("current-version.txt"))
        .map_err(|e| format!("Не удалось прочитать текущую версию Katran: {e}"))?;
    file.take(65)
        .read_to_string(&mut pointer)
        .map_err(|e| format!("Некорректная текущая версия Katran: {e}"))?;
    let id = pointer.trim();
    if pointer.len() > 64 || id.len() != 12 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Некорректный указатель текущей версии Katran".into());
    }
    let target = versions.join(id).join("katran-ui.exe");
    if !target.is_file() {
        return Err("Файл текущей версии Katran отсутствует".into());
    }
    Ok(target)
}

fn successor_args(args: &[OsString], pid: u32) -> Result<Vec<OsString>, String> {
    if pid == 0 {
        return Err("Некорректный PID перезапуска".into());
    }
    let (args, _) = split_args(args)?;
    let flag = OsStr::new("--open-folder");
    let profile = match args.as_slice() {
        [] => None,
        [profile] if !profile.to_string_lossy().starts_with("--") => Some(profile.clone()),
        [arg, folder] if arg == flag && !folder.is_empty() => None,
        [profile, arg, folder]
            if !profile.to_string_lossy().starts_with("--")
                && arg == flag
                && !folder.is_empty() =>
        {
            Some(profile.clone())
        }
        _ => return Err("Некорректные аргументы запуска Katran".into()),
    };
    let profile = match profile {
        Some(profile) => profile,
        None => std::env::current_dir()
            .map_err(|e| format!("Не удалось определить профиль: {e}"))?
            .join("katran-data")
            .into_os_string(),
    };
    Ok(vec![
        profile,
        OsString::from(MARKER),
        OsString::from(pid.to_string()),
    ])
}

/// Start the current installed version; caller closes the old instance only
/// after this returns success. No shell is involved.
pub(super) fn spawn_successor() -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let executable = successor_target(&current)?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let args = successor_args(&args, std::process::id())?;
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
    #[test]
    fn successor_drops_folder_intent_and_stale_marker() {
        assert_eq!(
            successor_args(
                &args(&["C:\\profile", "--open-folder", "C:\\folder", MARKER, "42"]),
                75
            )
            .unwrap(),
            args(&["C:\\profile", MARKER, "75"])
        );
        let implicit = successor_args(&args(&["--open-folder", "folder"]), 75).unwrap();
        assert_eq!(
            PathBuf::from(&implicit[0]),
            std::env::current_dir().unwrap().join("katran-data")
        );
        assert_eq!(&implicit[1..], &args(&[MARKER, "75"]));
    }
    #[test]
    fn installed_successor_uses_pointer_but_development_falls_back() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");
        let old = runtime.join("versions/012345abcdef/katran-ui.exe");
        let new = runtime.join("versions/abcdef012345/katran-ui.exe");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        std::fs::write(runtime.join("current-version.txt"), "abcdef012345\n").unwrap();
        assert_eq!(successor_target(&old).unwrap(), new);
        let dev = temp.path().join("target/debug/katran-ui.exe");
        assert_eq!(successor_target(&dev).unwrap(), dev);
        std::fs::write(runtime.join("current-version.txt"), "../../invalid").unwrap();
        assert!(successor_target(&old).is_err());
        std::fs::write(runtime.join("current-version.txt"), "aaaaaaaaaaaa").unwrap();
        assert!(successor_target(&old).is_err());
        std::fs::remove_file(runtime.join("current-version.txt")).unwrap();
        assert!(successor_target(&old).is_err());
    }
}

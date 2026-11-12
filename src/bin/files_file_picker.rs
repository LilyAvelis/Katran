//! Explicit multi-file picker for adding reference-only Files pointers.
use std::{path::PathBuf, sync::mpsc::Receiver};

pub(super) type PickResult = Result<Option<Vec<PathBuf>>, String>;

#[cfg(any(windows, test))]
fn start_worker(
    worker: impl FnOnce() -> PickResult + Send + 'static,
) -> Result<Receiver<PickResult>, String> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("katran-files-picker".into())
        .spawn(move || {
            let _ = sender.send(worker());
        })
        .map_err(|error| format!("Не удалось открыть выбор файлов: {error}"))?;
    Ok(receiver)
}

/// Opens only on a user action. No file is read, copied or moved here.
#[cfg(windows)]
pub(super) fn start_pick_files() -> Result<Receiver<PickResult>, String> {
    start_worker(pick_on_sta)
}

#[cfg(not(windows))]
pub(super) fn start_pick_files() -> Result<Receiver<PickResult>, String> {
    Err("Выбор файлов доступен только в Windows".into())
}

#[cfg(windows)]
fn pick_on_sta() -> PickResult {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::{
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::Shell::{
            FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
            FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
        },
    };
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
        .map_err(|error| format!("Не удалось инициализировать диалог: {error}"))?;
    let _guard = ComGuard;
    (|| {
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("Не удалось создать диалог: {error}"))?;
        unsafe {
            let options = dialog
                .GetOptions()
                .map_err(|error| format!("Не удалось настроить диалог: {error}"))?;
            dialog
                .SetOptions(
                    options
                        | FOS_ALLOWMULTISELECT
                        | FOS_FORCEFILESYSTEM
                        | FOS_FILEMUSTEXIST
                        | FOS_PATHMUSTEXIST,
                )
                .map_err(|error| format!("Не удалось настроить диалог: {error}"))?;
            if let Err(error) = dialog.Show(None) {
                if error.code().0 as u32 == 0x800704c7 {
                    return Ok(None);
                }
                return Err(format!("Не удалось выбрать файлы: {error}"));
            }
            let items = dialog
                .GetResults()
                .map_err(|error| format!("Не удалось получить файлы: {error}"))?;
            let count = items
                .GetCount()
                .map_err(|error| format!("Не удалось получить количество файлов: {error}"))?;
            if count > 1000 {
                return Err("Можно выбрать не более 1000 файлов за раз".into());
            }
            let mut paths = Vec::with_capacity(count as usize);
            for index in 0..count {
                let item = items
                    .GetItemAt(index)
                    .map_err(|error| format!("Не удалось получить файл: {error}"))?;
                let name = item
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .map_err(|error| format!("Не удалось получить путь файла: {error}"))?;
                if name.0.is_null() {
                    return Err("Не удалось получить путь файла".into());
                }
                let path = PathBuf::from(std::ffi::OsString::from_wide(name.as_wide()));
                CoTaskMemFree(Some(name.0.cast()));
                paths.push(path);
            }
            Ok(Some(paths))
        }
    })()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn picker_worker_returns_without_blocking_ui() {
        let (release, blocked) = mpsc::channel();
        let receiver = start_worker(move || {
            blocked.recv().map_err(|error| error.to_string())?;
            Ok(Some(vec![PathBuf::from("synthetic.pdf")]))
        })
        .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        release.send(()).unwrap();
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
            Some(vec![PathBuf::from("synthetic.pdf")])
        );
    }
}

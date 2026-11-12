//! Explicit native file picker for the optional theme background.
use super::i18n::tr;
use std::{path::PathBuf, sync::mpsc::Receiver};

pub(super) type PickResult = Result<Option<PathBuf>, String>;

#[cfg(any(windows, test))]
fn start_worker(
    worker: impl FnOnce() -> PickResult + Send + 'static,
) -> Result<Receiver<PickResult>, String> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("katran-theme-picker".into())
        .spawn(move || {
            let _ = sender.send(worker());
        })
        .map_err(|error| {
            format!(
                "{}: {error}",
                tr(
                    "Не удалось открыть выбор файла",
                    "Could not open file picker"
                )
            )
        })?;
    Ok(receiver)
}

/// Opens only on a user action. No file is read or copied by this adapter.
#[cfg(windows)]
pub(super) fn start_pick_image() -> Result<Receiver<PickResult>, String> {
    start_worker(|| pick_on_sta(PickKind::Image))
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum PickKind {
    Image,
    Font,
    Video,
}

#[cfg(windows)]
fn pick_on_sta(kind: PickKind) -> Result<Option<PathBuf>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        Win32::{
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{
                Common::COMDLG_FILTERSPEC, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
                FOS_PATHMUSTEXIST, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
            },
        },
        core::w,
    };
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }.map_err(|error| {
        format!(
            "{}: {error}",
            tr(
                "Не удалось инициализировать диалог",
                "Could not initialize dialog"
            )
        )
    })?;
    let _guard = ComGuard;
    // The closure drops all COM interfaces before the apartment guard.
    (|| {
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.map_err(
                |error| {
                    format!(
                        "{}: {error}",
                        tr("Не удалось создать диалог", "Could not create dialog")
                    )
                },
            )?;
        let filters = match kind {
            PickKind::Font => [COMDLG_FILTERSPEC {
                pszName: if super::i18n::is_english() {
                    w!("Fonts (TTF, TTC, OTF)")
                } else {
                    w!("Шрифты (TTF, TTC, OTF)")
                },
                pszSpec: w!("*.ttf;*.ttc;*.otf"),
            }],
            PickKind::Video => [COMDLG_FILTERSPEC {
                pszName: if super::i18n::is_english() {
                    w!("Videos (MP4, WEBM, MOV, MKV)")
                } else {
                    w!("Видео (MP4, WEBM, MOV, MKV)")
                },
                pszSpec: w!("*.mp4;*.webm;*.mov;*.mkv"),
            }],
            PickKind::Image => [COMDLG_FILTERSPEC {
                pszName: if super::i18n::is_english() {
                    w!("Images (PNG, JPG, WEBP, GIF, BMP)")
                } else {
                    w!("Изображения (PNG, JPG, WEBP, GIF, BMP)")
                },
                pszSpec: w!("*.png;*.jpg;*.jpeg;*.webp;*.gif;*.bmp"),
            }],
        };
        unsafe {
            dialog.SetFileTypes(&filters).map_err(|error| {
                format!(
                    "{}: {error}",
                    tr(
                        "Не удалось задать фильтр файлов",
                        "Could not set file filter"
                    )
                )
            })?;
            let options = dialog.GetOptions().map_err(|error| {
                format!(
                    "{}: {error}",
                    tr("Не удалось настроить диалог", "Could not configure dialog")
                )
            })?;
            dialog
                .SetOptions(options | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST)
                .map_err(|error| {
                    format!(
                        "{}: {error}",
                        tr("Не удалось настроить диалог", "Could not configure dialog")
                    )
                })?;
            if let Err(error) = dialog.Show(None) {
                // HRESULT_FROM_WIN32(ERROR_CANCELLED) is ordinary cancellation.
                if error.code().0 as u32 == 0x800704c7 {
                    return Ok(None);
                }
                return Err(format!(
                    "{}: {error}",
                    tr("Не удалось выбрать файл", "Could not select file")
                ));
            }
            let item = dialog.GetResult().map_err(|error| {
                format!(
                    "{}: {error}",
                    tr("Не удалось получить файл", "Could not retrieve file")
                )
            })?;
            let name = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(|error| {
                format!(
                    "{}: {error}",
                    tr(
                        "Не удалось получить путь файла",
                        "Could not retrieve file path"
                    )
                )
            })?;
            if name.0.is_null() {
                return Err(tr(
                    "Не удалось получить путь файла",
                    "Could not retrieve file path",
                )
                .into());
            }
            let path = PathBuf::from(std::ffi::OsString::from_wide(name.as_wide()));
            CoTaskMemFree(Some(name.0.cast()));
            Ok(Some(path))
        }
    })()
}

#[cfg(windows)]
pub(super) fn start_pick_font() -> Result<Receiver<PickResult>, String> {
    start_worker(|| pick_on_sta(PickKind::Font))
}
#[cfg(not(windows))]
pub(super) fn start_pick_font() -> Result<Receiver<PickResult>, String> {
    Err(tr(
        "Выбор шрифта доступен только в Windows",
        "Font selection is available only on Windows",
    )
    .into())
}

#[cfg(windows)]
pub(super) fn start_pick_video() -> Result<Receiver<PickResult>, String> {
    start_worker(|| pick_on_sta(PickKind::Video))
}
#[cfg(not(windows))]
pub(super) fn start_pick_video() -> Result<Receiver<PickResult>, String> {
    Err(tr(
        "Выбор видео доступен только в Windows",
        "Video selection is available only on Windows",
    )
    .into())
}

#[cfg(not(windows))]
pub(super) fn start_pick_image() -> Result<Receiver<PickResult>, String> {
    Err(tr(
        "Выбор изображения доступен только в Windows",
        "Image selection is available only on Windows",
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn worker_returns_immediately_and_delivers_result_later() {
        let (release, blocked) = mpsc::channel();
        let receiver = start_worker(move || {
            blocked.recv().map_err(|error| error.to_string())?;
            Ok(Some(PathBuf::from("synthetic.png")))
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
            Some(PathBuf::from("synthetic.png"))
        );
    }
}

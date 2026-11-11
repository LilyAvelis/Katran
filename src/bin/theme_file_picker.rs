//! Explicit native file picker for the optional theme background.
use std::path::PathBuf;

/// Opens only on a user action. No file is read or copied by this adapter.
#[cfg(windows)]
pub(super) fn pick_image() -> Result<Option<PathBuf>, String> {
    std::thread::Builder::new()
        .name("katran-theme-picker".into())
        .spawn(pick_on_sta)
        .map_err(|error| format!("Не удалось открыть выбор изображения: {error}"))?
        .join()
        .map_err(|_| "Диалог выбора изображения завершился с ошибкой".to_owned())?
}

#[cfg(windows)]
fn pick_on_sta() -> Result<Option<PathBuf>, String> {
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
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
        .map_err(|error| format!("Не удалось инициализировать диалог: {error}"))?;
    let _guard = ComGuard;
    // The closure drops all COM interfaces before the apartment guard.
    (|| {
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("Не удалось создать диалог: {error}"))?;
        let filters = [COMDLG_FILTERSPEC {
            pszName: w!("Изображения (PNG, JPG, WEBP, GIF, BMP)"),
            pszSpec: w!("*.png;*.jpg;*.jpeg;*.webp;*.gif;*.bmp"),
        }];
        unsafe {
            dialog
                .SetFileTypes(&filters)
                .map_err(|error| format!("Не удалось задать фильтр изображений: {error}"))?;
            let options = dialog
                .GetOptions()
                .map_err(|error| format!("Не удалось настроить диалог: {error}"))?;
            dialog
                .SetOptions(options | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST)
                .map_err(|error| format!("Не удалось настроить диалог: {error}"))?;
            if let Err(error) = dialog.Show(None) {
                // HRESULT_FROM_WIN32(ERROR_CANCELLED) is ordinary cancellation.
                if error.code().0 as u32 == 0x800704c7 {
                    return Ok(None);
                }
                return Err(format!("Не удалось выбрать изображение: {error}"));
            }
            let item = dialog
                .GetResult()
                .map_err(|error| format!("Не удалось получить файл: {error}"))?;
            let name = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|error| format!("Не удалось получить путь изображения: {error}"))?;
            if name.0.is_null() {
                return Err("Не удалось получить путь изображения".into());
            }
            let path = PathBuf::from(std::ffi::OsString::from_wide(name.as_wide()));
            CoTaskMemFree(Some(name.0.cast()));
            Ok(Some(path))
        }
    })()
}

#[cfg(not(windows))]
pub(super) fn pick_image() -> Result<Option<PathBuf>, String> {
    Err("Выбор изображения доступен только в Windows".into())
}

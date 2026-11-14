//! Windows desktop wallpaper adapter. The documented API targets monitors;
//! it does not specify a virtual-desktop target or guarantee its scope.
use std::path::{Path, PathBuf};

/// The caller owns image validation, revision checking and durable file lifetime.
pub trait WallpaperHost: Send + Sync {
    fn set_wallpaper(&self, path: &Path) -> std::result::Result<(), String>;
}

pub struct NativeWallpaperHost;

/// Requests the same image on all monitors. Success is an OS acknowledgement,
/// not proof of visual presentation or of a particular virtual-desktop scope.
pub fn set_wallpaper(path: &Path, host: &dyn WallpaperHost) -> std::result::Result<(), String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("Wallpaper must be an existing file with an absolute path".into());
    }
    host.set_wallpaper(path)
}

/// Returns the image paths reported for all monitors, including detached monitors
/// with an assigned image. This does not enumerate other virtual desktops, and
/// Windows may report a derived image path; absence is not proof a file is unused.
#[cfg(windows)]
pub fn current_wallpapers() -> std::result::Result<Vec<PathBuf>, String> {
    std::thread::spawn(|| {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        use windows::{
            Win32::{
                System::Com::{
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                    CoInitializeEx, CoTaskMemFree, CoUninitialize,
                },
                UI::Shell::{DesktopWallpaper, IDesktopWallpaper},
            },
            core::{PCWSTR, PWSTR},
        };
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { CoUninitialize() }
            }
        }
        struct ComString(PWSTR);
        impl Drop for ComString {
            fn drop(&mut self) {
                unsafe { CoTaskMemFree(Some(self.0.0.cast())) }
            }
        }
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
            .map_err(|e| format!("Initializing wallpaper service failed: {e}"))?;
        let _apartment = Apartment;
        let wallpaper: IDesktopWallpaper =
            unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_INPROC_SERVER) }
                .map_err(|e| format!("Opening wallpaper service failed: {e}"))?;
        let count = unsafe { wallpaper.GetMonitorDevicePathCount() }
            .map_err(|e| format!("Reading wallpaper monitor count failed: {e}"))?;
        if count == 0 {
            return Err("No wallpaper monitors were reported".into());
        }
        let mut paths = Vec::new();
        for index in 0..count {
            let monitor = ComString(
                unsafe { wallpaper.GetMonitorDevicePathAt(index) }
                    .map_err(|e| format!("Reading wallpaper monitor {index} failed: {e}"))?,
            );
            if monitor.0.is_null() {
                return Err(format!("Wallpaper monitor {index} has no identifier"));
            }
            let path = ComString(
                unsafe { wallpaper.GetWallpaper(PCWSTR(monitor.0.0)) }
                    .map_err(|e| format!("Reading wallpaper for monitor {index} failed: {e}"))?,
            );
            if path.0.is_null() {
                return Err(format!("Wallpaper for monitor {index} has no result"));
            }
            let value = unsafe { path.0.as_wide() };
            if !value.is_empty() {
                paths.push(PathBuf::from(OsString::from_wide(value)));
            }
        }
        Ok(paths)
    })
    .join()
    .map_err(|_| "Wallpaper service thread failed".to_string())?
}

#[cfg(not(windows))]
pub fn current_wallpapers() -> std::result::Result<Vec<PathBuf>, String> {
    Err("Desktop wallpaper is only available on Windows".into())
}

#[cfg(windows)]
impl WallpaperHost for NativeWallpaperHost {
    fn set_wallpaper(&self, path: &Path) -> std::result::Result<(), String> {
        use std::os::windows::ffi::OsStrExt;
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err("Wallpaper path contains a null character".into());
        }
        wide.push(0);
        // A dedicated thread avoids changing the caller's COM apartment model.
        std::thread::spawn(move || {
            use windows::{
                Win32::{
                    System::Com::{
                        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                        CoInitializeEx, CoUninitialize,
                    },
                    UI::Shell::{DesktopWallpaper, IDesktopWallpaper},
                },
                core::PCWSTR,
            };
            struct Apartment;
            impl Drop for Apartment {
                fn drop(&mut self) {
                    unsafe { CoUninitialize() }
                }
            }
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
                .map_err(|e| format!("Initializing wallpaper service failed: {e}"))?;
            let _apartment = Apartment;
            let wallpaper: IDesktopWallpaper =
                unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_INPROC_SERVER) }
                    .map_err(|e| format!("Opening wallpaper service failed: {e}"))?;
            unsafe { wallpaper.SetWallpaper(PCWSTR::null(), PCWSTR(wide.as_ptr())) }
                .map_err(|e| format!("Setting desktop wallpaper failed: {e}"))
        })
        .join()
        .map_err(|_| "Wallpaper service thread failed".to_string())?
    }
}

#[cfg(not(windows))]
impl WallpaperHost for NativeWallpaperHost {
    fn set_wallpaper(&self, _path: &Path) -> std::result::Result<(), String> {
        Err("Desktop wallpaper is only available on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeHost {
        calls: Mutex<Vec<std::path::PathBuf>>,
        fail: bool,
    }
    impl WallpaperHost for FakeHost {
        fn set_wallpaper(&self, path: &Path) -> std::result::Result<(), String> {
            self.calls.lock().unwrap().push(path.to_path_buf());
            if self.fail {
                Err("synthetic failure".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn validates_and_propagates_host_result_without_touching_desktop() {
        let temp = tempfile::tempdir().unwrap();
        let image = temp.path().join("synthetic.png");
        std::fs::write(&image, b"synthetic").unwrap();
        let fake = FakeHost {
            calls: Mutex::new(Vec::new()),
            fail: false,
        };
        set_wallpaper(&image, &fake).unwrap();
        assert_eq!(*fake.calls.lock().unwrap(), vec![image.clone()]);
        assert!(set_wallpaper(Path::new("relative.png"), &fake).is_err());
        assert!(set_wallpaper(&temp.path().join("missing.png"), &fake).is_err());
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
        let failing = FakeHost {
            calls: Mutex::new(Vec::new()),
            fail: true,
        };
        assert_eq!(
            set_wallpaper(&image, &failing).unwrap_err(),
            "synthetic failure"
        );
    }
}

//! Explicit headless per-user registration; never launches the UI.
use std::{io, path::Path};
#[cfg(windows)]
pub fn write_link(project: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{
            Foundation::PROPERTYKEY,
            System::{
                Com::{
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                    CoInitializeEx, CoUninitialize, IPersistFile,
                    StructuredStorage::{PROPVARIANT, PVCHF_DEFAULT, PropVariantChangeType},
                },
                Variant::VT_LPWSTR,
            },
            UI::Shell::{IShellLinkW, PropertiesSystem::IPropertyStore, ShellLink},
        },
        core::{GUID, Interface, PCWSTR},
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let err = |e: windows::core::Error| io::Error::other(e.to_string());
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(err)?;
        let _guard = Apartment;
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(err)?;
        let launcher = wide(&project.join("runtime/Katran.exe"));
        let icon_file = project.join("assets/branding/katran.ico");
        let icon = wide(&if icon_file.is_file() {
            icon_file
        } else {
            project.join("runtime/Katran.exe")
        });
        let cwd = wide(project);
        let data = super::launcher_layout::data(
            project,
            None,
            std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from),
        )?;
        let args = format!("\"{}\"", data.display())
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        link.SetPath(PCWSTR(launcher.as_ptr())).map_err(err)?;
        link.SetWorkingDirectory(PCWSTR(cwd.as_ptr()))
            .map_err(err)?;
        link.SetArguments(PCWSTR(args.as_ptr())).map_err(err)?;
        link.SetIconLocation(PCWSTR(icon.as_ptr()), 0)
            .map_err(err)?;
        link.SetDescription(windows::core::w!("Katran — local media shelf"))
            .map_err(err)?;
        let store: IPropertyStore = link.cast().map_err(err)?;
        let app_id = PROPERTYKEY {
            fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
            pid: 5,
        };
        let mut value = PROPVARIANT::default();
        PropVariantChangeType(
            &mut value,
            &PROPVARIANT::from("LiliaAvelis.Katran"),
            PVCHF_DEFAULT,
            VT_LPWSTR,
        )
        .map_err(err)?;
        store.SetValue(&app_id, &value).map_err(err)?;
        store.Commit().map_err(err)?;
        let persist: IPersistFile = link.cast().map_err(err)?;
        let destination = wide(destination);
        persist
            .Save(PCWSTR(destination.as_ptr()), true)
            .map_err(err)?;
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn write_link(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::other(
        "Application registration requires Windows",
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn synthetic_shortcut_roundtrips_its_app_id_and_arguments() {
        use windows::{
            Win32::{
                Foundation::PROPERTYKEY,
                System::Com::{
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                    CoInitializeEx, CoUninitialize, IPersistFile, STGM_READ,
                },
                UI::Shell::{IShellLinkW, PropertiesSystem::IPropertyStore, ShellLink},
            },
            core::{BSTR, GUID, Interface, PCWSTR},
        };
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("Катран with spaces");
        std::fs::create_dir_all(project.join("runtime")).unwrap();
        std::fs::write(project.join("runtime/Katran.exe"), b"synthetic").unwrap();
        let destination = root.path().join("Katran.lnk");
        write_link(&project, &destination).unwrap();
        assert!(destination.is_file());
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            {
                let link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                let persist: IPersistFile = link.cast().unwrap();
                let path = destination
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<_>>();
                persist.Load(PCWSTR(path.as_ptr()), STGM_READ).unwrap();
                let store: IPropertyStore = link.cast().unwrap();
                let key = PROPERTYKEY {
                    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
                    pid: 5,
                };
                let value = store.GetValue(&key).unwrap();
                assert_eq!(
                    BSTR::try_from(&value).unwrap().to_string(),
                    "LiliaAvelis.Katran"
                );
                let mut args = [0u16; 1024];
                link.GetArguments(&mut args).unwrap();
                let len = args.iter().position(|c| *c == 0).unwrap();
                assert_eq!(
                    String::from_utf16_lossy(&args[..len]),
                    format!("\"{}\"", project.join("katran-data").display())
                );
            }
            CoUninitialize();
        }
    }
}

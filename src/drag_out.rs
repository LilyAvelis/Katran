//! Native OLE copy-only drag. No clipboard replacement, no source file moves.
use std::path::PathBuf;

pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub struct Outcome {
    pub dropped: bool,
    pub warnings: Vec<String>,
}
/// The caller must run the native implementation on the owner window thread,
/// outside egui painting/context locks. Test runners have no OS side effects.
pub trait Runner: Send {
    fn run(
        &mut self,
        paths: &[PathBuf],
        preview: Option<&Preview>,
        owner_window: u64,
    ) -> Result<Outcome, String>;
}
pub struct NativeRunner;
impl Runner for NativeRunner {
    fn run(
        &mut self,
        paths: &[PathBuf],
        preview: Option<&Preview>,
        owner_window: u64,
    ) -> Result<Outcome, String> {
        if paths.is_empty() {
            return Err("No files to drag".into());
        }
        #[cfg(windows)]
        {
            native::run(paths, preview, owner_window)
        }
        #[cfg(not(windows))]
        {
            let _ = (preview, owner_window);
            Err("Native drag requires Windows".into())
        }
    }
}

pub(crate) struct RunnerSink<'a> {
    pub runner: &'a mut dyn Runner,
    pub preview: Option<&'a Preview>,
    pub owner_window: u64,
}
impl crate::copy_out::ClipboardSink for RunnerSink<'_> {
    fn write(&mut self, paths: &[PathBuf]) -> Result<(u32, Vec<String>), String> {
        let result = self.runner.run(paths, self.preview, self.owner_window)?;
        if result.dropped {
            Ok((0, result.warnings))
        } else {
            Err("drag_cancelled".into())
        }
    }
}

#[cfg(windows)]
#[path = "drag_image.rs"]
mod drag_image;
#[cfg(windows)]
mod native {
    use std::path::PathBuf;
    use windows::Win32::{
        Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS},
        System::{
            Com::{CoTaskMemFree, IDataObject},
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DoDragDrop, IDropSource, IDropSource_Impl,
                OleInitialize, OleUninitialize,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::Shell::{
            BHID_DataObject, Common::ITEMIDLIST, SHCreateShellItemArrayFromIDLists,
            SHParseDisplayName,
        },
    };
    use windows::core::{BOOL, HRESULT, PCWSTR, implement};
    #[implement(IDropSource)]
    struct Source;
    impl IDropSource_Impl for Source_Impl {
        fn QueryContinueDrag(&self, escape: BOOL, state: MODIFIERKEYS_FLAGS) -> HRESULT {
            if escape.as_bool() {
                DRAGDROP_S_CANCEL
            } else if state.0 & MK_LBUTTON.0 == 0 {
                DRAGDROP_S_DROP
            } else {
                HRESULT(0)
            }
        }
        fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { OleUninitialize() }
        }
    }
    struct Items(Vec<*mut ITEMIDLIST>);
    impl Drop for Items {
        fn drop(&mut self) {
            for p in &self.0 {
                unsafe { CoTaskMemFree(Some((*p).cast())) }
            }
        }
    }
    fn build_data(paths: &[PathBuf]) -> Result<IDataObject, String> {
        unsafe {
            let mut items = Items(Vec::new());
            for path in paths {
                use std::os::windows::ffi::OsStrExt;
                // Shell requires normal Win32 paths, not extended canonicalization prefixes.
                let text = path.to_string_lossy();
                let text = if let Some(p) = text.strip_prefix("\\\\?\\UNC\\") {
                    format!("\\\\{p}")
                } else {
                    text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
                };
                let wide: Vec<u16> = std::ffi::OsStr::new(&text)
                    .encode_wide()
                    .chain(Some(0))
                    .collect();
                let mut pidl = std::ptr::null_mut();
                SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)
                    .map_err(|e| e.to_string())?;
                items.0.push(pidl);
            }
            let pointers: Vec<*const ITEMIDLIST> = items.0.iter().map(|p| *p as *const _).collect();
            let data: IDataObject = SHCreateShellItemArrayFromIDLists(&pointers)
                .and_then(|items| items.BindToHandler(None, &BHID_DataObject))
                .map_err(|e| e.to_string())?;

            Ok(data)
        }
    }
    pub fn run(
        paths: &[PathBuf],
        preview: Option<&super::Preview>,
        owner_window: u64,
    ) -> Result<super::Outcome, String> {
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                GetAsyncKeyState, ReleaseCapture, VK_ESCAPE, VK_LBUTTON,
            };
            use windows::Win32::{
                Foundation::HWND,
                System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
                UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
            };
            let owner = HWND(owner_window as usize as *mut _);
            let mut process = 0;
            if !IsWindow(Some(owner)).as_bool()
                || GetWindowThreadProcessId(owner, Some(&mut process)) != GetCurrentThreadId()
                || process != GetCurrentProcessId()
            {
                return Err("Native drag must run on its owner window thread".into());
            }
            let cancelled = || super::Outcome {
                dropped: false,
                warnings: vec![],
            };
            if GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000 == 0
                || GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0
            {
                return Ok(cancelled());
            }
            OleInitialize(None).map_err(|e| e.to_string())?;
            let _apartment = Apartment;
            let data = build_data(paths)?;
            let mut warnings = Vec::new();
            if let Some(preview) = preview
                && let Err(error) = super::drag_image::attach(&data, preview)
            {
                warnings.push(format!("Drag preview unavailable: {error}"));
            }
            let source: IDropSource = Source.into();
            let mut effect = DROPEFFECT(0);
            if GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000 == 0
                || GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0
            {
                return Ok(cancelled());
            }
            // Winit captures the mouse on button-down. OLE must take over on
            // this SAME thread; a background modal loop misses release events.
            let _ = ReleaseCapture();
            let result = DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect);
            if result == DRAGDROP_S_CANCEL {
                return Ok(cancelled());
            }
            result.ok().map_err(|e| e.to_string())?;
            Ok(super::Outcome {
                dropped: result == DRAGDROP_S_DROP && effect.0 & DROPEFFECT_COPY.0 != 0,
                warnings,
            })
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn ownerless_native_call_is_rejected_before_any_drag_loop() {
            let error = run(&[], None, 0).err().unwrap();
            assert!(error.contains("owner window thread"));
        }
        #[test]
        fn shell_payload_contains_group_without_touching_clipboard() {
            use windows::Win32::{
                System::{
                    Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL},
                    Ole::ReleaseStgMedium,
                },
                UI::Shell::{DragQueryFileW, HDROP},
            };
            let a = tempfile::tempdir().unwrap();
            let b = tempfile::tempdir().unwrap();
            let paths = vec![a.path().join("first.png"), b.path().join("вторая.png")];
            for path in &paths {
                std::fs::write(path, b"synthetic").unwrap();
            }
            unsafe {
                OleInitialize(None).unwrap();
                let _guard = Apartment;
                let data = build_data(&paths).unwrap();
                let format = FORMATETC {
                    cfFormat: 15,
                    ptd: std::ptr::null_mut(),
                    dwAspect: DVASPECT_CONTENT.0,
                    lindex: -1,
                    tymed: TYMED_HGLOBAL.0 as u32,
                };
                data.QueryGetData(&format).ok().unwrap();
                let mut medium = data.GetData(&format).unwrap();
                let count = DragQueryFileW(HDROP(medium.u.hGlobal.0), u32::MAX, None);
                ReleaseStgMedium(&mut medium);
                assert_eq!(count, 2);
                let source: IDropSource = Source.into();
                assert_eq!(
                    source.QueryContinueDrag(true, MK_LBUTTON),
                    DRAGDROP_S_CANCEL
                );
                assert_eq!(
                    source.QueryContinueDrag(false, MODIFIERKEYS_FLAGS(0)),
                    DRAGDROP_S_DROP
                );
            }
            for path in paths {
                assert!(path.is_file());
            }
        }
    }
}

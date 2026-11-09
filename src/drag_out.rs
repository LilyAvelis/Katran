//! Native OLE copy-only drag. No clipboard replacement, no source file moves.
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
    pub fn run(paths: &[PathBuf]) -> Result<bool, String> {
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON,
            };
            if GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000 == 0
                || GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0
            {
                return Ok(false);
            }
            OleInitialize(None).map_err(|e| e.to_string())?;
            let _apartment = Apartment;
            let data = build_data(paths)?;
            let source: IDropSource = Source.into();
            let mut effect = DROPEFFECT(0);
            let result = DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect);
            if result == DRAGDROP_S_CANCEL {
                return Ok(false);
            }
            result.ok().map_err(|e| e.to_string())?;
            Ok(result == DRAGDROP_S_DROP && effect.0 & DROPEFFECT_COPY.0 != 0)
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
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
pub fn run(paths: &[std::path::PathBuf]) -> Result<bool, String> {
    if paths.is_empty() {
        return Err("No files to drag".into());
    }
    #[cfg(windows)]
    {
        native::run(paths)
    }
    #[cfg(not(windows))]
    {
        Err("Native drag requires Windows".into())
    }
}
pub struct NativeDrag;
impl crate::copy_out::ClipboardSink for NativeDrag {
    fn write(&mut self, paths: &[std::path::PathBuf]) -> Result<(u32, Vec<String>), String> {
        if run(paths)? {
            Ok((0, vec![]))
        } else {
            Err("drag_cancelled".into())
        }
    }
}

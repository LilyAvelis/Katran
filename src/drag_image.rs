//! Windows Shell cursor-following image; independent of egui's canvas.
use super::Preview;
use windows::Win32::{
    Foundation::{COLORREF, POINT, SIZE},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
    },
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IDataObject},
    UI::Shell::{CLSID_DragDropHelper, IDragSourceHelper, SHDRAGIMAGE},
};

pub(super) fn attach(data: &IDataObject, preview: &Preview) -> Result<(), String> {
    if preview.width == 0
        || preview.height == 0
        || preview.width > 160
        || preview.height > 160
        || preview.rgba.len() != preview.width as usize * preview.height as usize * 4
    {
        return Err("Invalid drag preview dimensions".into());
    }
    unsafe {
        let helper: IDragSourceHelper =
            CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: preview.width as i32,
                biHeight: -(preview.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| e.to_string())?;
        let output = std::slice::from_raw_parts_mut(bits.cast::<u8>(), preview.rgba.len());
        for (dest, rgba) in output.chunks_exact_mut(4).zip(preview.rgba.chunks_exact(4)) {
            // InitializeFromBitmap expects straight BGRA, not premultiplied.
            dest.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
        let image = SHDRAGIMAGE {
            sizeDragImage: SIZE {
                cx: preview.width as i32,
                cy: preview.height as i32,
            },
            ptOffset: POINT {
                x: 12.min(preview.width as i32),
                y: 12.min(preview.height as i32),
            },
            hbmpDragImage: bitmap,
            crColorKey: COLORREF(0xffff_ffff),
        };
        // On success the Shell helper owns the bitmap; on failure we retain it.
        match helper.InitializeFromBitmap(&image, data) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = DeleteObject(bitmap.into());
                Err(e.to_string())
            }
        }
    }
}

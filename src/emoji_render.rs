//! Windows-native, bounded, whole-string color text rasterization.
//! Returns straight-alpha RGBA without accessing the desktop or clipboard.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
const MAX_UNITS: usize = 128;
const MAX_WIDTH: u32 = 1024;
const MAX_HEIGHT: u32 = 256;

pub fn render(text: &str, size: f32) -> Result<Rendered, String> {
    if text.is_empty()
        || text.encode_utf16().count() > MAX_UNITS
        || text.chars().any(char::is_control)
    {
        return Err("Text must contain 1..128 UTF-16 units and no control characters".into());
    }
    if !size.is_finite() || !(12.0..=128.0).contains(&size) {
        return Err("Font size must be finite and in 12..128".into());
    }
    #[cfg(windows)]
    {
        render_with_flags(text, size)
    }
    #[cfg(not(windows))]
    {
        platform::render(text, size)
    }
}

#[cfg(windows)]
mod flags {
    include!("../assets/emoji-flags/flags.rs");
}

#[cfg(windows)]
fn render_flag(svg: &str, size: f32) -> Result<Rendered, String> {
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .map_err(|e| format!("Invalid bundled flag: {e}"))?;
    let side = size.ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(side, side).ok_or("Invalid flag dimensions")?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        side as f32 / tree.size().width(),
        side as f32 / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut rgba = pixmap.data().to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        let a = u32::from(pixel[3]);
        if a > 0 {
            for channel in &mut pixel[..3] {
                *channel = (u32::from(*channel) * 255 + a / 2)
                    .checked_div(a)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
        }
    }
    Ok(Rendered {
        width: side,
        height: side,
        rgba,
    })
}
#[cfg(windows)]
fn render_with_flags(text: &str, size: f32) -> Result<Rendered, String> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut parts = Vec::new();
    let mut run = 0;
    let mut i = 0;
    while i + 1 < chars.len() {
        let (start, a) = chars[i];
        let (_, b) = chars[i + 1];
        if ('\u{1f1e6}'..='\u{1f1ff}').contains(&a) && ('\u{1f1e6}'..='\u{1f1ff}').contains(&b) {
            let key = format!("{:x}-{:x}", a as u32, b as u32);
            if let Some(svg) = flags::flag_svg(&key) {
                if run < start {
                    parts.push(platform::render(&text[run..start], size)?);
                }
                parts.push(render_flag(svg, size)?);
                i += 2;
                run = chars.get(i).map_or(text.len(), |entry| entry.0);
                continue;
            }
        }
        i += 1;
    }
    if parts.is_empty() {
        return platform::render(text, size);
    }
    if run < text.len() {
        parts.push(platform::render(&text[run..], size)?);
    }
    let width = parts
        .iter()
        .try_fold(0u32, |sum, p| sum.checked_add(p.width))
        .ok_or("Rendered text exceeds bitmap dimensions")?;
    let height = parts.iter().map(|p| p.height).max().unwrap_or(1);
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err("Rendered text exceeds maximum bitmap dimensions".into());
    }
    let mut rgba = vec![0; width as usize * height as usize * 4];
    let mut x = 0usize;
    for part in parts {
        let offset_y = (height - part.height) as usize / 2;
        let row = part.width as usize * 4;
        for y in 0..part.height as usize {
            let dst = ((y + offset_y) * width as usize + x) * 4;
            rgba[dst..dst + row].copy_from_slice(&part.rgba[y * row..(y + 1) * row]);
        }
        x += part.width as usize;
    }
    Ok(Rendered {
        width,
        height,
        rgba,
    })
}

#[cfg(not(windows))]
mod platform {
    use super::Rendered;
    pub fn render(_: &str, _: f32) -> Result<Rendered, String> {
        Err("Native color text rendering requires Windows".into())
    }
}

#[cfg(windows)]
mod platform {
    use super::{MAX_HEIGHT, MAX_WIDTH, Rendered};
    use windows::{
        Win32::{
            Graphics::{
                Direct2D::{
                    self,
                    Common::{
                        D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
                    },
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_FACTORY_TYPE_SINGLE_THREADED,
                    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, ID2D1Factory,
                },
                DirectWrite::{
                    self, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_MEASURING_MODE_NATURAL, DWRITE_TEXT_METRICS,
                    DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory,
                },
                Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                Imaging::{
                    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
                    WICBitmapCacheOnLoad, WICBitmapLockRead, WICRect,
                },
            },
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
        },
        core::w,
    };
    struct Com(bool);
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }
    fn err(e: windows::core::Error) -> String {
        format!("Windows color text renderer: {e}")
    }

    pub fn render(text: &str, size: f32) -> Result<Rendered, String> {
        // Each caller uses its existing COM apartment or initializes one for this call.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if initialized.is_err() && initialized.0 != 0x80010106u32 as i32 {
            return Err(format!("COM initialization failed: {initialized:?}"));
        }
        let _com = Com(initialized.is_ok());
        unsafe {
            let dwrite: IDWriteFactory =
                DirectWrite::DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).map_err(err)?;
            let format = dwrite
                .CreateTextFormat(
                    w!("Segoe UI Emoji"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-us"),
                )
                .map_err(err)?;
            format
                .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .map_err(err)?;
            let utf16: Vec<u16> = text.encode_utf16().collect();
            let layout = dwrite
                .CreateTextLayout(&utf16, &format, MAX_WIDTH as f32, MAX_HEIGHT as f32)
                .map_err(err)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics).map_err(err)?;
            let width = (metrics.widthIncludingTrailingWhitespace.ceil() as u32)
                .saturating_add(8)
                .max(1);
            let height = (metrics.height.ceil() as u32).saturating_add(8).max(1);
            if width > MAX_WIDTH || height > MAX_HEIGHT {
                return Err("Rendered text exceeds maximum bitmap dimensions".into());
            }
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                    .map_err(err)?;
            let bitmap = wic
                .CreateBitmap(
                    width,
                    height,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapCacheOnLoad,
                )
                .map_err(err)?;
            let factory: ID2D1Factory =
                Direct2D::D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                    .map_err(err)?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let target = factory
                .CreateWicBitmapRenderTarget(&bitmap, &props)
                .map_err(err)?;
            let white = D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            };
            let transparent = D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            let brush = target.CreateSolidColorBrush(&white, None).map_err(err)?;
            target.BeginDraw();
            target.Clear(Some(&transparent));
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            target.DrawText(
                &utf16,
                &format,
                &D2D_RECT_F {
                    left: 4.0,
                    top: 4.0,
                    right: width as f32,
                    bottom: height as f32,
                },
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            target.EndDraw(None, None).map_err(err)?;
            drop(brush);
            drop(target);
            let lock = bitmap
                .Lock(
                    &WICRect {
                        X: 0,
                        Y: 0,
                        Width: width as i32,
                        Height: height as i32,
                    },
                    WICBitmapLockRead.0 as u32,
                )
                .map_err(err)?;
            let stride = lock.GetStride().map_err(err)? as usize;
            let mut len = 0u32;
            let mut ptr = std::ptr::null_mut();
            lock.GetDataPointer(&mut len, &mut ptr).map_err(err)?;
            let row = width as usize * 4;
            let required = stride
                .checked_mul(height.saturating_sub(1) as usize)
                .and_then(|v| v.checked_add(row))
                .ok_or("Bitmap size overflow")?;
            if ptr.is_null() || stride < row || (len as usize) < required {
                return Err("Invalid WIC bitmap buffer".into());
            }
            let source = std::slice::from_raw_parts(ptr, required);
            let mut rgba = vec![0; row * height as usize];
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let src = y * stride + x * 4;
                    let dst = y * row + x * 4;
                    let alpha = source[src + 3];
                    let unpremul = |v: u8| {
                        if alpha == 0 {
                            0
                        } else {
                            ((u32::from(v) * 255 + u32::from(alpha) / 2) / u32::from(alpha))
                                .min(255) as u8
                        }
                    };
                    rgba[dst] = unpremul(source[src + 2]);
                    rgba[dst + 1] = unpremul(source[src + 1]);
                    rgba[dst + 2] = unpremul(source[src]);
                    rgba[dst + 3] = alpha;
                }
            }
            Ok(Rendered {
                width,
                height,
                rgba,
            })
        }
    }
}

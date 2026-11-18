//! Private, never-shown WGL surface owned by the player worker thread.
use glow::HasContext;
use std::{
    ffi::{CStr, c_char, c_void},
    ptr,
};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Gdi::{GetDC, HDC, ReleaseDC},
            OpenGL::*,
        },
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP},
    },
    core::{PCSTR, w},
};

type Handle = *mut c_void;
#[repr(C)]
pub(super) struct Init {
    pub resolve: unsafe extern "C" fn(Handle, *const c_char) -> *mut c_void,
    pub context: Handle,
}
#[repr(C)]
pub(super) struct Fbo {
    pub fbo: i32,
    pub width: i32,
    pub height: i32,
    pub format: i32,
}

unsafe extern "C" fn resolve(_: Handle, name: *const c_char) -> *mut c_void {
    if name.is_null() {
        return ptr::null_mut();
    }
    let wgl = unsafe { wglGetProcAddress(PCSTR(name.cast())) };
    let pointer = wgl.map(|p| p as *const () as usize).unwrap_or(0);
    if pointer > 3 && pointer != usize::MAX {
        return pointer as *mut c_void;
    }
    let module = unsafe { GetModuleHandleW(w!("opengl32.dll")) }.unwrap_or(HMODULE::default());
    unsafe { GetProcAddress(module, PCSTR(name.cast())) }
        .map(|p| p as *const () as *mut c_void)
        .unwrap_or(ptr::null_mut())
}

#[derive(Default)]
struct Surface {
    window: HWND,
    dc: HDC,
    context: HGLRC,
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_invalid() {
                let _ = wglMakeCurrent(HDC::default(), HGLRC::default());
                let _ = wglDeleteContext(self.context);
            }
            if !self.dc.is_invalid() {
                let _ = ReleaseDC(Some(self.window), self.dc);
            }
            if !self.window.is_invalid() {
                let _ = DestroyWindow(self.window);
            }
        }
    }
}

pub(super) struct Gpu {
    gl: glow::Context,
    target: Option<(glow::Framebuffer, glow::Texture, u32, u32)>,
    surface: Surface,
}
impl Gpu {
    pub(super) fn new() -> Result<Self, String> {
        unsafe {
            let mut surface = Surface::default();
            surface.window = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| format!("WGL surface: {e}"))?;
            surface.dc = GetDC(Some(surface.window));
            if surface.dc.is_invalid() {
                return Err("WGL device context unavailable".into());
            }
            let pfd = PIXELFORMATDESCRIPTOR {
                nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
                nVersion: 1,
                dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL,
                iPixelType: PFD_TYPE_RGBA,
                cColorBits: 32,
                ..Default::default()
            };
            let format = ChoosePixelFormat(surface.dc, &pfd);
            if format == 0 {
                return Err("WGL pixel format unavailable".into());
            }
            SetPixelFormat(surface.dc, format, &pfd)
                .map_err(|e| format!("WGL pixel format: {e}"))?;
            surface.context =
                wglCreateContext(surface.dc).map_err(|e| format!("WGL context: {e}"))?;
            wglMakeCurrent(surface.dc, surface.context)
                .map_err(|e| format!("WGL activation: {e}"))?;
            let gl = glow::Context::from_loader_function_cstr(|name: &CStr| {
                resolve(ptr::null_mut(), name.as_ptr()).cast_const()
            });
            let renderer = gl.get_parameter_string(glow::RENDERER);
            let lower = renderer.to_ascii_lowercase();
            if renderer.is_empty()
                || lower.contains("gdi generic")
                || lower.contains("llvmpipe")
                || lower.contains("software rasterizer")
            {
                return Err(format!("Software OpenGL renderer: {renderer}"));
            }
            Ok(Self {
                gl,
                target: None,
                surface,
            })
        }
    }
    pub(super) fn init() -> Init {
        Init {
            resolve,
            context: ptr::null_mut(),
        }
    }
    pub(super) fn prepare(&mut self, w: u32, h: u32) -> Result<Fbo, String> {
        unsafe {
            if self.target.as_ref().is_none_or(|t| t.2 != w || t.3 != h) {
                self.free_target();
                let texture = self
                    .gl
                    .create_texture()
                    .map_err(|e| format!("GL texture: {e}"))?;
                let fbo = match self.gl.create_framebuffer() {
                    Ok(f) => f,
                    Err(e) => {
                        self.gl.delete_texture(texture);
                        return Err(format!("GL framebuffer: {e}"));
                    }
                };
                self.target = Some((fbo, texture, w, h));
                self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                self.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w as i32,
                    h as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                self.gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(texture),
                    0,
                );
                if self.gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE
                {
                    return Err("GL framebuffer incomplete".into());
                }
            }
            let fbo = self.target.as_ref().unwrap().0;
            Ok(Fbo {
                fbo: fbo.0.get() as i32,
                width: w as i32,
                height: h as i32,
                format: glow::RGBA8 as i32,
            })
        }
    }
    pub(super) fn read(&self, w: u32, h: u32) -> Result<Vec<u8>, String> {
        unsafe {
            let mut data = vec![0; w as usize * h as usize * 4];
            let fbo = self.target.as_ref().ok_or("GL framebuffer missing")?.0;
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            self.gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut data)),
            );
            let error = self.gl.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("GL readback failed: 0x{error:04x}"));
            }
            for pixel in data.chunks_exact_mut(4) {
                pixel[3] = 255
            }
            Ok(data)
        }
    }
    unsafe fn free_target(&mut self) {
        if let Some((f, t, _, _)) = self.target.take() {
            unsafe {
                self.gl.delete_framebuffer(f);
                self.gl.delete_texture(t);
            }
        }
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe { self.free_target() };
        let _ = &self.surface;
    }
}

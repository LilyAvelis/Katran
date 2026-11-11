//! Experimental OpenGL background refraction. The host must call `destroy` on
//! the render thread while the corresponding GL context is current.
use eframe::{egui, egui_glow, glow};
use glow::HasContext;
use std::{cell::RefCell, collections::HashMap, num::NonZeroU32, sync::Arc};

struct Resources {
    program: glow::Program,
    vao: glow::VertexArray,
}
thread_local! {
    // GL objects are context-bound. A failed compilation is remembered as well.
    static CACHE: RefCell<HashMap<usize, Option<Resources>>> = RefCell::new(HashMap::new());
}

/// Release cached GL resources on the thread where `gl` is current. Must be
/// called for every context on shutdown before its GL context is destroyed.
pub(super) fn destroy(gl: &glow::Context) {
    let key = gl as *const glow::Context as usize;
    CACHE.with(|cache| {
        if let Some(Some(resources)) = cache.borrow_mut().remove(&key) {
            unsafe {
                gl.delete_vertex_array(resources.vao);
                gl.delete_program(resources.program);
            }
        }
    });
}

#[derive(Clone, Copy)]
pub(super) struct Material {
    pub adjustments: [f32; 4],
    pub selection_tint: Option<[f32; 4]>,
}

/// A callback shape that can be inserted below text with `Painter::set`.
/// The supplied texture and mapping must represent an application-owned background;
/// this does not capture or read the desktop or framebuffer.
pub(super) fn shape(
    rect: egui::Rect,
    source_texture: egui::TextureId,
    background_rect: egui::Rect,
    background_uv: egui::Rect,
    radius: f32,
    strength: f32,
    material: Material,
) -> egui::Shape {
    let Material {
        adjustments,
        selection_tint,
    } = material;
    if !rect.is_finite()
        || !background_rect.is_finite()
        || !background_uv.is_finite()
        || !rect.is_positive()
        || !background_rect.is_positive()
        || !background_uv.is_positive()
        || !radius.is_finite()
        || !strength.is_finite()
        || strength <= 0.0
    {
        return egui::Shape::Noop;
    }
    if adjustments
        .iter()
        .any(|x| !x.is_finite() || !(0.0..=2.0).contains(x))
    {
        return egui::Shape::Noop;
    }
    let selection_tint = selection_tint.unwrap_or([0.0; 4]);
    if selection_tint
        .iter()
        .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
    {
        return egui::Shape::Noop;
    }
    let callback = egui_glow::CallbackFn::new(move |info, painter| {
        let Some(texture) = painter.texture(source_texture) else {
            return;
        };
        let gl = painter.gl();
        let key = Arc::as_ptr(gl) as usize;
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let resources = cache
                .entry(key)
                .or_insert_with(|| match unsafe { create(gl) } {
                    Ok(resources) => Some(resources),
                    Err(error) => {
                        eprintln!("Optical glass unavailable: {error}");
                        None
                    }
                });
            let Some(resources) = resources else {
                return;
            };
            let viewport = info.viewport_in_pixels();
            if viewport.width_px <= 0 || viewport.height_px <= 0 {
                return;
            }
            unsafe {
                let old_program = gl.get_parameter_i32(glow::CURRENT_PROGRAM) as u32;
                let old_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) as u32;
                let old_active = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
                gl.active_texture(glow::TEXTURE0);
                let old_texture = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D) as u32;
                gl.use_program(Some(resources.program));
                gl.bind_vertex_array(Some(resources.vao));
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                let uniform = |name| gl.get_uniform_location(resources.program, name);
                gl.uniform_1_i32(uniform("image").as_ref(), 0);
                gl.uniform_4_f32(
                    uniform("viewport").as_ref(),
                    viewport.left_px as f32,
                    viewport.from_bottom_px as f32,
                    viewport.width_px as f32,
                    viewport.height_px as f32,
                );
                gl.uniform_4_f32(
                    uniform("lens").as_ref(),
                    rect.left(),
                    rect.top(),
                    rect.width(),
                    rect.height(),
                );
                gl.uniform_4_f32(
                    uniform("background").as_ref(),
                    background_rect.left(),
                    background_rect.top(),
                    background_rect.width(),
                    background_rect.height(),
                );
                gl.uniform_4_f32(
                    uniform("uv_rect").as_ref(),
                    background_uv.left(),
                    background_uv.top(),
                    background_uv.right(),
                    background_uv.bottom(),
                );
                gl.uniform_1_f32(
                    uniform("radius").as_ref(),
                    radius.max(0.0).min(rect.width().min(rect.height()) * 0.5),
                );
                gl.uniform_1_f32(uniform("strength").as_ref(), strength.clamp(0.0, 1.0));
                gl.uniform_4_f32(
                    uniform("optics").as_ref(),
                    adjustments[0],
                    adjustments[1],
                    adjustments[2],
                    adjustments[3],
                );
                gl.uniform_4_f32(
                    uniform("selection_tint").as_ref(),
                    selection_tint[0],
                    selection_tint[1],
                    selection_tint[2],
                    selection_tint[3],
                );
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                gl.bind_texture(
                    glow::TEXTURE_2D,
                    NonZeroU32::new(old_texture).map(glow::NativeTexture),
                );
                gl.active_texture(old_active);
                gl.bind_vertex_array(NonZeroU32::new(old_vao).map(glow::NativeVertexArray));
                gl.use_program(NonZeroU32::new(old_program).map(glow::NativeProgram));
            }
        });
    });
    egui::Shape::Callback(egui::PaintCallback {
        rect,
        callback: Arc::new(callback),
    })
}

unsafe fn create(gl: &glow::Context) -> Result<Resources, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        let result = (|| {
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX),
                (glow::FRAGMENT_SHADER, FRAGMENT),
            ] {
                let shader = gl.create_shader(kind)?;
                shaders.push(shader);
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    return Err(gl.get_shader_info_log(shader));
                }
                gl.attach_shader(program, shader);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            let vao = gl.create_vertex_array()?;
            Ok(Resources { program, vao })
        })();
        for shader in shaders {
            gl.delete_shader(shader);
        }
        if result.is_err() {
            gl.delete_program(program);
        }
        result
    }
}

const VERTEX: &str = r#"#version 330 core
void main() {
    vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}"#;

const FRAGMENT: &str = r#"#version 330 core
uniform sampler2D image;
uniform vec4 viewport, lens, background, uv_rect;
uniform float radius, strength;
uniform vec4 optics;
uniform vec4 selection_tint;
out vec4 color;
vec2 uv_at(vec2 p) {
    vec2 uv = uv_rect.xy + ((p-background.xy)/background.zw)*(uv_rect.zw-uv_rect.xy);
    return clamp(uv, uv_rect.xy, uv_rect.zw);
}
void main() {
    vec2 t = (gl_FragCoord.xy-viewport.xy)/viewport.zw;
    vec2 p = lens.xy + vec2(t.x,1.0-t.y)*lens.zw;
    vec2 delta = p-(lens.xy+lens.zw*0.5);
    vec2 q = abs(delta)-(lens.zw*0.5-vec2(radius));
    vec2 outside = max(q,0.0);
    float d = length(outside)+min(max(q.x,q.y),0.0)-radius;
    if (d >= 0.0) discard;
    vec2 normal;
    if (length(outside)>0.0001) normal = normalize(outside)*sign(delta);
    else if(q.x>q.y) normal = vec2(sign(delta.x),0.0);
    else normal = vec2(0.0,sign(delta.y));
    float edge = clamp(1.0+d/max(1.0,min(lens.z,lens.w)*0.12),0.0,1.0);
    float shift = strength*12.0*edge*edge*optics.x;
    vec2 sample_p = p-normal*shift;
    float blur = strength*1.25*optics.y;
    vec4 c = texture(image,uv_at(sample_p))*0.4;
    c += texture(image,uv_at(sample_p+vec2(blur,0)))*0.15;
    c += texture(image,uv_at(sample_p-vec2(blur,0)))*0.15;
    c += texture(image,uv_at(sample_p+vec2(0,blur)))*0.15;
    c += texture(image,uv_at(sample_p-vec2(0,blur)))*0.15;
    float chroma = shift*0.08*optics.w;
    c.r = texture(image,uv_at(sample_p-normal*chroma)).r;
    c.b = texture(image,uv_at(sample_p+normal*chroma)).b;
    float shine = strength*0.18*optics.z*pow(edge,4.0)*max(dot(normal,normalize(vec2(-1.0,-1.0))),0.0);
    c.rgb = min(mix(c.rgb,vec3(c.a),shine),vec3(c.a));
    float selected_weight = selection_tint.a * (0.30 + 0.20 * edge * edge);
    c.rgb = min(mix(c.rgb, selection_tint.rgb * c.a, selected_weight), vec3(c.a));
    color = c;
}"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_tint_is_optional_and_rejects_invalid_channels() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 80.0));
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        let render = |tint| {
            shape(
                rect,
                egui::TextureId::User(1),
                rect,
                uv,
                8.0,
                0.5,
                Material {
                    adjustments: [1.0; 4],
                    selection_tint: tint,
                },
            )
        };
        assert!(matches!(render(None), egui::Shape::Callback(_)));
        assert!(matches!(
            render(Some([0.1, 0.8, 0.2, 0.5])),
            egui::Shape::Callback(_)
        ));
        for invalid in [
            [f32::NAN, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 1.1],
            [-0.1, 0.0, 0.0, 0.0],
        ] {
            assert!(matches!(render(Some(invalid)), egui::Shape::Noop));
        }
    }
}

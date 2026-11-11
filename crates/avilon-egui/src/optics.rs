//! Small CPU reference lens for cached lab previews; not a desktop capture or live GPU shader.
use egui::{Color32, ColorImage};

/// Rect coordinates are normalized `[left, top, right, bottom]`.
/// Radius is a fraction of the lens's shorter side. Refraction and blur are output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensOptions {
    pub rect: [f32; 4],
    pub corner_radius: f32,
    pub refraction: f32,
    pub blur: f32,
    pub chromatic: f32,
    pub highlight: f32,
}
impl Default for LensOptions {
    fn default() -> Self {
        Self {
            rect: [0.1, 0.15, 0.9, 0.85],
            corner_radius: 0.18,
            refraction: 8.0,
            blur: 1.0,
            chromatic: 0.04,
            highlight: 0.2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LensError {
    InvalidSource,
    InvalidSize,
    InvalidOptions,
}
impl std::fmt::Display for LensError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSource => "Invalid source image",
            Self::InvalidSize => "Output dimensions must be 1..=512",
            Self::InvalidOptions => "Lens parameters are outside their valid ranges",
        })
    }
}
impl std::error::Error for LensError {}

/// Render an independently owned preview. Source and output coordinates may have different sizes;
/// outside the lens the output is the bilinearly resampled source. A zero-effect lens is identity
/// when sizes match. Channels are interpolated in egui's premultiplied representation.
pub fn render_lens(
    source: &ColorImage,
    output: [usize; 2],
    options: LensOptions,
) -> Result<ColorImage, LensError> {
    let [sw, sh] = source.size;
    if sw == 0 || sh == 0 || sw.checked_mul(sh) != Some(source.pixels.len()) {
        return Err(LensError::InvalidSource);
    }
    let [width, height] = output;
    if width == 0 || height == 0 || width > 512 || height > 512 {
        return Err(LensError::InvalidSize);
    }
    let [left, top, right, bottom] = options.rect;
    let in_range = |v: f32, max: f32| v.is_finite() && (0.0..=max).contains(&v);
    if !options.rect.iter().all(|&v| in_range(v, 1.0))
        || left >= right
        || top >= bottom
        || !in_range(options.corner_radius, 0.5)
        || !in_range(options.refraction, 64.0)
        || !in_range(options.blur, 8.0)
        || !in_range(options.chromatic, 1.0)
        || !in_range(options.highlight, 1.0)
    {
        return Err(LensError::InvalidOptions);
    }
    let w = width as f32;
    let h = height as f32;
    let center = [(left + right) * w * 0.5, (top + bottom) * h * 0.5];
    let half = [(right - left) * w * 0.5, (bottom - top) * h * 0.5];
    let radius = options.corner_radius * (2.0 * half[0]).min(2.0 * half[1]);
    let edge_width = (half[0].min(half[1]) * 0.3).clamp(1.0, 24.0);
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let base = sample(source, px, py, output);
            let dx = px - center[0];
            let dy = py - center[1];
            let qx = dx.abs() - (half[0] - radius);
            let qy = dy.abs() - (half[1] - radius);
            let ox = qx.max(0.0);
            let oy = qy.max(0.0);
            let distance = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - radius;
            if distance >= 0.0
                || (options.refraction == 0.0 && options.blur == 0.0 && options.chromatic == 0.0)
            {
                pixels.push(to_color(base));
                continue;
            }
            let normal = if ox > 0.0 || oy > 0.0 {
                let len = (ox * ox + oy * oy).sqrt().max(f32::EPSILON);
                [ox / len * dx.signum(), oy / len * dy.signum()]
            } else if qx > qy {
                [dx.signum(), 0.0]
            } else {
                [0.0, dy.signum()]
            };
            let edge = (1.0 + distance / edge_width).clamp(0.0, 1.0);
            let shift = options.refraction * edge * edge;
            let center_sample = [px - normal[0] * shift, py - normal[1] * shift];
            let offsets = if options.blur > 0.0 {
                [-options.blur, 0.0, options.blur]
            } else {
                [0.0; 3]
            };
            let mut result = [0.0; 4];
            for oy in offsets {
                for ox in offsets {
                    let position = [center_sample[0] + ox, center_sample[1] + oy];
                    let color = sample(source, position[0], position[1], output);
                    let displacement = shift * options.chromatic;
                    let red = sample(
                        source,
                        position[0] - normal[0] * displacement,
                        position[1] - normal[1] * displacement,
                        output,
                    );
                    let blue = sample(
                        source,
                        position[0] + normal[0] * displacement,
                        position[1] + normal[1] * displacement,
                        output,
                    );
                    result[0] += red[0] / 9.0;
                    result[1] += color[1] / 9.0;
                    result[2] += blue[2] / 9.0;
                    result[3] += color[3] / 9.0;
                }
            }
            let shine = options.highlight
                * edge.powi(4)
                * ((-normal[0] - normal[1]) * 0.707_106_77).max(0.0)
                * (options.refraction / 8.0).min(1.0);
            for channel in 0..3 {
                result[channel] += (result[3] - result[channel]).max(0.0) * shine;
                result[channel] = result[channel].min(result[3]);
            }
            pixels.push(to_color(result));
        }
    }
    Ok(ColorImage::new(output, pixels))
}

fn sample(source: &ColorImage, x: f32, y: f32, output: [usize; 2]) -> [f32; 4] {
    let [w, h] = source.size;
    let sx = (x * w as f32 / output[0] as f32 - 0.5).clamp(0.0, (w - 1) as f32);
    let sy = (y * h as f32 / output[1] as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let x0 = sx.floor() as usize;
    let y0 = sy.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = sx - x0 as f32;
    let fy = sy - y0 as f32;
    let mut result = [0.0; 4];
    for (index, weight) in [
        (y0 * w + x0, (1.0 - fx) * (1.0 - fy)),
        (y0 * w + x1, fx * (1.0 - fy)),
        (y1 * w + x0, (1.0 - fx) * fy),
        (y1 * w + x1, fx * fy),
    ] {
        for (channel, value) in source.pixels[index].to_array().into_iter().enumerate() {
            result[channel] += value as f32 * weight;
        }
    }
    result
}
fn to_color(channels: [f32; 4]) -> Color32 {
    let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(
        byte(channels[0]),
        byte(channels[1]),
        byte(channels[2]),
        byte(channels[3]),
    )
}

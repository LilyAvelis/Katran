//! Deterministic bounded decorative materials; the host owns colors and policy.
use egui::{self, Color32, Painter, Pos2, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Solid,
    Dots,
    Lines,
    Grain,
}
/// Scale 1..=100; intensity zero suppresses marks, and the host supplies ink alpha.
/// Angle is in degrees; seed remains stable across frames.
#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    pub pattern: Pattern,
    pub scale: u8,
    pub intensity: u8,
    pub angle: u16,
    pub seed: u32,
}

// Bound work even on unusually large panes; patterns are anchored to the local
// rectangle and seeded only by persisted slot parameters.
pub const MAX_MARKS: usize = 512;
#[derive(Clone, Debug, PartialEq)]
enum Mark {
    Dot(Pos2, f32),
    Line(Pos2, Pos2),
}
fn hash(seed: u32, x: u32, y: u32) -> u32 {
    let mut n = seed ^ x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^ (n >> 16)
}
fn marks(rect: Rect, slot: &Parameters) -> Vec<Mark> {
    if !rect.is_positive()
        || !rect.is_finite()
        || slot.intensity == 0
        || slot.pattern == Pattern::Solid
    {
        return Vec::new();
    }
    let scale = f32::from(slot.scale.clamp(1, 100) - 1) / 99.0;
    let mut result = Vec::new();
    if slot.pattern == Pattern::Lines {
        let angle = f32::from(slot.angle % 360).to_radians();
        let direction = egui::vec2(angle.cos(), angle.sin());
        let normal = egui::vec2(-direction.y, direction.x);
        let diagonal = rect.size().length();
        let spacing = (4.0 + 12.0 * scale).max(2.0 * diagonal / MAX_MARKS as f32);
        let mut offset = -diagonal + (slot.seed % 97) as f32 / 97.0 * spacing;
        while offset <= diagonal && result.len() < MAX_MARKS {
            let center = rect.center() + normal * offset;
            result.push(Mark::Line(
                center - direction * diagonal,
                center + direction * diagonal,
            ));
            offset += spacing;
        }
    } else {
        let nominal = if slot.pattern == Pattern::Dots {
            3.0 + 7.0 * scale
        } else {
            3.5 + 5.0 * scale
        };
        // Extra row and column are included in the budget calculation.
        let spacing = nominal
            .max(((rect.width() + nominal) * (rect.height() + nominal) / MAX_MARKS as f32).sqrt());
        let x_offset = (slot.seed % 97) as f32 / 97.0 * spacing;
        let y_offset = (slot.seed.wrapping_mul(31) % 97) as f32 / 97.0 * spacing;
        let mut y = rect.top() + y_offset;
        let mut row = 0u32;
        while y < rect.bottom() && result.len() < MAX_MARKS {
            let mut x = rect.left() + x_offset;
            let mut col = 0u32;
            while x < rect.right() && result.len() < MAX_MARKS {
                if slot.pattern == Pattern::Grain {
                    let h = hash(slot.seed, col, row);
                    let jitter_x = (h & 255) as f32 / 255.0 * spacing * 0.8;
                    let jitter_y = ((h >> 8) & 255) as f32 / 255.0 * spacing * 0.8;
                    result.push(Mark::Dot(
                        Pos2::new(x + jitter_x, y + jitter_y),
                        if h & 0x10000 == 0 { 0.55 } else { 0.9 },
                    ));
                } else {
                    result.push(Mark::Dot(Pos2::new(x, y), 1.25));
                }
                x += spacing;
                col += 1;
            }
            y += spacing;
            row += 1;
        }
    }
    result
}

/// Draw the supplied fill and ink within the intersection of region and host clip.
/// The host controls contrast, opacity, radius and selection layering.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    params: &Parameters,
    fill: Color32,
    ink: Color32,
    radius: f32,
) {
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(rect));
    painter.rect_filled(rect, radius, fill);
    for mark in marks(rect, params) {
        match mark {
            Mark::Dot(pos, radius) => {
                painter.circle_filled(pos, radius, ink);
            }
            Mark::Line(start, end) => {
                painter.line_segment([start, end], egui::Stroke::new(1.25, ink));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patterns_are_deterministic_distinct_and_bounded() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(3000.0, 2000.0));
        let mut p = Parameters {
            pattern: Pattern::Dots,
            scale: 1,
            intensity: 100,
            angle: 0,
            seed: 42,
        };
        for pattern in [Pattern::Dots, Pattern::Lines, Pattern::Grain] {
            p.pattern = pattern;
            let first = marks(rect, &p);
            assert!(!first.is_empty() && first.len() <= MAX_MARKS);
            assert_eq!(first, marks(rect, &p));
        }
        p.pattern = Pattern::Dots;
        let dots = marks(rect, &p);
        p.pattern = Pattern::Grain;
        assert_ne!(dots, marks(rect, &p));
        p.pattern = Pattern::Lines;
        let lines = marks(rect, &p);
        p.angle = 90;
        assert_ne!(lines, marks(rect, &p));
        p.intensity = 0;
        assert!(marks(rect, &p).is_empty());
    }
    #[test]
    fn respects_existing_painter_clip() {
        let ctx = egui::Context::default();
        let clip = Rect::from_min_size(Pos2::ZERO, egui::vec2(40.0, 40.0));
        let region = Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(40.0, 40.0));
        let p = Parameters {
            pattern: Pattern::Dots,
            scale: 50,
            intensity: 50,
            angle: 0,
            seed: 0,
        };
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint(
                &ui.painter().with_clip_rect(clip),
                region,
                &p,
                Color32::GRAY,
                Color32::WHITE,
                6.0,
            )
        });
        out.textures_delta.clear();
        assert!(!out.shapes.is_empty());
        assert!(
            out.shapes
                .iter()
                .all(|shape| clip.contains_rect(shape.clip_rect))
        );
    }
}

use eframe::egui::{self, Color32, Pos2, Rect};
use katran::tag_slots::{Material, Slot};

pub fn color(slot: &Slot) -> Color32 {
    let s = slot.color.as_bytes();
    if s.len() != 7 || s[0] != b'#' {
        return Color32::GRAY;
    }
    let parse = |range: std::ops::Range<usize>| {
        std::str::from_utf8(&s[range])
            .ok()
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .unwrap_or(128)
    };
    Color32::from_rgb(parse(1..3), parse(3..5), parse(5..7))
}

/// Pick a title color against the tag tint over a typical dark card surface.
/// Final contrast still depends on the theme and any optical background.
pub fn title_color(slot: &Slot) -> Color32 {
    let base = color(slot);
    let alpha = f32::from(slot.opacity.min(100)) / 100.0;
    let component = |value: u8| {
        let srgb = (f32::from(value) * alpha + 60.0 * (1.0 - alpha)) / 255.0;
        if srgb <= 0.04045 {
            srgb / 12.92
        } else {
            ((srgb + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance =
        0.2126 * component(base.r()) + 0.7152 * component(base.g()) + 0.0722 * component(base.b());
    if luminance > 0.18 {
        Color32::from_gray(20)
    } else {
        Color32::from_gray(235)
    }
}

// Bound work even on unusually large panes; patterns are anchored to the local
// rectangle and seeded only by persisted slot parameters.
const MAX_MARKS: usize = 512;
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
fn marks(rect: Rect, slot: &Slot) -> Vec<Mark> {
    if !rect.is_positive()
        || !rect.is_finite()
        || slot.intensity == 0
        || slot.material == Material::Solid
    {
        return Vec::new();
    }
    let scale = f32::from(slot.scale.clamp(1, 100) - 1) / 99.0;
    let mut result = Vec::new();
    if slot.material == Material::Lines {
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
        let nominal = if slot.material == Material::Dots {
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
                if slot.material == Material::Grain {
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

/// Paint only within a host-provided decoration region, before content.
pub fn paint(ui: &egui::Ui, rect: Rect, slot: &Slot) {
    let painter = ui.painter().with_clip_rect(rect);
    let base = color(slot);
    let alpha = (u16::from(slot.opacity.min(100)) * 255 / 100) as u8;
    painter.rect_filled(
        rect,
        6.0,
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha),
    );
    let luminance =
        u32::from(base.r()) * 299 + u32::from(base.g()) * 587 + u32::from(base.b()) * 114;
    let value = if luminance >= 140_000 { 0 } else { 255 };
    let strength = (u16::from(slot.intensity.min(100)) * 170 / 100) as u8;
    let ink = Color32::from_rgba_unmultiplied(value, value, value, strength);
    for mark in marks(rect, slot) {
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

#[derive(Default)]
pub struct Cache {
    key: Option<(Vec<i64>, u64)>,
    pub slots: Vec<Slot>,
    pub assignments: std::collections::HashMap<i64, u8>,
}
impl super::Shelf {
    pub(super) fn refresh_tag_materials(&mut self) {
        if self.api.is_none() {
            return;
        }
        let ids: Vec<i64> = self
            .events
            .iter()
            .filter_map(|e| e["id"].as_i64())
            .filter(|id| *id > 0)
            .collect();
        let generation = self.state["view_generation"].as_u64().unwrap_or(0);
        let key = (ids.clone(), generation);
        if self.tag_material_cache.key.as_ref() == Some(&key) {
            return;
        }
        self.tag_material_cache.key = Some(key);
        self.tag_material_cache.assignments.clear();
        let slots = self.call(katran::Command::ListTagSlots);
        self.tag_material_cache.slots =
            serde_json::from_value(slots["slots"].clone()).unwrap_or_default();
        for chunk in ids.chunks(1000) {
            let response = self.call(katran::Command::GetTagSlots {
                ids: chunk.to_vec(),
            });
            if let Some(items) = response["items"].as_array() {
                for item in items {
                    if let (Some(id), Some(slot)) = (
                        item["id"].as_i64(),
                        item["slot"].as_u64().and_then(|n| u8::try_from(n).ok()),
                    ) {
                        self.tag_material_cache.assignments.insert(id, slot);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn color_conversion_and_dots_are_bounded_for_valid_slots() {
        let slot = Slot {
            number: 1,
            name: "Тест".into(),
            color: "#12A0FF".into(),
            opacity: 55,
            material: Material::Dots,
            scale: 1,
            intensity: 100,
            angle: 0,
            seed: 42,
        };
        assert_eq!(color(&slot), Color32::from_rgb(18, 160, 255));
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(80.0, 30.0));
            paint(ui, rect, &slot);
        });
        assert!(output.shapes.len() <= MAX_MARKS + 1);
        output.textures_delta.clear();
    }
    #[test]
    fn procedural_patterns_are_distinct_stable_and_bounded() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(3000.0, 2000.0));
        let mut slot = Slot {
            number: 1,
            name: "test".into(),
            color: "#123456".into(),
            opacity: 55,
            material: Material::Dots,
            scale: 1,
            intensity: 100,
            angle: 0,
            seed: 42,
        };
        for material in [Material::Dots, Material::Lines, Material::Grain] {
            slot.material = material;
            let first = marks(rect, &slot);
            assert!(!first.is_empty());
            assert!(first.len() <= MAX_MARKS);
            assert_eq!(first, marks(rect, &slot));
        }
        slot.material = Material::Dots;
        let dots = marks(rect, &slot);
        slot.material = Material::Grain;
        assert_ne!(dots, marks(rect, &slot));
        slot.material = Material::Lines;
        let lines = marks(rect, &slot);
        slot.angle = 90;
        assert_ne!(lines, marks(rect, &slot));
        slot.intensity = 0;
        assert!(marks(rect, &slot).is_empty());
        slot.opacity = 0;
        slot.intensity = 100;
        assert!(!marks(rect, &slot).is_empty());
    }
    #[test]
    fn title_color_respects_color_and_opacity() {
        let mut slot = Slot {
            number: 1,
            name: "test".into(),
            color: "#FFFFFF".into(),
            opacity: 100,
            material: Material::Solid,
            scale: 50,
            intensity: 50,
            angle: 0,
            seed: 0,
        };
        assert_eq!(title_color(&slot), Color32::from_gray(20));
        slot.opacity = 0;
        assert_eq!(title_color(&slot), Color32::from_gray(235));
        slot.color = "#000000".into();
        slot.opacity = 100;
        assert_eq!(title_color(&slot), Color32::from_gray(235));
    }
}

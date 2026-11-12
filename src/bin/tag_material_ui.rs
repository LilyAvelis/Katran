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

/// Paint only within a host-provided decoration region, before content.
pub fn paint(ui: &egui::Ui, rect: Rect, slot: &Slot) {
    let painter = ui.painter().with_clip_rect(rect);
    let base = color(slot);
    let tint = ((slot.intensity.min(100) as f32 / 100.0) * 65.0) as u8;
    painter.rect_filled(
        rect,
        6.0,
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), tint),
    );
    if slot.material != Material::Dots || tint == 0 {
        return;
    }
    let spacing = 6.0 + (slot.scale as f32 - 1.0) * 18.0 / 99.0;
    let x_offset = (slot.seed % 97) as f32 / 97.0 * spacing;
    let y_offset = (slot.seed.wrapping_mul(31) % 97) as f32 / 97.0 * spacing;
    let dot = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), tint);
    let mut y = rect.top() + y_offset;
    while y < rect.bottom() {
        let mut x = rect.left() + x_offset;
        while x < rect.right() {
            painter.circle_filled(Pos2::new(x, y), 1.1, dot);
            x += spacing;
        }
        y += spacing;
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
        assert!(output.shapes.len() < 200);
        output.textures_delta.clear();
    }
}

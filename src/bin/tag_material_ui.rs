#[cfg(test)]
use eframe::egui::Pos2;
use eframe::egui::{self, Color32, Rect};
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

/// Paint only within a host-provided decoration region, before content.
pub fn paint(ui: &egui::Ui, rect: Rect, slot: &Slot) {
    let base = color(slot);
    let alpha = (u16::from(slot.opacity.min(100)) * 255 / 100) as u8;
    let fill = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
    let luminance =
        u32::from(base.r()) * 299 + u32::from(base.g()) * 587 + u32::from(base.b()) * 114;
    let value = if luminance >= 140_000 { 0 } else { 255 };
    let strength = (u16::from(slot.intensity.min(100)) * 170 / 100) as u8;
    let ink = Color32::from_rgba_unmultiplied(value, value, value, strength);
    let pattern = match slot.material {
        Material::Solid => avilon_egui::material::Pattern::Solid,
        Material::Dots => avilon_egui::material::Pattern::Dots,
        Material::Lines => avilon_egui::material::Pattern::Lines,
        Material::Grain => avilon_egui::material::Pattern::Grain,
    };
    let params = avilon_egui::material::Parameters {
        pattern,
        scale: slot.scale,
        intensity: slot.intensity,
        angle: slot.angle,
        seed: slot.seed,
    };
    avilon_egui::material::paint(ui.painter(), rect, &params, fill, ink, 6.0);
}

#[derive(Default)]
pub struct Cache {
    key: Option<(Vec<i64>, u64)>,
    pub slots: Vec<Slot>,
    pub assignments: std::collections::HashMap<i64, u8>,
}
impl super::Shelf {
    pub(super) fn seed_preloaded_tags(&mut self, page: &serde_json::Value) {
        let ids: Vec<i64> = self
            .events
            .iter()
            .filter_map(|e| e["id"].as_i64())
            .filter(|id| *id > 0)
            .collect();
        let generation = self.state["view_generation"].as_u64().unwrap_or(0);
        self.tag_material_cache.key = Some((ids, generation));
        self.tag_material_cache.slots =
            serde_json::from_value(page["slots"].clone()).unwrap_or_default();
        self.tag_material_cache.assignments = page["assignments"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                Some((
                    item["id"].as_i64()?,
                    u8::try_from(item["slot"].as_u64()?).ok()?,
                ))
            })
            .collect();
    }

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
        assert!(output.shapes.len() <= avilon_egui::material::MAX_MARKS + 1);
        output.textures_delta.clear();
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

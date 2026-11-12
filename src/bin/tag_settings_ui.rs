//! Slot editor is a client of the tag commands; drafts are never persisted implicitly.
use super::ui_foundation::{SettingsRow, SettingsSection};
use eframe::egui::{self, Color32};
use katran::{
    Command, Request, Service,
    tag_slots::{Material, Slot},
};

fn default_texture_strength(slot: &mut Slot, previous: Material) {
    if previous == Material::Solid && slot.material != Material::Solid && slot.intensity == 0 {
        slot.intensity = 60;
    }
}

#[derive(Default)]
pub struct State {
    slots: Option<Vec<Slot>>,
    draft: Option<Slot>,
    error: String,
    preview_selected: bool,
    preview_cursor: bool,
}
impl State {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn show(&mut self, ui: &mut egui::Ui, api: &mut Service) -> bool {
        let mut changed = false;
        if self.slots.is_none() {
            let response = api.execute(Request {
                api_version: 1,
                request_id: "tag-settings-list".into(),
                command: Command::ListTagSlots,
            });
            if response["ok"] == true {
                self.slots = serde_json::from_value(response["result"]["slots"].clone()).ok();
                if self.slots.is_none() {
                    self.error = "Не удалось прочитать слоты тегов".into();
                }
            } else {
                self.error = response["error"]["message"]
                    .as_str()
                    .unwrap_or("Не удалось загрузить теги")
                    .into();
            }
        }
        ui.heading("Теги");
        ui.weak("Теги назначаются только закреплённым объектам.");
        ui.add_space(12.0);
        if let Some(slots) = &self.slots {
            let mut edit = None;
            SettingsSection::show(ui, |ui| {
                for slot in slots {
                    ui.push_id(slot.number, |ui| {
                        SettingsRow::show(
                            ui,
                            &format!("{} · {}", slot.number, slot.name),
                            "",
                            |ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(22.0, 22.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    4.0,
                                    super::tag_material_ui::color(slot),
                                );
                                if ui.button("Изменить").clicked() {
                                    edit = Some(slot.clone());
                                }
                            },
                        );
                    });
                }
            });
            if let Some(slot) = edit {
                self.draft = Some(slot);
                self.error.clear();
            }
        }
        let mut save = false;
        let mut cancel = false;
        if let Some(slot) = self.draft.as_mut() {
            ui.add_space(14.0);
            ui.heading(format!("Тег {}", slot.number));
            ui.label("Название");
            ui.text_edit_singleline(&mut slot.name);
            let c = super::tag_material_ui::color(slot);
            let mut rgb = [c.r(), c.g(), c.b()];
            ui.horizontal(|ui| {
                ui.label("Цвет");
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    slot.color = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                }
            });
            let previous_material = slot.material;
            egui::ComboBox::from_id_salt("tag-material")
                .selected_text(match slot.material {
                    Material::Solid => "Цвет",
                    Material::Dots => "Точки",
                    Material::Lines => "Линии",
                    Material::Grain => "Зерно",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut slot.material, Material::Solid, "Цвет");
                    ui.selectable_value(&mut slot.material, Material::Dots, "Точки");
                    ui.selectable_value(&mut slot.material, Material::Lines, "Линии");
                    ui.selectable_value(&mut slot.material, Material::Grain, "Зерно");
                });
            default_texture_strength(slot, previous_material);
            ui.add(egui::Slider::new(&mut slot.opacity, 0..=100).text("Непрозрачность цвета"));
            if slot.material != Material::Solid {
                ui.add(egui::Slider::new(&mut slot.intensity, 0..=100).text("Сила фактуры"));
                ui.add(egui::Slider::new(&mut slot.scale, 1..=100).text("Масштаб"));
                if slot.material == Material::Lines {
                    ui.add(egui::Slider::new(&mut slot.angle, 0..=359).text("Угол"));
                }
            }
            ui.label("Предпросмотр материала");
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.preview_selected, "Выбрано");
                ui.checkbox(&mut self.preview_cursor, "Курсор");
            });
            for image in [false, true] {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().min(300.0), 72.0),
                    egui::Sense::hover(),
                );
                ui.painter()
                    .rect_filled(rect, 6.0, ui.visuals().widgets.inactive.weak_bg_fill);
                let content =
                    egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + 22.0));
                super::tag_material_ui::paint(ui, content, slot);
                ui.painter().text(
                    content.center(),
                    egui::Align2::CENTER_CENTER,
                    if image {
                        "Пример изображения"
                    } else {
                        "Пример текста"
                    },
                    egui::FontId::proportional(14.0),
                    super::tag_material_ui::title_color(slot),
                );
                if image {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            rect.left_bottom() - egui::vec2(-10.0, 40.0),
                            rect.right_bottom() - egui::vec2(10.0, 8.0),
                        ),
                        3.0,
                        Color32::from_gray(110),
                    );
                }
                if let Some(stroke) =
                    super::shelf_view::card_outline(ui.ctx(), self.preview_selected)
                {
                    ui.painter()
                        .rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
                }
                if self.preview_cursor {
                    super::shelf_view::paint_cursor(ui, rect);
                }
                ui.add_space(4.0);
            }
            ui.weak("Оптическое стекло в предпросмотре недоступно; проверьте его в основном окне.");
            ui.horizontal(|ui| {
                save = ui.button("Применить").clicked();
                cancel = ui.button("Отмена").clicked();
            });
        }
        if cancel {
            self.draft = None;
            self.error.clear();
        }
        if save && let Some(slot) = self.draft.clone() {
            let response = api.execute(Request {
                api_version: 1,
                request_id: "tag-settings-save".into(),
                command: Command::SaveTagSlot { slot },
            });
            if response["ok"] == true {
                self.slots = None;
                self.draft = None;
                self.error.clear();
                changed = true;
            } else {
                self.error = response["error"]["message"]
                    .as_str()
                    .unwrap_or("Не удалось сохранить тег")
                    .into();
            }
        }
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_procedural_choice_has_visible_draft_without_changing_existing_strength() {
        let saved = Slot {
            number: 1,
            name: "Test".into(),
            color: "#112233".into(),
            opacity: 55,
            material: Material::Solid,
            scale: 50,
            intensity: 0,
            angle: 0,
            seed: 0,
        };
        let mut draft = saved.clone();
        draft.material = Material::Dots;
        default_texture_strength(&mut draft, saved.material);
        assert_eq!(draft.intensity, 60);
        assert_eq!(saved.intensity, 0);
        draft.intensity = 0;
        draft.material = Material::Lines;
        default_texture_strength(&mut draft, Material::Dots);
        assert_eq!(draft.intensity, 0);
        draft.material = Material::Solid;
        default_texture_strength(&mut draft, Material::Lines);
        assert_eq!(draft.intensity, 0);
        draft.intensity = 35;
        draft.material = Material::Grain;
        default_texture_strength(&mut draft, Material::Solid);
        assert_eq!(draft.intensity, 35);
    }
}

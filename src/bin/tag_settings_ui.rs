//! Live slot editor is a client of the scoped tag commands.
use super::i18n::tr;
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

#[derive(Clone)]
struct TagDrag {
    tab: katran::interaction::ShelfTab,
    source: u8,
    revision: String,
}

fn move_order(source: u8, target: u8) -> Option<Vec<u8>> {
    if !(1..=9).contains(&source) || !(1..=9).contains(&target) || source == target {
        return None;
    }
    let mut order: Vec<u8> = (1..=9).collect();
    let item = order.remove(usize::from(source - 1));
    order.insert(usize::from(target - 1), item);
    Some(order)
}

pub(super) fn edit_controls(ui: &mut egui::Ui, slot: &mut Slot) {
    ui.label(tr("Название", "Name"));
    ui.text_edit_singleline(&mut slot.name);
    let c = super::tag_material_ui::color(slot);
    let mut rgb = [c.r(), c.g(), c.b()];
    ui.horizontal(|ui| {
        ui.label(tr("Цвет", "Color"));
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            slot.color = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
        }
    });
    let previous_material = slot.material;
    egui::ComboBox::from_id_salt("tag-material")
        .selected_text(match slot.material {
            Material::Solid => tr("Цвет", "Color"),
            Material::Dots => tr("Точки", "Dots"),
            Material::Lines => tr("Линии", "Lines"),
            Material::Grain => tr("Зерно", "Grain"),
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut slot.material, Material::Solid, tr("Цвет", "Color"));
            ui.selectable_value(&mut slot.material, Material::Dots, tr("Точки", "Dots"));
            ui.selectable_value(&mut slot.material, Material::Lines, tr("Линии", "Lines"));
            ui.selectable_value(&mut slot.material, Material::Grain, tr("Зерно", "Grain"));
        });
    default_texture_strength(slot, previous_material);
    ui.add(
        egui::Slider::new(&mut slot.opacity, 0..=100)
            .text(tr("Непрозрачность цвета", "Color opacity")),
    );
    if slot.material != Material::Solid {
        ui.add(
            egui::Slider::new(&mut slot.intensity, 0..=100)
                .text(tr("Сила фактуры", "Texture intensity")),
        );
        ui.add(egui::Slider::new(&mut slot.scale, 1..=100).text(tr("Масштаб", "Scale")));
        if slot.material == Material::Lines {
            ui.add(egui::Slider::new(&mut slot.angle, 0..=359).text(tr("Угол", "Angle")));
        }
    }
}

#[derive(Default)]
pub struct State {
    slots: Option<Vec<Slot>>,
    draft: Option<Slot>,
    error: String,
    preview_selected: bool,
    preview_cursor: bool,
    tab: Option<katran::interaction::ShelfTab>,
    dirty: bool,
    revision: Option<String>,
    conflicted: bool,
}
impl State {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn flush(&mut self, api: &mut Service) -> bool {
        if !self.dirty {
            return true;
        }
        let (Some(tab), Some(slot), Some(revision)) =
            (self.tab, self.draft.clone(), self.revision.clone())
        else {
            return false;
        };
        if self.conflicted {
            return false;
        }
        let response = api.execute(Request {
            api_version: 1,
            request_id: "tag-settings-save".into(),
            command: Command::SaveTagSlotChecked {
                tab,
                slot: slot.clone(),
                expected_revision: revision,
            },
        });
        if response["ok"] == true {
            if let Some(slots) = self.slots.as_mut()
                && let Some(old) = slots.iter_mut().find(|old| old.number == slot.number)
            {
                *old = slot;
            }
            self.dirty = false;
            self.revision = response["result"]["revision"].as_str().map(str::to_owned);
            self.error.clear();
            true
        } else {
            self.conflicted = response["error"]["code"] == "tag_slots_changed";
            self.error = response["error"]["message"]
                .as_str()
                .unwrap_or(tr("Не удалось сохранить тег", "Could not save tag"))
                .into();
            false
        }
    }
    fn load_slots(&mut self, api: &mut Service) -> bool {
        let Some(tab) = self.tab else {
            return false;
        };
        let response = api.execute(Request {
            api_version: 1,
            request_id: "tag-settings-list".into(),
            command: Command::ListTagSlotsForTab { tab },
        });
        if response["ok"] == true
            && let (Ok(slots), Some(revision)) = (
                serde_json::from_value::<Vec<Slot>>(response["result"]["slots"].clone()),
                response["result"]["revision"].as_str(),
            )
        {
            self.slots = Some(slots);
            self.revision = Some(revision.to_owned());
            true
        } else {
            self.error = response["error"]["message"]
                .as_str()
                .unwrap_or(tr("Не удалось загрузить теги", "Could not load tags"))
                .into();
            false
        }
    }

    fn reorder(&mut self, api: &mut Service, drag: &TagDrag, target: u8) -> bool {
        let Some(order) = move_order(drag.source, target) else {
            return false;
        };
        if self.tab != Some(drag.tab) || !self.flush(api) {
            return false;
        }
        let response = api.execute(Request {
            api_version: 1,
            request_id: "tag-settings-reorder".into(),
            command: Command::ReorderTagSlots {
                tab: drag.tab,
                order: order.clone(),
                expected_revision: drag.revision.clone(),
            },
        });
        if response["ok"] == true {
            let Some(revision) = response["result"]["revision"].as_str() else {
                self.slots = None;
                self.revision = None;
                self.draft = None;
                self.error = tr(
                    "Тег перемещён; не удалось прочитать список",
                    "Tag moved; could not read the list",
                )
                .into();
                return true;
            };
            let Ok(slots) =
                serde_json::from_value::<Vec<Slot>>(response["result"]["slots"].clone())
            else {
                self.slots = None;
                self.revision = None;
                self.draft = None;
                self.error = tr(
                    "Тег перемещён; не удалось прочитать список",
                    "Tag moved; could not read the list",
                )
                .into();
                return true;
            };
            if let Some(draft) = self.draft.as_mut()
                && let Some(position) = order.iter().position(|number| *number == draft.number)
            {
                draft.number = (position + 1) as u8;
            }
            self.slots = Some(slots);
            self.revision = Some(revision.to_owned());
            self.error.clear();
            true
        } else {
            let stale = response["error"]["code"] == "tag_slots_changed";
            let error = response["error"]["message"]
                .as_str()
                .unwrap_or(tr("Не удалось переместить тег", "Could not move tag"))
                .to_owned();
            if stale {
                self.draft = None;
                self.slots = None;
                self.revision = None;
                self.load_slots(api);
            }
            self.error = error;
            false
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, api: &mut Service) -> bool {
        let mut changed = false;
        if self.tab.is_none() {
            let interaction = api.execute(Request {
                api_version: 1,
                request_id: "tag-settings-context".into(),
                command: Command::Interaction,
            });
            let current: katran::interaction::ShelfTab =
                serde_json::from_value(interaction["result"]["tab"].clone()).unwrap_or_default();
            self.tab = Some(if current == katran::interaction::ShelfTab::All {
                katran::interaction::ShelfTab::Images
            } else {
                current
            });
        }
        let tabs = api.execute(Request {
            api_version: 1,
            request_id: "tag-settings-tabs".into(),
            command: Command::Tabs,
        });
        let mut next_tab = self.tab.unwrap_or_default();
        ui.label(tr("Теги вкладки", "Tags for tab"));
        egui::ComboBox::from_id_salt("tag-settings-tab")
            .selected_text(super::settings_ui::localized_shelf_tab(next_tab))
            .show_ui(ui, |ui| {
                if let Some(tabs) = tabs["result"]["tabs"].as_array() {
                    for entry in tabs {
                        if let Ok(tab) = serde_json::from_value::<katran::interaction::ShelfTab>(
                            entry["tab"].clone(),
                        ) && tab != katran::interaction::ShelfTab::All
                        {
                            ui.selectable_value(
                                &mut next_tab,
                                tab,
                                super::settings_ui::localized_shelf_tab(tab),
                            );
                        }
                    }
                }
            });
        if Some(next_tab) != self.tab {
            let was_dirty = self.dirty;
            if self.flush(api) {
                changed |= was_dirty;
                self.tab = Some(next_tab);
                self.slots = None;
                self.revision = None;
                self.draft = None;
            }
        }
        ui.weak(tr(
            "Настройка набора и оформления. Назначение объектам — на полке.",
            "Configure the set and appearance. Assign tags on the shelf.",
        ));
        if self.slots.is_none() {
            self.load_slots(api);
        }
        if self.conflicted {
            ui.colored_label(
                Color32::LIGHT_RED,
                tr(
                    "Теги изменились в другом окне. Черновик не сохранён.",
                    "Tags changed elsewhere. The draft was not saved.",
                ),
            );
            if ui
                .button(tr(
                    "Отменить черновик и обновить",
                    "Discard draft and reload",
                ))
                .clicked()
            {
                self.draft = None;
                self.dirty = false;
                self.conflicted = false;
                self.slots = None;
                self.revision = None;
                self.error.clear();
                self.load_slots(api);
            }
        }
        ui.heading(tr("Теги", "Tags"));
        ui.weak(tr(
            "Теги назначаются только закреплённым объектам.",
            "Tags can only be assigned to pinned items.",
        ));
        ui.add_space(12.0);
        let slots = self.slots.clone().unwrap_or_default();
        let mut clicked = None;
        let mut draft_changed = false;
        let mut drop = None;
        SettingsSection::show(ui, |ui| {
            for row in &slots {
                ui.push_id((format!("{:?}", self.tab), row.number), |ui| {
                    let expanded = self
                        .draft
                        .as_ref()
                        .is_some_and(|slot| slot.number == row.number);
                    let geometry = SettingsRow::show(
                        ui,
                        &format!("{} · {}", row.number, row.name),
                        "",
                        |ui| {
                            let grip = ui
                                .add(egui::Button::new("⠿").sense(egui::Sense::drag()))
                                .on_hover_text(tr("Перетащить тег", "Drag tag"));
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::new(("tag-grip", row.number)), grip.rect)
                            });
                            if grip.drag_started() && self.flush(api) {
                                let previous = self.slots.clone();
                                if self.load_slots(api)
                                    && previous == self.slots
                                    && let (Some(tab), Some(revision)) =
                                        (self.tab, self.revision.clone())
                                {
                                    grip.dnd_set_drag_payload(TagDrag {
                                        tab,
                                        source: row.number,
                                        revision,
                                    });
                                } else {
                                    if previous != self.slots {
                                        self.draft = None;
                                    }
                                    self.error = tr(
                                        "Теги изменились; перетащите снова",
                                        "Tags changed; drag again",
                                    )
                                    .into();
                                }
                            }
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(rect, 4.0, super::tag_material_ui::color(row));
                            if ui
                                .button(if expanded {
                                    tr("Закрыть", "Close")
                                } else {
                                    tr("Изменить", "Edit")
                                })
                                .clicked()
                            {
                                clicked = Some(row.clone());
                            }
                        },
                    );
                    let target = ui.interact(
                        geometry.labels.union(geometry.controls),
                        ui.id().with("tag-drop"),
                        egui::Sense::hover(),
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::new(("tag-target", row.number)), target.rect)
                    });
                    if let Some(payload) = target.dnd_hover_payload::<TagDrag>()
                        && payload.tab == self.tab.unwrap_or_default()
                        && payload.source != row.number
                    {
                        ui.painter().rect_stroke(
                            target.rect,
                            4.0,
                            ui.visuals().selection.stroke,
                            egui::StrokeKind::Inside,
                        );
                    }
                    if let Some(payload) = target.dnd_release_payload::<TagDrag>() {
                        drop = Some(((*payload).clone(), row.number));
                    }
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new(("tag-row-bottom", row.number)),
                            ui.min_rect().bottom(),
                        )
                    });
                    if expanded && let Some(slot) = self.draft.as_mut() {
                        #[cfg(test)]
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::new(("tag-editor-top", row.number)),
                                ui.cursor().top(),
                            )
                        });
                        let before = slot.clone();
                        ui.add_space(14.0);
                        ui.heading(format!("{} {}", tr("Тег", "Tag"), slot.number));
                        ui.add_enabled_ui(!self.conflicted, |ui| edit_controls(ui, slot));
                        ui.label(tr("Предпросмотр материала", "Material preview"));
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.preview_selected, tr("Выбрано", "Selected"));
                            ui.checkbox(&mut self.preview_cursor, tr("Курсор", "Cursor"));
                        });
                        for image in [false, true] {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width().min(300.0), 72.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                6.0,
                                ui.visuals().widgets.inactive.weak_bg_fill,
                            );
                            let content = egui::Rect::from_min_max(
                                rect.min,
                                egui::pos2(rect.right(), rect.top() + 22.0),
                            );
                            super::tag_material_ui::paint(ui, content, slot);
                            ui.painter().text(
                                content.center(),
                                egui::Align2::CENTER_CENTER,
                                if image {
                                    tr("Пример изображения", "Image example")
                                } else {
                                    tr("Пример текста", "Text example")
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
                                ui.painter().rect_stroke(
                                    rect,
                                    6.0,
                                    stroke,
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if self.preview_cursor {
                                super::shelf_view::paint_cursor(ui, rect);
                            }
                            ui.add_space(4.0);
                        }
                        ui.weak(tr(
                "Оптическое стекло в предпросмотре недоступно; проверьте его в основном окне.",
                "Optical glass is unavailable in the preview; check it in the main window.",
            ));
                        draft_changed |= *slot != before;
                    }
                });
            }
        });
        if draft_changed {
            self.dirty = true;
            if self.flush(api) {
                changed = true;
            }
        }
        if let Some((drag, target)) = drop
            && self.reorder(api, &drag, target)
        {
            changed = true;
        }
        if let Some(row) = clicked
            && self.flush(api)
        {
            if self
                .draft
                .as_ref()
                .is_some_and(|slot| slot.number == row.number)
            {
                self.draft = None;
            } else {
                self.draft = Some(row);
            }
        }
        if self.dirty
            && !self.conflicted
            && ui
                .button(tr("Повторить сохранение", "Retry save"))
                .clicked()
            && self.flush(api)
        {
            changed = true;
        }
        if !self.error.is_empty() {
            ui.colored_label(
                Color32::LIGHT_RED,
                format!("{}: {}", tr("Не сохранено", "Not saved"), self.error),
            );
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_settings_editor_preserves_draft_and_cannot_overwrite_reordered_tag() {
        use katran::interaction::ShelfTab;
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100_000).unwrap();
        let mut state = State {
            tab: Some(ShelfTab::Images),
            ..Default::default()
        };
        assert!(state.load_slots(&mut api));
        let revision = state.revision.clone().unwrap();
        let mut draft = state.slots.as_ref().unwrap()[0].clone();
        draft.name = "Pending".into();
        state.draft = Some(draft);
        state.dirty = true;
        let result = api.execute(Request {
            api_version: 1,
            request_id: "reorder".into(),
            command: Command::ReorderTagSlots {
                tab: ShelfTab::Images,
                order: vec![2, 1, 3, 4, 5, 6, 7, 8, 9],
                expected_revision: revision,
            },
        });
        assert_eq!(result["ok"], true);
        assert!(!state.flush(&mut api));
        assert!(state.conflicted && state.dirty);
        assert_eq!(state.draft.as_ref().unwrap().name, "Pending");
        assert!(!state.flush(&mut api));
        let after = api.execute(Request {
            api_version: 1,
            request_id: "list".into(),
            command: Command::ListTagSlotsForTab {
                tab: ShelfTab::Images,
            },
        });
        assert_eq!(after["result"]["slots"], result["result"]["slots"]);
    }

    #[test]
    fn dragging_grip_moves_tag_to_drop_row() {
        use katran::interaction::ShelfTab;
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100_000).unwrap();
        let mut state = State {
            tab: Some(ShelfTab::Images),
            ..Default::default()
        };
        assert!(state.load_slots(&mut api));
        let before = state.slots.as_ref().unwrap()[0].clone();
        let ctx = egui::Context::default();
        {
            let mut run = |events| {
                let mut changed = false;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 2000.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        changed = state.show(ui, &mut api);
                    },
                );
                output.textures_delta.clear();
                changed
            };
            run(vec![]);
            let (source, target): (egui::Rect, egui::Rect) = ctx.data(|d| {
                (
                    d.get_temp(egui::Id::new(("tag-grip", 1))).unwrap(),
                    d.get_temp(egui::Id::new(("tag-target", 3))).unwrap(),
                )
            });
            let from = source.center();
            let to = target.center();
            run(vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            run(vec![egui::Event::PointerMoved(to)]);
            assert!(run(vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE
            }]));
        }
        let after = &state.slots.as_ref().unwrap()[2];
        assert_eq!(after.name, before.name);
        assert_eq!(after.color, before.color);
        assert_eq!(after.number, 3);
    }

    #[test]
    fn drag_reorder_follows_tag_and_rejects_stale_snapshot() {
        use katran::interaction::ShelfTab;
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100_000).unwrap();
        let mut state = State {
            tab: Some(ShelfTab::Images),
            ..Default::default()
        };
        assert!(state.load_slots(&mut api));
        let mut draft = state.slots.as_ref().unwrap()[0].clone();
        draft.name = "Moved".into();
        state.draft = Some(draft);
        state.dirty = true;
        assert!(state.flush(&mut api));
        assert!(state.load_slots(&mut api));
        let drag = TagDrag {
            tab: ShelfTab::Images,
            source: 1,
            revision: state.revision.clone().unwrap(),
        };
        assert!(state.reorder(&mut api, &drag, 3));
        assert_eq!(state.slots.as_ref().unwrap()[2].name, "Moved");
        assert_eq!(state.draft.as_ref().unwrap().number, 3);
        let after = state.slots.clone();
        assert!(!state.reorder(&mut api, &drag, 3));
        assert_eq!(state.slots, after);
        assert!(state.draft.is_none());
        let text = api.execute(Request {
            api_version: 1,
            request_id: "test".into(),
            command: Command::ListTagSlotsForTab {
                tab: ShelfTab::Text,
            },
        });
        assert_ne!(text["result"]["slots"][2]["name"], "Moved");
        assert_eq!(move_order(3, 1).unwrap(), vec![3, 1, 2, 4, 5, 6, 7, 8, 9]);
        assert!(move_order(1, 1).is_none());
    }

    #[test]
    fn expanded_editor_is_between_its_row_and_next_row() {
        use katran::interaction::ShelfTab;
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100_000).unwrap();
        let response = api.execute(Request {
            api_version: 1,
            request_id: "test".into(),
            command: Command::ListTagSlotsForTab {
                tab: ShelfTab::Images,
            },
        });
        let slots: Vec<Slot> = serde_json::from_value(response["result"]["slots"].clone()).unwrap();
        let mut state = State {
            tab: Some(ShelfTab::Images),
            draft: Some(slots[0].clone()),
            slots: Some(slots),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 2000.0),
                )),
                ..Default::default()
            },
            |ui| {
                state.show(ui, &mut api);
            },
        );
        output.textures_delta.clear();
        let (row_bottom, editor_top, next_bottom): (f32, f32, f32) = ctx.data(|d| {
            (
                d.get_temp(egui::Id::new(("tag-row-bottom", 1))).unwrap(),
                d.get_temp(egui::Id::new(("tag-editor-top", 1))).unwrap(),
                d.get_temp(egui::Id::new(("tag-row-bottom", 2))).unwrap(),
            )
        });
        assert!(editor_top >= row_bottom && editor_top < next_bottom);
    }

    #[test]
    fn flush_saves_to_selected_tab_and_retains_invalid_draft() {
        use katran::interaction::ShelfTab;
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100_000).unwrap();
        let response = api.execute(Request {
            api_version: 1,
            request_id: "test".into(),
            command: Command::ListTagSlotsForTab {
                tab: ShelfTab::Text,
            },
        });
        let mut slot: Slot =
            serde_json::from_value(response["result"]["slots"][0].clone()).unwrap();
        slot.color = "#123456".into();
        let mut state = State {
            tab: Some(ShelfTab::Text),
            draft: Some(slot.clone()),
            dirty: true,
            revision: response["result"]["revision"].as_str().map(str::to_owned),
            ..Default::default()
        };
        assert!(state.flush(&mut api));
        let saved = api.execute(Request {
            api_version: 1,
            request_id: "test".into(),
            command: Command::ListTagSlotsForTab {
                tab: ShelfTab::Text,
            },
        });
        assert_eq!(saved["result"]["slots"][0]["color"], "#123456");
        state.draft.as_mut().unwrap().color = "invalid".into();
        state.dirty = true;
        assert!(!state.flush(&mut api));
        assert!(state.dirty);
        assert_eq!(state.tab, Some(ShelfTab::Text));
        assert_eq!(state.draft.as_ref().unwrap().color, "invalid");
    }

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

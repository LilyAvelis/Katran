//! Live slot editor is a client of the scoped tag commands.
use super::i18n::tr;
use super::ui_foundation::{SettingsRow, SettingsSection, text_field};
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
    ui.vertical_centered(|ui| {
        let width = ui.available_width().min(365.0);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                edit_fields(ui, slot);
            },
        );
    });
}

fn edit_fields(ui: &mut egui::Ui, slot: &mut Slot) {
    ui.label(tr("Название", "Name"));
    ui.add(text_field(
        egui::TextEdit::singleline(&mut slot.name).desired_width(ui.available_width().min(420.0)),
    ));
    ui.add_space(4.0);
    let c = super::tag_material_ui::color(slot);
    let mut rgb = [c.r(), c.g(), c.b()];
    let previous_material = slot.material;
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Цвет", "Color"));
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            slot.color = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
        }
        ui.add_space(12.0);
        ui.label(tr("Фактура", "Texture"));
        egui::ComboBox::from_id_salt("tag-material")
            .selected_text(match slot.material {
                Material::Solid => tr("Сплошная", "Solid"),
                Material::Dots => tr("Точки", "Dots"),
                Material::Lines => tr("Линии", "Lines"),
                Material::Grain => tr("Зерно", "Grain"),
                Material::Flower => tr("Цветы", "Flowers"),
                Material::Star => tr("Звёзды", "Stars"),
                Material::Diamond => tr("Ромбы", "Diamonds"),
                Material::Grid => tr("Сетка", "Grid"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut slot.material, Material::Solid, tr("Сплошная", "Solid"));
                ui.selectable_value(&mut slot.material, Material::Dots, tr("Точки", "Dots"));
                ui.selectable_value(&mut slot.material, Material::Lines, tr("Линии", "Lines"));
                ui.selectable_value(&mut slot.material, Material::Grain, tr("Зерно", "Grain"));
                ui.selectable_value(&mut slot.material, Material::Flower, tr("Цветы", "Flowers"));
                ui.selectable_value(&mut slot.material, Material::Star, tr("Звёзды", "Stars"));
                ui.selectable_value(
                    &mut slot.material,
                    Material::Diamond,
                    tr("Ромбы", "Diamonds"),
                );
                ui.selectable_value(&mut slot.material, Material::Grid, tr("Сетка", "Grid"));
            });
    });
    default_texture_strength(slot, previous_material);
    ui.add_space(6.0);
    super::ui_foundation::slider_scope(ui, |ui| {
        let width = (ui.available_width() - 190.0).clamp(80.0, 164.0);
        ui.spacing_mut().slider_width = width;
        ui.add(
            egui::Slider::new(&mut slot.opacity, 0..=100)
                .suffix("%")
                .text(tr("Непрозрачность цвета", "Color opacity")),
        );
        if slot.material != Material::Solid {
            ui.add(
                egui::Slider::new(&mut slot.intensity, 0..=100)
                    .suffix("%")
                    .text(tr("Сила фактуры", "Texture intensity")),
            );
            ui.add(egui::Slider::new(&mut slot.scale, 1..=100).text(tr("Масштаб", "Scale")));
            if slot.material == Material::Lines {
                ui.add(
                    egui::Slider::new(&mut slot.angle, 0..=359)
                        .suffix("°")
                        .text(tr("Угол", "Angle")),
                );
            }
        }
    });
}

pub(super) fn editor_modal<R>(
    ctx: &egui::Context,
    id: &'static str,
    anchor: Option<egui::Pos2>,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    let mut modal = egui::Modal::new(egui::Id::new(id))
        .backdrop_color(Color32::from_black_alpha(24))
        .frame(super::action_row::popup_frame(ctx));
    if let Some(anchor) = anchor {
        modal = modal.area(
            egui::Area::new(egui::Id::new(id))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(anchor),
        );
    }
    modal.show(ctx, |ui| {
        ui.set_width(width);
        ui.vertical_centered(|ui| {
            let font = egui::FontFamily::Name(super::ui_fonts::BOLD_FAMILY.into());
            let title = egui::RichText::new(tr("Оформление тега", "Tag appearance")).size(19.0);
            let has_bold = ui.fonts(|fonts| fonts.definitions().families.contains_key(&font));
            ui.label(if has_bold {
                title.family(font)
            } else {
                title.strong()
            });
        });
        ui.add_space(6.0);
        content(ui)
    })
}

fn sample_card(ui: &mut egui::Ui, slot: &Slot, selected: &mut bool, cursor: &mut bool) {
    ui.vertical_centered(|ui| {
        ui.label(tr("Материал на карточке", "Card material"));
        let size = egui::vec2(ui.available_width().min(155.0), 138.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 6.0, Color32::from_gray(60));
        super::tag_material_ui::paint(ui, rect, slot);
        let image = egui::Rect::from_min_max(
            rect.min + egui::vec2(13.0, 12.0),
            rect.max - egui::vec2(13.0, 34.0),
        );
        painter.rect_filled(image, 5.0, Color32::from_rgb(25, 31, 48));
        let sun = image.left_top() + egui::vec2(image.width() * 0.69, image.height() * 0.25);
        painter.circle_filled(sun, 8.0, Color32::from_rgb(238, 188, 141));
        for (x, color) in [
            (0.43, Color32::from_rgb(81, 95, 136)),
            (0.72, Color32::from_rgb(50, 66, 99)),
        ] {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    image.left_bottom() + egui::vec2(4.0, -2.0),
                    image.left_top() + egui::vec2(image.width() * x, image.height() * 0.30),
                    image.right_bottom() - egui::vec2(4.0, 2.0),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        painter.text(
            egui::pos2(rect.center().x, rect.bottom() - 17.0),
            egui::Align2::CENTER_CENTER,
            tr("Пример карточки", "Sample card"),
            egui::FontId::proportional(13.0),
            super::tag_material_ui::title_color(slot),
        );
        if let Some(stroke) = super::shelf_view::card_outline(ui.ctx(), *selected) {
            painter.rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
        }
        if *cursor {
            super::shelf_view::paint_cursor(ui, rect);
        }
        ui.checkbox(selected, tr("Выбрано", "Selected"));
        ui.checkbox(cursor, tr("Курсор", "Cursor"));
    });
}

pub(super) fn editor_footer(ui: &mut egui::Ui, number: u8, on_shelf: bool) -> bool {
    use super::special_mode_bar;
    use egui_lucide::Lucide;
    ui.separator();
    special_mode_bar::show(ui, |ui| {
        ui.add(
            Lucide::Tag
                .size(15.0)
                .image()
                .tint(ui.visuals().text_color()),
        );
        ui.label(format!("{} · {number}", tr("Тег", "Tag")));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if on_shelf {
                tr("Назад · Esc", "Back · Esc")
            } else {
                tr("Закрыть", "Close")
            };
            special_mode_bar::action(ui, label, None, true, false, 110.0).clicked()
        })
        .inner
    })
    .inner
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
    pub fn is_editing(&self) -> bool {
        self.draft.is_some()
    }

    fn show_editor(&mut self, ctx: &egui::Context, api: &mut Service, opened: bool) -> bool {
        let Some(slot) = self.draft.as_mut() else {
            return false;
        };
        let before = slot.clone();
        let width = (ctx.content_rect().width() - 36.0).clamp(240.0, 560.0);
        let mut retry = false;
        let mut reload = false;
        let modal = editor_modal(ctx, "settings-tag-editor", None, width, |ui| {
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 145.0).max(80.0))
                .show(ui, |ui| {
                    if width >= 490.0 {
                        ui.horizontal_top(|ui| {
                            let main = ui.available_width() - 185.0;
                            ui.allocate_ui_with_layout(
                                egui::vec2(main, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(main);
                                    ui.add_enabled_ui(!self.conflicted, |ui| {
                                        edit_controls(ui, slot)
                                    });
                                },
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(175.0, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(175.0);
                                    sample_card(
                                        ui,
                                        slot,
                                        &mut self.preview_selected,
                                        &mut self.preview_cursor,
                                    );
                                },
                            );
                        });
                    } else {
                        ui.add_enabled_ui(!self.conflicted, |ui| edit_controls(ui, slot));
                        ui.add_space(12.0);
                        sample_card(
                            ui,
                            slot,
                            &mut self.preview_selected,
                            &mut self.preview_cursor,
                        );
                    }
                    if !self.error.is_empty() {
                        ui.colored_label(Color32::LIGHT_RED, &self.error);
                        if self.conflicted {
                            reload = ui
                                .button(tr(
                                    "Отменить черновик и обновить",
                                    "Discard draft and reload",
                                ))
                                .clicked();
                        } else if self.dirty {
                            retry = ui
                                .button(tr("Повторить сохранение", "Retry save"))
                                .clicked();
                        }
                    }
                });
            editor_footer(ui, slot.number, false)
        });
        let edited = before != *slot;
        let mut changed = false;
        if reload {
            self.draft = None;
            self.dirty = false;
            self.conflicted = false;
            self.slots = None;
            self.revision = None;
            self.error.clear();
            self.load_slots(api);
        } else {
            if edited || retry {
                self.dirty = true;
                changed = self.flush(api);
            }
            if (modal.inner || (!opened && modal.should_close())) && !self.dirty {
                self.draft = None;
                egui::Popup::close_all(ctx);
                ctx.memory_mut(|memory| {
                    if let Some(focus) = memory.focused() {
                        memory.surrender_focus(focus);
                    }
                });
            }
        }
        changed
    }

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
        if self.conflicted && self.draft.is_none() {
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
        ui.weak(tr(
            "Теги назначаются только закреплённым объектам.",
            "Tags can only be assigned to pinned items.",
        ));
        ui.add_space(12.0);
        let slots = self.slots.clone().unwrap_or_default();
        let mut clicked = None;
        let mut drop = None;
        SettingsSection::show(ui, |ui| {
            for row in &slots {
                ui.push_id((format!("{:?}", self.tab), row.number), |ui| {
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
                            if ui.button(tr("Изменить", "Edit")).clicked() {
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
                });
            }
        });
        if let Some((drag, target)) = drop
            && self.reorder(api, &drag, target)
        {
            changed = true;
        }
        let mut opened = false;
        if let Some(row) = clicked
            && self.flush(api)
        {
            self.draft = Some(row);
            opened = true;
        }
        changed |= self.show_editor(ui.ctx(), api, opened);
        if self.draft.is_none() && !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
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
    fn settings_tag_editor_opens_as_a_centered_modal() {
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
        for _ in 0..3 {
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
        }
        let rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("settings-tag-editor")))
            .expect("Edit should use a separate modal");
        assert!(
            rect.center().distance(egui::pos2(450.0, 1000.0)) <= 2.0,
            "{rect:?}"
        );
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

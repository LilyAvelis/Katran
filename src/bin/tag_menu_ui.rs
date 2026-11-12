//! Actions-style tag picker. Domain operations remain dispatcher commands.
use eframe::egui::{self, Color32, RichText};
use katran::tag_slots::Slot;

#[derive(Default)]
pub struct State {
    pub open: bool,
    pub slots: Vec<Slot>,
    pub(super) query: String,
    selected: usize,
    focus: bool,
    fresh: bool,
    scroll: bool,
    retry: Option<(Vec<i64>, Option<u8>)>,
    error: String,
}
#[derive(Clone, Copy)]
pub enum Intent {
    Filter(Option<u8>),
    Assign(Option<u8>),
    Retry,
}
impl State {
    pub fn start(&mut self, slots: Vec<Slot>) {
        *self = Self {
            open: true,
            slots,
            focus: false,
            fresh: true,
            scroll: true,
            ..Default::default()
        };
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        eligible: bool,
        selection_count: usize,
        has_tag: bool,
        summary: &str,
        active: Option<u8>,
    ) -> Option<Intent> {
        if !self.open {
            return None;
        }
        if self.fresh {
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("tag-menu-search")));
            self.fresh = false;
        }
        let mut rows: Vec<(String, Intent, bool)> = Vec::new();
        rows.push(("Фильтр · Все".into(), Intent::Filter(None), true));
        for slot in &self.slots {
            rows.push((
                format!(
                    "Фильтр · {} · {}{}",
                    slot.number,
                    slot.name,
                    if active == Some(slot.number) {
                        " ✓"
                    } else {
                        ""
                    }
                ),
                Intent::Filter(Some(slot.number)),
                true,
            ));
        }
        for slot in &self.slots {
            rows.push((
                format!("Назначить · {} · {}", slot.number, slot.name),
                Intent::Assign(Some(slot.number)),
                eligible,
            ));
        }
        rows.push((
            format!("Снять теги · Ctrl 0 · {selection_count}"),
            Intent::Assign(None),
            has_tag,
        ));
        if let Some((ids, _)) = &self.retry {
            rows.push((
                format!("Повторить ошибки · {}", ids.len()),
                Intent::Retry,
                true,
            ));
        }
        let query = self.query.to_lowercase();
        rows.retain(|(label, _, _)| label.to_lowercase().contains(&query));
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        let search_focused = ctx.memory(|m| m.has_focus(egui::Id::new("tag-menu-search")));
        let shortcut = if search_focused {
            None
        } else {
            ctx.input_mut(|input| {
                let shortcut = input.events.iter().find_map(|event| match event {
                    egui::Event::Key {
                        key,
                        physical_key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } if modifiers.is_none()
                        || *modifiers == egui::Modifiers::SHIFT
                        || *modifiers == egui::Modifiers::CTRL =>
                    {
                        match physical_key.unwrap_or(*key) {
                            egui::Key::Num0 if modifiers.ctrl || modifiers.shift => Some((0, true)),
                            egui::Key::Num1 if !modifiers.ctrl => Some((1, modifiers.shift)),
                            egui::Key::Num2 if !modifiers.ctrl => Some((2, modifiers.shift)),
                            egui::Key::Num3 if !modifiers.ctrl => Some((3, modifiers.shift)),
                            egui::Key::Num4 if !modifiers.ctrl => Some((4, modifiers.shift)),
                            egui::Key::Num5 if !modifiers.ctrl => Some((5, modifiers.shift)),
                            egui::Key::Num6 if !modifiers.ctrl => Some((6, modifiers.shift)),
                            egui::Key::Num7 if !modifiers.ctrl => Some((7, modifiers.shift)),
                            egui::Key::Num8 if !modifiers.ctrl => Some((8, modifiers.shift)),
                            egui::Key::Num9 if !modifiers.ctrl => Some((9, modifiers.shift)),
                            egui::Key::Slash if modifiers.is_none() => Some((0, false)),
                            _ => None,
                        }
                    }
                    _ => None,
                });
                if shortcut.is_some() {
                    super::shortcuts_ui::discard(input);
                }
                shortcut
            })
        };
        if let Some((number, assign)) = shortcut {
            if number == 0 && assign {
                if !has_tag {
                    self.error = "У выбранных объектов нет тегов".into();
                } else {
                    self.open = false;
                    return Some(Intent::Assign(None));
                }
            } else if number == 0 {
                self.open = false;
                return Some(Intent::Filter(None));
            }
            if self.slots.iter().any(|slot| slot.number == number) {
                if assign && !eligible {
                    self.error = "Для назначения выделите только закреплённые объекты".into();
                } else {
                    self.open = false;
                    return Some(if assign {
                        Intent::Assign(Some(number))
                    } else {
                        Intent::Filter(Some(number))
                    });
                }
            }
        }
        let before = self.selected;
        let enter = ctx.input_mut(|i| {
            if !rows.is_empty() && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                self.selected = (self.selected + 1) % rows.len();
            }
            if !rows.is_empty() && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                self.selected = (self.selected + rows.len() - 1) % rows.len();
            }
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
        });
        self.scroll |= before != self.selected || self.focus;
        let width = (ctx.content_rect().width() - 28.0).clamp(240.0, 420.0);
        let anchor = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("tags-button-rect")))
            .map(|r| r.right_top() + egui::vec2(0.0, -6.0))
            .unwrap_or(ctx.content_rect().right_bottom() - egui::vec2(8.0, 40.0));
        let mut picked = None;
        let mut optical = None;
        let result = egui::Modal::new(egui::Id::new("tag-menu"))
            .area(
                egui::Area::new(egui::Id::new("tag-menu"))
                    .order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_BOTTOM)
                    .fixed_pos(anchor),
            )
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(super::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                optical = Some((ui.painter().clone(), ui.painter().add(egui::Shape::Noop)));
                ui.set_width(width);
                ui.label(
                    RichText::new("Теги · фильтр и назначение")
                        .strong()
                        .size(13.0),
                );
                ui.weak(summary);
                if !self.error.is_empty() {
                    ui.colored_label(Color32::LIGHT_RED, &self.error);
                }
                if !eligible {
                    ui.weak("Для назначения все выбранные объекты должны быть закреплены.");
                }
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 180.0).clamp(80.0, 320.0))
                    .show(ui, |ui| {
                        for (index, (label, intent, enabled)) in rows.iter().enumerate() {
                            let shortcut_label = match intent {
                                Intent::Filter(None) => "/".to_owned(),
                                Intent::Filter(Some(number)) => number.to_string(),
                                Intent::Assign(Some(number)) => format!("Shift+{number}"),
                                _ => String::new(),
                            };
                            let response = ui
                                .add_enabled_ui(*enabled, |ui| {
                                    super::action_row::ActionRow::new(
                                        super::action_row::ActionPresentation {
                                            label,
                                            icon: egui_lucide::Lucide::Tag,
                                            primary_shortcut: &shortcut_label,
                                            full_shortcut_tooltip: "",
                                        },
                                    )
                                    .show(ui, index == self.selected)
                                })
                                .inner;
                            if index == self.selected && self.scroll {
                                response.scroll_to_me(None);
                                self.scroll = false;
                            }
                            if response.clicked() && *enabled {
                                picked = Some(*intent);
                            }
                        }
                        if rows.is_empty() {
                            ui.label("Ничего не найдено");
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id(egui::Id::new("tag-menu-search"))
                            .hint_text("Поиск тегов…")
                            .frame(egui::Frame::NONE)
                            .desired_width(width - 65.0),
                    );
                    if self.focus {
                        r.request_focus();
                        self.focus = false;
                    }
                    if r.changed() {
                        self.selected = 0;
                        self.scroll = true;
                    }
                    super::ui_style::keycap(ui, "Esc");
                });
            });
        if let Some((painter, index)) = optical
            && let Some(shape) = super::theme_ui::optical_shape(ctx, result.response.rect, 12.0)
        {
            painter.set(index, shape);
        }
        if enter && picked.is_none() {
            picked = rows
                .get(self.selected)
                .and_then(|(_, intent, enabled)| enabled.then_some(*intent));
        }
        if picked.is_some() || result.should_close() {
            self.open = false;
        }
        picked
    }
}

impl super::Shelf {
    pub(super) fn open_tag_menu(&mut self) {
        if self.state["tab"] == "folder" {
            return;
        }
        let response = self.call(katran::Command::ListTagSlots);
        match serde_json::from_value::<Vec<Slot>>(response["slots"].clone()) {
            Ok(slots) => self.tag_menu.start(slots),
            Err(_) => {
                if self.message.is_empty() {
                    self.message = "Не удалось загрузить теги".into();
                }
            }
        }
    }
    pub(super) fn tag_menu_panel(&mut self, ctx: &egui::Context) {
        if !self.tag_menu.open {
            return;
        }
        let ids = self.selected_ids();
        let eligible = !ids.is_empty()
            && ids.iter().all(|id| {
                self.events.iter().any(|event| {
                    event["id"].as_i64() == Some(*id) && event["properties"]["pinned"] == true
                })
            });
        let active = self.state["tag_slot_filter"]
            .as_u64()
            .and_then(|n| u8::try_from(n).ok());
        let has_tag = ids
            .iter()
            .any(|id| self.tag_material_cache.assignments.contains_key(id));
        let tags: std::collections::HashSet<_> = ids
            .iter()
            .map(|id| self.tag_material_cache.assignments.get(id).copied())
            .collect();
        let summary = if ids.is_empty() {
            "Ничего не выбрано".to_owned()
        } else if tags.len() > 1 {
            format!("Выбрано: {} · разные теги", ids.len())
        } else if let Some(Some(number)) = tags.iter().next().copied() {
            let name = self
                .tag_menu
                .slots
                .iter()
                .find(|s| s.number == number)
                .map(|s| s.name.as_str())
                .unwrap_or("Тег");
            format!("Выбрано: {} · {}", ids.len(), name)
        } else {
            format!("Выбрано: {} · без тега", ids.len())
        };
        if let Some(intent) =
            self.tag_menu
                .show(ctx, eligible, ids.len(), has_tag, &summary, active)
        {
            match intent {
                Intent::Filter(slot) => {
                    let state = self.call(katran::Command::FilterTagSlot { slot });
                    if !state.is_null() {
                        self.state = state;
                        self.refresh();
                    }
                }
                Intent::Assign(slot) => {
                    if ids.is_empty() || (slot.is_some() && !eligible) {
                        return;
                    }
                    self.assign_tag_menu(ids, slot);
                }
                Intent::Retry => {
                    if let Some((ids, slot)) = self.tag_menu.retry.take() {
                        self.assign_tag_menu(ids, slot);
                    }
                }
            }
        }
    }
    fn assign_tag_menu(&mut self, ids: Vec<i64>, slot: Option<u8>) {
        let result = self.call(katran::Command::AssignTagSlot {
            ids: ids.clone(),
            slot,
        });
        if result.is_null() {
            self.tag_menu.retry = Some((ids, slot));
            self.tag_menu.error = self.message.clone();
            self.tag_menu.open = true;
            return;
        }
        let failures: Vec<_> = result["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["outcome"] != "applied")
            .collect();
        if !failures.is_empty() {
            let failed_ids = failures.iter().filter_map(|v| v["id"].as_i64()).collect();
            self.tag_menu.error = failures
                .iter()
                .map(|v| format!("{}: {}", v["id"], v["outcome"].as_str().unwrap_or("ошибка")))
                .collect::<Vec<_>>()
                .join(", ");
            self.tag_menu.retry = Some((failed_ids, slot));
            self.tag_menu.open = true;
        } else {
            self.tag_menu.retry = None;
            self.tag_menu.error.clear();
        }
        self.state = self.call(katran::Command::Interaction);
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    fn slot(number: u8) -> katran::tag_slots::Slot {
        katran::tag_slots::Slot {
            number,
            name: "Test".into(),
            color: "#123456".into(),
            material: katran::tag_slots::Material::Solid,
            scale: 50,
            intensity: 50,
            opacity: 55,
            angle: 0,
            seed: 1,
        }
    }

    #[test]
    fn shifted_physical_digit_assigns_plain_digit_filters_and_slash_clears() {
        use eframe::egui::{self, Key, Modifiers};
        for (key, physical, modifiers, eligible, expected) in [
            (Key::A, Key::Num2, Modifiers::SHIFT, true, "assign"),
            (Key::Num2, Key::Num2, Modifiers::NONE, true, "filter"),
            (Key::Slash, Key::Slash, Modifiers::NONE, true, "clear"),
            (Key::A, Key::Num2, Modifiers::SHIFT, false, "disabled"),
            (Key::Num2, Key::Num2, Modifiers::CTRL, true, "ignored"),
        ] {
            let ctx = egui::Context::default();
            let mut state = super::State::default();
            state.start(vec![slot(2)]);
            let mut result = None;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: Some(physical),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ui| {
                    result = state.show(ui.ctx(), eligible, 1, false, "", None);
                },
            );
            out.textures_delta.clear();
            match expected {
                "assign" => assert!(matches!(result, Some(super::Intent::Assign(Some(2))))),
                "filter" => assert!(matches!(result, Some(super::Intent::Filter(Some(2))))),
                "clear" => assert!(matches!(result, Some(super::Intent::Filter(None)))),
                "disabled" => {
                    assert!(result.is_none());
                    assert!(state.open);
                    assert!(!state.error.is_empty());
                }
                _ => {
                    assert!(result.is_none());
                    assert!(state.open);
                }
            }
        }
    }

    #[test]
    fn control_zero_clears_selected_tag() {
        use eframe::egui::{self, Key, Modifiers};
        let ctx = egui::Context::default();
        let mut state = super::State::default();
        state.start(vec![slot(1)]);
        let mut intent = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Num0,
                    physical_key: Some(Key::Num0),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ui| intent = state.show(ui.ctx(), true, 1, true, "", None),
        );
        output.textures_delta.clear();
        assert!(matches!(intent, Some(super::Intent::Assign(None))));
    }

    #[test]
    fn shifted_digit_remains_text_when_search_has_focus() {
        use eframe::egui::{self, Key, Modifiers};
        let ctx = egui::Context::default();
        let mut state = super::State::default();
        state.start(vec![slot(2)]);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.show(ui.ctx(), true, 1, false, "", None);
        });
        out.textures_delta.clear();
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("tag-menu-search")));
        let mut result = None;
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::Key {
                        key: Key::A,
                        physical_key: Some(Key::Num2),
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::SHIFT,
                    },
                    egui::Event::Text("@".into()),
                ],
                ..Default::default()
            },
            |ui| {
                result = state.show(ui.ctx(), true, 1, false, "", None);
            },
        );
        out.textures_delta.clear();
        assert!(result.is_none());
        assert_eq!(state.query, "@");
        assert!(state.open);
    }

    #[test]
    fn partial_assignment_retries_only_failed_items() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let make = |app: &mut super::super::Shelf, name: &str| {
            app.call(katran::Command::IngestBytes {
                name: name.into(),
                bytes: vec![1],
                source: "test".into(),
            })["id"]
                .as_i64()
                .unwrap()
        };
        let first = make(&mut app, "one.png");
        let second = make(&mut app, "two.png");
        app.call(katran::Command::TogglePins { ids: vec![first] });
        app.assign_tag_menu(vec![first, second], Some(1));
        assert_eq!(app.tag_menu.retry.as_ref().unwrap().0, vec![second]);
        assert_eq!(
            app.call(katran::Command::GetTagSlot { id: first })["slot"],
            1
        );
        assert!(app.call(katran::Command::GetTagSlot { id: second })["slot"].is_null());
        app.call(katran::Command::TogglePins { ids: vec![second] });
        let (ids, slot) = app.tag_menu.retry.take().unwrap();
        app.assign_tag_menu(ids, slot);
        assert!(app.tag_menu.retry.is_none());
        assert_eq!(
            app.call(katran::Command::GetTagSlot { id: second })["slot"],
            1
        );
    }
}

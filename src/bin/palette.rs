use super::*;

impl Shelf {
    pub(super) fn palette(&mut self, ctx: &egui::Context) {
        if !self.actions {
            return;
        }
        let catalogue = self.call(Command::Actions);
        let actions: Vec<_> = catalogue
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| {
                a["label"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&self.action_query.to_lowercase())
            })
            .cloned()
            .collect();
        let count = actions.len();
        self.action_selected = self.action_selected.min(count.saturating_sub(1));
        if self.shortcut_editor.is_none()
            && shortcuts_ui::take_assignment_key(ctx)
            && let Some(action) = actions.get(self.action_selected)
        {
            self.shortcut_editor = Some(shortcuts_ui::Editor::new(
                action["id"].as_str().unwrap(),
                action["label"].as_str().unwrap(),
                action["shortcut"].as_str().unwrap_or(""),
            ));
        }
        if let Some(editor) = self.shortcut_editor.as_mut() {
            let result = egui::Modal::new(egui::Id::new("shortcut-editor"))
                .show(ctx, |ui| editor.render(ui));
            if let Some(intent) = result.inner
                && self
                    .api
                    .as_mut()
                    .is_some_and(|api| shortcuts_ui::apply(api, editor, intent))
            {
                self.shortcut_editor = None;
            }
            return;
        }
        let previous_selection = self.action_selected;
        let enter = ctx.input_mut(|i| {
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                self.action_selected = (self.action_selected + 1) % count;
            }
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                self.action_selected = (self.action_selected + count - 1) % count;
            }
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
        });
        self.scroll_actions |= previous_selection != self.action_selected || self.focus_actions;
        let width = (ctx.content_rect().width() - 28.0).clamp(240.0, 420.0);
        let mut picked = None;
        // Adapted directly from Afio palette.rs: anchor, surface, groups, rows, bottom search.
        let result = egui::Modal::new(egui::Id::new("actions"))
            .area(
                egui::Modal::default_area(egui::Id::new("actions"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-14.0, -58.0]),
            )
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(
                egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                    .fill(Color32::from_rgb(69, 69, 74))
                    .stroke(egui::Stroke::new(1.0, Color32::from_gray(118)))
                    .corner_radius(12)
                    .inner_margin(12),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.label(
                    RichText::new("Действия с выделением и полкой")
                        .strong()
                        .size(13.0),
                );
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 180.0).clamp(80.0, 320.0))
                    .show(ui, |ui| {
                        let mut group = String::new();
                        for (index, a) in actions.iter().enumerate() {
                            let next = a["group"].as_str().unwrap_or("");
                            if group != next {
                                group = next.into();
                                ui.add_space(3.0);
                                ui.label(
                                    RichText::new(&group)
                                        .size(11.0)
                                        .color(Color32::from_gray(185)),
                                );
                            }
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 38.0),
                                egui::Sense::click(),
                            );
                            if index == self.action_selected || response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    7.0,
                                    Color32::from_rgb(100, 100, 108),
                                );
                            }
                            if index == self.action_selected && self.scroll_actions {
                                response.scroll_to_me(None);
                                self.scroll_actions = false;
                            }
                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(rect.shrink2(egui::vec2(9.0, 3.0)))
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                |ui| {
                                    ui.add(
                                        action_icons::icon(a["id"].as_str().unwrap_or(""))
                                            .size(16.0)
                                            .image(),
                                    );
                                    ui.label(a["label"].as_str().unwrap_or(""));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui_style::keycap(
                                                ui,
                                                a["shortcut"].as_str().unwrap_or(""),
                                            );
                                        },
                                    );
                                },
                            );
                            if response.clicked() {
                                picked = a["id"].as_str().map(str::to_owned);
                            }
                        }
                        if actions.is_empty() {
                            ui.label("Ничего не найдено");
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.action_query)
                            .hint_text("Поиск действий…")
                            .frame(egui::Frame::NONE)
                            .desired_width(width - 65.0),
                    );
                    if self.focus_actions {
                        r.request_focus();
                        self.focus_actions = false;
                    }
                    if r.changed() {
                        self.action_selected = 0;
                        self.scroll_actions = true;
                    }
                    ui_style::keycap(ui, "Esc");
                });
            });
        if enter && picked.is_none() {
            picked = catalogue
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| {
                    a["label"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&self.action_query.to_lowercase())
                })
                .nth(self.action_selected)
                .and_then(|a| a["id"].as_str())
                .map(str::to_owned);
        }
        if let Some(id) = picked {
            self.run(id, ctx);
        } else if result.should_close() {
            self.actions = false;
        }
    }
}

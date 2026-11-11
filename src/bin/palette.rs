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
        // Use the actual trigger rectangle, independent of footer height and DPI.
        let trigger = ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new("actions-button-rect")));
        let anchor = trigger.map(|rect| rect.right_top() + egui::vec2(0.0, -6.0))
            .unwrap_or(ctx.content_rect().right_bottom() - egui::vec2(8.0, 40.0));
        let result = egui::Modal::new(egui::Id::new("actions"))
            .area(
                egui::Area::new(egui::Id::new("actions")).order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_BOTTOM)
                    .fixed_pos(anchor),
            )
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(crate::action_row::popup_frame(ctx))
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
                                        .color(theme_ui::color(ctx,theme_ui::Role::Muted,Color32::from_gray(185))),
                                );
                            }
                            let shortcut = a["shortcut"].as_str().unwrap_or("");
                            // Sequence hints are presentation, never persisted chord bindings.
                            let hint=sorting_ui::sequence_hint(a["id"].as_str().unwrap_or(""));
                            let display=if shortcut.is_empty(){hint.unwrap_or("")}else{crate::action_row::primary_shortcut(shortcut)};
                            let tooltip=hint.map(|hint|format!("Последовательность: {hint} · сначала запятая, затем клавиша")).unwrap_or_else(||shortcut.to_owned());
                            let response = crate::action_row::ActionRow::new(
                                crate::action_row::ActionPresentation {
                                    label: a["label"].as_str().unwrap_or(""),
                                    icon: action_icons::icon(a["id"].as_str().unwrap_or("")),
                                    primary_shortcut: display,
                                    full_shortcut_tooltip: &tooltip,
                                },
                            )
                            .show(ui, index == self.action_selected);
                            if index == self.action_selected && self.scroll_actions {
                                response.scroll_to_me(None);
                                self.scroll_actions = false;
                            }
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
        #[cfg(test)]
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("test-actions-popup-rect"), result.response.rect));
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

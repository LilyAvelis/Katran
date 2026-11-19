//! Domain-backed emoji group chooser using the shared Actions popup visuals.
use super::*;

pub(super) fn group_icon(group: Option<&str>) -> egui_lucide::Lucide {
    use egui_lucide::Lucide;
    match group {
        Some("Smileys & Emotion") => Lucide::FaceGrinning,
        Some("People & Body") => Lucide::UsersRound,
        Some("Animals & Nature") => Lucide::PawPrint,
        Some("Food & Drink") => Lucide::Utensils,
        Some("Travel & Places") => Lucide::Map,
        Some("Activities") => Lucide::Trophy,
        Some("Objects") => Lucide::Package,
        Some("Symbols") => Lucide::Shapes,
        Some("Flags") => Lucide::Flag,
        _ => Lucide::LayoutGrid,
    }
}

impl Shelf {
    pub(super) fn open_emoji_categories(&mut self) {
        self.pods_ui.category_query.clear();
        self.pods_ui.category_selected = 0;
        self.pods_ui.category_focus = true;
        self.pods_ui.category_open = true;
        self.pods_ui.category_just_opened = true;
    }

    pub(super) fn emoji_categories_panel(&mut self, ctx: &egui::Context) {
        if !self.pods_ui.category_open {
            return;
        }
        if self.state["tab"] != "pods" || self.state["emoji_catalogue"] != true {
            self.pods_ui.category_open = false;
            return;
        }
        let trigger_rect =
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("emoji-category-button-rect")));
        let clicked_trigger = trigger_rect.is_some_and(|rect| ctx.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, .. } if rect.contains(*pos)))));
        let just_opened = std::mem::take(&mut self.pods_ui.category_just_opened);
        if clicked_trigger && !just_opened {
            self.pods_ui.category_open = false;
            return;
        }
        let groups = self.call(Command::EmojiCategories);
        let query = self.pods_ui.category_query.trim().to_lowercase();
        let mut choices: Vec<(Option<String>, String)> = Vec::new();
        let all = i18n::tr("Все категории", "All Categories").to_owned();
        if query.is_empty() || all.to_lowercase().contains(&query) {
            choices.push((None, all));
        }
        for group in groups["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if query.is_empty() || group.to_lowercase().contains(&query) {
                choices.push((Some(group.to_owned()), group.to_owned()));
            }
        }
        let count = choices.len();
        self.pods_ui.category_selected =
            self.pods_ui.category_selected.min(count.saturating_sub(1));
        let mut scroll_to_selected = false;
        let enter = ctx.input_mut(|input| {
            if count > 0 && input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                self.pods_ui.category_selected = (self.pods_ui.category_selected + 1) % count;
                scroll_to_selected = true;
            }
            if count > 0 && input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                self.pods_ui.category_selected =
                    (self.pods_ui.category_selected + count - 1) % count;
                scroll_to_selected = true;
            }
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
        });
        let width = (ctx.content_rect().width() - 28.0).clamp(220.0, 360.0);
        let trigger =
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("emoji-category-button-rect")));
        let anchor = trigger
            .map(|rect| rect.right_bottom() + egui::vec2(0.0, 5.0))
            .unwrap_or(ctx.content_rect().right_top() + egui::vec2(-8.0, 42.0));
        let mut picked = None;
        let mut optical_slot = None;
        let result = egui::Modal::new(egui::Id::new("emoji-category-picker"))
            .area(
                egui::Area::new(egui::Id::new("emoji-category-picker"))
                    .order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_TOP)
                    .fixed_pos(anchor),
            )
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                optical_slot = Some((ui.painter().clone(), ui.painter().add(egui::Shape::Noop)));
                ui.set_width(width);
                ui.label(
                    RichText::new(i18n::tr("Категории emoji", "Emoji categories"))
                        .strong()
                        .size(14.0),
                );
                let search = ui.add(super::ui_foundation::text_field(
                    egui::TextEdit::singleline(&mut self.pods_ui.category_query)
                        .hint_text(i18n::tr("Найти категорию…", "Find a category…"))
                        .desired_width(width - 12.0),
                ));
                if self.pods_ui.category_focus {
                    search.request_focus();
                    self.pods_ui.category_focus = false;
                }
                if search.changed() {
                    self.pods_ui.category_selected = 0;
                }
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 180.0).clamp(80.0, 320.0))
                    .show(ui, |ui| {
                        for (index, (group, label)) in choices.iter().enumerate() {
                            let row = crate::action_row::ActionRow::new(
                                crate::action_row::ActionPresentation {
                                    label,
                                    icon: group_icon(group.as_deref()),
                                    primary_shortcut: "",
                                    full_shortcut_tooltip: "",
                                },
                            )
                            .show(ui, index == self.pods_ui.category_selected);
                            #[cfg(test)]
                            ctx.data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new(("emoji-category-row", index)),
                                    row.rect,
                                )
                            });
                            if index == self.pods_ui.category_selected && scroll_to_selected {
                                row.scroll_to_me(None);
                            }
                            if row.clicked() {
                                picked = Some(index);
                            }
                        }
                        if choices.is_empty() {
                            ui.label(i18n::tr("Ничего не найдено", "Nothing found"));
                        }
                    });
                ui.separator();
                ui.weak(i18n::tr(
                    "↑↓ выбрать · Enter открыть · Esc закрыть",
                    "↑↓ select · Enter open · Esc close",
                ));
            });
        #[cfg(test)]
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new("emoji-category-popup-rect"),
                result.response.rect,
            )
        });
        if let Some((painter, index)) = optical_slot
            && let Some(shape) = theme_ui::optical_shape(ctx, result.response.rect, 12.0)
        {
            painter.set(index, shape);
        }
        if enter && picked.is_none() && !choices.is_empty() {
            picked = Some(self.pods_ui.category_selected);
        }
        if let Some(index) = picked {
            let group = choices[index].0.clone();
            let next = self.call(Command::SetEmojiCategory { group });
            if next.is_object() {
                self.state = next;
                self.pods_ui.category_open = false;
                self.events.clear();
                self.next_cursor = None;
                self.loaded_query.clear();
                self.refresh();
            }
        } else if result.should_close() {
            self.pods_ui.category_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_unicode_group_has_a_specific_icon() {
        let mut service = katran::Service::open(tempfile::tempdir().unwrap().path(), 1000).unwrap();
        let reply = service.execute(katran::Request {
            api_version: 1,
            request_id: "icons".into(),
            command: katran::Command::EmojiCategories,
        });
        let groups = reply["result"]["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 9);
        for group in groups {
            assert_ne!(group_icon(group.as_str()), group_icon(None), "{group}");
        }
    }
}

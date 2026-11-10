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
                            let shortcut = a["shortcut"].as_str().unwrap_or("");
                            paint_row_contents(
                                ui,
                                rect,
                                a["id"].as_str().unwrap_or(""),
                                a["label"].as_str().unwrap_or(""),
                                shortcut,
                            );
                            let response = if shortcut.contains('/') {
                                response.on_hover_text(shortcut)
                            } else {
                                response
                            };
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

// Child text regions must retain the scroll viewport's clip. Replacing it
// lets rows outside the viewport paint over the popup header and search field.
fn paint_row_contents(ui: &mut egui::Ui, rect: egui::Rect, id: &str, label: &str, shortcut: &str) {
    let primary = primary_shortcut(shortcut);
    let key_width = primary
        .split_whitespace()
        .map(|key| {
            ui.painter()
                .layout_no_wrap(
                    key.to_owned(),
                    egui::FontId::proportional(crate::ui_foundation::tokens::CAPTION),
                    Color32::WHITE,
                )
                .size()
                .x
                + 8.0
        })
        .sum::<f32>()
        + primary.split_whitespace().count().saturating_sub(1) as f32 * 3.0;
    let (label_rect, keys_rect) = row_text_rects(rect, key_width);
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(9.0, 3.0))),
        |ui| {
            ui.add(action_icons::icon(id).size(16.0).image());
        },
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(label_rect));
        ui.add(egui::Label::new(label).truncate());
    });
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(keys_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(keys_rect));
            ui_style::keycap(ui, primary);
        },
    );
}

fn primary_shortcut(shortcut: &str) -> &str {
    shortcut.split('/').next().unwrap_or("").trim()
}

// Reserve separate clipped regions so even unusually long user bindings cannot
// collide with the action label in a narrow palette.
fn row_text_rects(row: egui::Rect, key_width: f32) -> (egui::Rect, egui::Rect) {
    let left = row.left() + 33.0;
    let right = (row.right() - 9.0).max(left);
    let width = (right - left).max(0.0);
    let keys = key_width.min(width * 0.55);
    let gap = if keys > 0.0 {
        6.0_f32.min(width - keys)
    } else {
        0.0
    };
    let split = right - keys;
    (
        egui::Rect::from_min_max(
            egui::pos2(left, row.top() + 8.0),
            egui::pos2(split - gap, row.bottom() - 3.0),
        ),
        egui::Rect::from_min_max(
            egui::pos2(split, row.top() + 3.0),
            egui::pos2(right, row.bottom() - 3.0),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolled_rows_keep_text_and_keycaps_inside_viewport() {
        for scale in [1.0, 1.5] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            let mut viewport = egui::Rect::NOTHING;
            let mut rows = Vec::new();
            // Run twice so ScrollArea has measured its content before applying
            // the offset. Both ends then contain fully hidden and partial rows.
            for _ in 0..2 {
                rows.clear();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(420.0, 300.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            ui.label("Popup header");
                            egui::ScrollArea::vertical()
                                .max_height(110.0)
                                .vertical_scroll_offset(61.0)
                                .show(ui, |ui| {
                                    viewport = ui.clip_rect();
                                    for index in 0..10 {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(300.0, 38.0),
                                            egui::Sense::click(),
                                        );
                                        rows.push(rect);
                                        ui.painter().rect_filled(
                                            rect,
                                            7.0,
                                            Color32::from_rgb(100, 100, 108),
                                        );
                                        paint_row_contents(
                                            ui,
                                            rect,
                                            "copy",
                                            &format!("Action{index}"),
                                            &format!("Key{index}"),
                                        );
                                    }
                                });
                            ui.label("Search field");
                        });
                    },
                );
                output.textures_delta.clear();
                let mut visible_labels = 0;
                let mut visible_keys = 0;
                for clipped in &output.shapes {
                    if let egui::Shape::Rect(rect) = &clipped.shape
                        && (rect.fill == Color32::from_rgb(100, 100, 108)
                            || rect.fill == Color32::from_white_alpha(12))
                        && clipped.clip_rect.is_positive()
                    {
                        assert!(viewport.contains_rect(clipped.clip_rect));
                    }
                    let egui::Shape::Text(text) = &clipped.shape else {
                        continue;
                    };
                    let value = text.galley.text();
                    let index = value
                        .strip_prefix("Action")
                        .or_else(|| value.strip_prefix("Key"))
                        .and_then(|value| value.parse::<usize>().ok());
                    let Some(index) = index else { continue };
                    if clipped.clip_rect.is_positive() {
                        assert!(
                            viewport.contains_rect(clipped.clip_rect),
                            "{value} escapes scroll viewport at scale {scale}: {:?} vs {viewport:?}",
                            clipped.clip_rect
                        );
                    }
                    let visible = clipped
                        .clip_rect
                        .intersect(text.visual_bounding_rect())
                        .is_positive();
                    if !rows[index].intersects(viewport) {
                        assert!(!visible, "offscreen {value} painted at scale {scale}");
                    }
                    if visible {
                        visible_labels += usize::from(value.starts_with("Action"));
                        visible_keys += usize::from(value.starts_with("Key"));
                    }
                }
                assert!(visible_labels > 0 && visible_keys > 0);
                assert!(rows.first().unwrap().bottom() < viewport.top());
                assert!(rows.last().unwrap().top() > viewport.bottom());
            }
        }
    }

    #[test]
    fn long_shortcut_aliases_fit_a_narrow_row() {
        let shortcut = "Ctrl Plus / Ctrl Equals / Ctrl Shift Plus / Ctrl Shift Equals";
        assert_eq!(primary_shortcut(shortcut), "Ctrl Plus");
        for pixels_per_point in [1.0, 1.5] {
            // egui uses logical points; transform the result to physical pixels
            // to check that the disjoint regions remain disjoint at each scale.
            let row = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(180.0, 38.0));
            let (label, keys) = row_text_rects(row, 500.0);
            assert!(label.right() * pixels_per_point <= keys.left() * pixels_per_point);
            assert!(keys.right() <= row.right());
            assert!(label.width() > 0.0);
        }
    }
}

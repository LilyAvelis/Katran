use super::i18n::{is_english, tr};
use super::*;

fn action_matches(action: &Value, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return !action["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("sort_"));
    }
    let haystack = format!(
        "{} {} {}",
        action["label"].as_str().unwrap_or(""),
        action["group"].as_str().unwrap_or(""),
        action["search_terms"].as_str().unwrap_or(""),
    )
    .to_lowercase();
    query.split_whitespace().all(|term| haystack.contains(term))
}

impl Shelf {
    pub(super) fn palette(&mut self, ctx: &egui::Context) {
        if !self.actions {
            return;
        }
        let catalogue = self.call(Command::Actions);
        let in_bull = self.upscale.collection_can_fullscreen();
        let actions: Vec<_> = catalogue
            .as_array()
            .into_iter()
            .flatten()
            .filter(|action| in_bull || action["id"] != "collection_fullscreen")
            .filter(|a| action_matches(a, &self.action_query))
            .cloned()
            .collect();
        let count = actions.len();
        self.action_selected = self.action_selected.min(count.saturating_sub(1));
        if self.shortcut_editor.is_none()
            && shortcuts_ui::take_assignment_key(ctx)
            && let Some(action) = actions.get(self.action_selected)
        {
            let compact = self.call(Command::GetSettings)["settings_summon_popover"] == true;
            let editor = shortcuts_ui::Editor::new(
                action["id"].as_str().unwrap(),
                action["label"].as_str().unwrap(),
                action["shortcut"].as_str().unwrap_or(""),
            );
            self.shortcut_editor = Some(if compact {
                editor.with_confirmation()
            } else {
                editor
            });
        }
        let raised = self
            .shortcut_editor
            .as_ref()
            .is_some_and(|editor| editor.uses_confirmation())
            && self.call(Command::GetSettings)["settings_raised_keycaps"] == true;
        if let Some(editor) = self.shortcut_editor.as_mut() {
            let compact = editor.uses_confirmation();
            let icon = action_icons::icon(&editor.id);
            let result = egui::Modal::new(egui::Id::new("shortcut-editor")).show(ctx, |ui| {
                if compact {
                    editor.render_popover(ui, icon, raised)
                } else {
                    editor.render(ui)
                }
            });
            let closed = if let Some(intent) = result.inner {
                self.api
                    .as_mut()
                    .is_some_and(|api| shortcuts_ui::apply(api, editor, intent))
            } else {
                compact && result.should_close()
            };
            if closed {
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
        let trigger = (!in_bull)
            .then(|| {
                ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new("actions-button-rect")))
            })
            .flatten();
        let anchor = trigger
            .map(|rect| rect.right_top() + egui::vec2(0.0, -6.0))
            .unwrap_or(ctx.content_rect().right_bottom() - egui::vec2(8.0, 40.0));
        let mut optical_slot = None;
        let result = egui::Modal::new(egui::Id::new("actions"))
            .area(
                egui::Area::new(egui::Id::new("actions")).order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_BOTTOM)
                    .fixed_pos(anchor),
            )
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                optical_slot=Some((ui.painter().clone(),ui.painter().add(egui::Shape::Noop)));
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Действия", "Actions")).strong().size(14.0));
                    ui.weak(if self.state["selected"].as_array().is_some_and(|selected| !selected.is_empty()) {
                        tr("Есть выделение", "Selection active")
                    } else {
                        tr("Ничего не выделено", "Nothing selected")
                    });
                });
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.action_query)
                        .hint_text(tr("Найти действие…", "Find an action…"))
                        .desired_width(width - 12.0),
                );
                if self.focus_actions {
                    search.request_focus();
                    self.focus_actions = false;
                }
                if search.changed() {
                    self.action_selected = 0;
                    self.scroll_actions = true;
                }
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
                            let tooltip=hint.map(|hint|if is_english() { format!("Sequence: {hint} · comma first, then the key") } else { format!("Последовательность: {hint} · сначала запятая, затем клавиша") }).unwrap_or_else(||shortcut.to_owned());
                            let enabled = a["enabled"] != false;
                            let response = ui.scope(|ui| {
                            if !enabled { ui.visuals_mut().override_text_color = Some(theme_ui::color(ctx,theme_ui::Role::Muted,Color32::from_gray(185))); }
                            crate::action_row::ActionRow::new(
                                crate::action_row::ActionPresentation {
                                    label: a["label"].as_str().unwrap_or(""),
                                    icon: action_icons::icon(a["id"].as_str().unwrap_or("")),
                                    primary_shortcut: display,
                                    full_shortcut_tooltip: &tooltip,
                                },
                            )
                            .show(ui, index == self.action_selected)
                            }).inner;
                            let response = if !enabled { response.on_hover_text(a["disabled_reason"].as_str().unwrap_or(tr("Недоступно", "Unavailable"))) } else { response };
                            #[cfg(test)]
                            ctx.data_mut(|data| data.insert_temp(egui::Id::new(("test-action-row", a["id"].as_str().unwrap_or(""))), response.rect));
                            if index == self.action_selected && self.scroll_actions {
                                response.scroll_to_me(None);
                                self.scroll_actions = false;
                            }
                            if response.clicked() {
                                self.action_selected = index;
                                if enabled {
                                    picked = a["id"].as_str().map(str::to_owned);
                                }
                            }
                        }
                        if actions.is_empty() {
                            ui.label(tr("Ничего не найдено", "Nothing found"));
                        }
                    });
                ui.separator();
                ui.weak(tr("↑↓ выбрать · Enter выполнить · Esc закрыть", "↑↓ select · Enter run · Esc close"));
            });
        if let Some((painter, index)) = optical_slot
            && let Some(shape) = theme_ui::optical_shape(ctx, result.response.rect, 12.0)
        {
            painter.set(index, shape);
        }
        #[cfg(test)]
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new("test-actions-popup-rect"),
                result.response.rect,
            )
        });
        if enter && picked.is_none() {
            picked = actions
                .get(self.action_selected)
                .filter(|a| a["enabled"] != false)
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

impl Shelf {
    pub(super) fn sort_menu(&mut self, ctx: &egui::Context) {
        if !self.sorting_input.is_open() {
            return;
        }
        let catalogue = self.call(Command::SortActions);
        let anchor = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("sorting-button-rect")))
            .map(|r| r.right_top() + egui::vec2(0.0, -6.0))
            .unwrap_or(ctx.content_rect().right_bottom() - egui::vec2(8.0, 40.0));
        let selected = self.sorting_input.selected();
        let scroll = self.sorting_input.take_scroll();
        let mut picked = None;
        let mut optical_slot = None;
        let response = egui::Modal::new(egui::Id::new("sorting-menu"))
            .area(
                egui::Area::new(egui::Id::new("sorting-menu"))
                    .order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_BOTTOM)
                    .fixed_pos(anchor),
            )
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                optical_slot = Some((ui.painter().clone(), ui.painter().add(egui::Shape::Noop)));
                ui.set_width((ctx.content_rect().width() - 40.0).clamp(200.0, 420.0));
                ui.heading(tr("Сортировка", "Sorting"));
                ui.weak(tr(
                    "Клавиша — выбрать · Shift — обратный порядок · , / Esc — закрыть",
                    "Key — select · Shift — reverse order · , / Esc — close",
                ));
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 160.0).clamp(80.0, 420.0))
                    .show(ui, |ui| {
                        for (index, item) in catalogue
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|a| a["id"].as_str().is_some_and(|id| id.starts_with("sort_")))
                            .enumerate()
                        {
                            let id = item["id"].as_str().unwrap();
                            let hint = sorting_ui::sequence_hint(id)
                                .unwrap_or("")
                                .trim_start_matches(", ");
                            let row = crate::action_row::ActionRow::new(
                                crate::action_row::ActionPresentation {
                                    label: item["label"].as_str().unwrap_or(id),
                                    icon: action_icons::icon(id),
                                    primary_shortcut: hint,
                                    full_shortcut_tooltip: hint,
                                },
                            )
                            .show(ui, index == selected);
                            if index == selected && scroll {
                                row.scroll_to_me(None);
                            }
                            if row.clicked() {
                                picked = Some(id.to_owned());
                            }
                        }
                    });
            });
        if let Some((painter, index)) = optical_slot
            && let Some(shape) = theme_ui::optical_shape(ctx, response.response.rect, 12.0)
        {
            painter.set(index, shape);
        }
        if let Some(id) = picked {
            self.sorting_input.close();
            self.run(id, ctx);
        } else if response.should_close() {
            self.sorting_input.close();
        }
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn actions_searches_group_aliases_and_keeps_sorting_out_of_empty_list() {
        let copy = serde_json::json!({"id":"transfer_copy","label":"Копировать в Folder","group":"Полка ↔ Folder","search_terms":"папка импорт копия"});
        assert!(action_matches(&copy, "папка копия"));
        assert!(action_matches(&copy, "folder"));
        assert!(!action_matches(&copy, "удалить"));
        let sort = serde_json::json!({"id":"sort_name","label":"Сортировать по имени","group":"Сортировка","search_terms":""});
        assert!(!action_matches(&sort, ""));
        assert!(action_matches(&sort, "сортировка"));
    }
}

#[cfg(test)]
mod video_action_tests {
    use super::*;
    use katran::interaction::SelectionMode;

    #[test]
    fn disabled_video_action_does_not_run_but_allows_shortcut_recording() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("synthetic.mp4"), b"synthetic video").unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        let page = app.call(Command::BrowseShelfSorted {
            query: String::new(),
            limit: 10,
            cursor: None,
        });
        let id = page["events"][0]["id"].as_i64().unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let catalogue = app.call(Command::Actions);
        let action = catalogue
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == "capture_video_frame")
            .unwrap();
        assert_eq!(action["enabled"], false);
        app.actions = true;
        app.action_query = "snapshot".into();
        app.action_selected = 0;
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        for _ in 0..2 {
            let mut out = ctx.run_ui(input(vec![]), |_| app.palette(&ctx));
            out.textures_delta.clear();
        }
        let rect: egui::Rect = ctx.data(|data| {
            data.get_temp(egui::Id::new(("test-action-row", "capture_video_frame")))
                .unwrap()
        });
        for pressed in [true, false] {
            let mut out = ctx.run_ui(
                input(vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]),
                |_| app.palette(&ctx),
            );
            out.textures_delta.clear();
        }
        assert!(app.actions && app.job.is_none());
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        let mut out = ctx.run_ui(
            input(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]),
            |_| app.palette(&ctx),
        );
        out.textures_delta.clear();
        assert!(app.actions && app.job.is_none());
        let mut out = ctx.run_ui(
            input(vec![key(
                egui::Key::Comma,
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            )]),
            |_| app.palette(&ctx),
        );
        out.textures_delta.clear();
        assert_eq!(
            app.shortcut_editor
                .as_ref()
                .map(|editor| editor.id.as_str()),
            Some("capture_video_frame")
        );
        assert!(!app.shortcut_editor.as_ref().unwrap().uses_confirmation());
        app.shortcut_editor = None;
        let mut settings: katran::settings::Settings =
            serde_json::from_value(app.call(Command::GetSettings)).unwrap();
        settings.settings_summon_popover = true;
        app.call(Command::SetSettings { settings });
        let modifiers = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let mut out = ctx.run_ui(
            input(vec![
                egui::Event::Key {
                    key: egui::Key::Comma,
                    physical_key: Some(egui::Key::Comma),
                    pressed: false,
                    repeat: false,
                    modifiers,
                },
                key(egui::Key::Comma, modifiers),
            ]),
            |_| app.palette(&ctx),
        );
        out.textures_delta.clear();
        assert!(app.shortcut_editor.as_ref().unwrap().uses_confirmation());
    }
}

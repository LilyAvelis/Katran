//! Actions-style tag picker. Domain operations remain dispatcher commands.
use super::i18n::tr;
use eframe::egui::{self, Color32, RichText};
use katran::interaction::ShelfTab;
use katran::tag_slots::Slot;

/// Reserved modal close keys: consume their paired typed character, not Paste.
fn take_tag_close(input: &mut egui::InputState, editing: bool) -> bool {
    let mut close = false;
    let mut paired_text = false;
    input.events.retain(|event| match event {
        egui::Event::Key {
            key,
            physical_key,
            pressed,
            repeat,
            modifiers,
            ..
        } if (physical_key.unwrap_or(*key) == egui::Key::Backslash
            || editing && physical_key.unwrap_or(*key) == egui::Key::Slash)
            && modifiers.is_none() =>
        {
            paired_text = *pressed;
            close |= *pressed && !*repeat;
            false
        }
        egui::Event::Key { .. } => {
            paired_text = false;
            true
        }
        egui::Event::Text(text) if paired_text && text.chars().count() == 1 => {
            paired_text = false;
            false
        }
        _ => true,
    });
    close
}

struct Editor {
    tab: ShelfTab,
    slot: Slot,
    dirty: bool,
    revision: String,
    conflicted: bool,
}

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
    tab: ShelfTab,
    editor: Option<Editor>,
    editing: bool,
    save_requested: bool,
    revision: Option<String>,
    reload_requested: bool,
}
#[derive(Clone, Copy)]
pub enum Intent {
    Filter(Option<u8>),
    Assign(Option<u8>),
    Retry,
}
impl State {
    fn clear_popup_focus(ctx: &egui::Context) {
        egui::Popup::close_all(ctx);
        if let Some(id) = ctx.memory(|memory| memory.focused()) {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
    }

    /// Dismiss only an active tag modal. Keep failed/unsaved draft and its revision.
    pub(super) fn close(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        Self::clear_popup_focus(ctx);
        self.open = false;
        self.editing = false;
        self.save_requested = false;
        self.reload_requested = false;
        if self.editor.as_ref().is_some_and(|editor| !editor.dirty) {
            self.editor = None;
        }
        ctx.request_repaint();
    }

    pub fn start(&mut self, slots: Vec<Slot>) {
        let editor = self.editor.take().filter(|editor| editor.dirty);
        let error = if editor.is_some() {
            std::mem::take(&mut self.error)
        } else {
            String::new()
        };
        *self = Self {
            open: true,
            slots,
            focus: false,
            fresh: true,
            scroll: true,
            editor,
            error,
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
        if ctx.input_mut(|input| take_tag_close(input, self.editing)) {
            self.close(ctx);
            return None;
        }
        if self.editing {
            let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if escape {
                Self::clear_popup_focus(ctx);
                self.editing = false;
                self.save_requested = false;
                self.reload_requested = false;
                if self.editor.as_ref().is_some_and(|editor| !editor.dirty) {
                    self.editor = None;
                }
                ctx.request_repaint();
                return None;
            }
            let width = (ctx.content_rect().width() - 28.0).clamp(240.0, 420.0);
            let anchor = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("tags-button-rect")))
                .map(|r| r.right_top() + egui::vec2(0.0, -6.0))
                .unwrap_or(ctx.content_rect().right_bottom() - egui::vec2(8.0, 40.0));
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
                    if let Some(editor) = self.editor.as_mut() {
                        ui.label(
                            RichText::new(format!("{} · {}", tr("Тег", "Tag"), editor.slot.number))
                                .strong(),
                        );
                        let before = editor.slot.clone();
                        egui::ScrollArea::vertical()
                            .max_height((ctx.content_rect().height() - 140.0).max(80.0))
                            .show(ui, |ui| {
                                ui.add_enabled_ui(!editor.conflicted, |ui| {
                                    super::tag_settings_ui::edit_controls(ui, &mut editor.slot)
                                });
                            });
                        if editor.slot != before {
                            editor.dirty = true;
                            self.save_requested = true;
                        }
                        if editor.dirty && !self.error.is_empty() {
                            ui.colored_label(
                                Color32::LIGHT_RED,
                                format!("{}: {}", tr("Не сохранено", "Not saved"), self.error),
                            );
                            if !editor.conflicted
                                && ui
                                    .button(tr("Повторить сохранение", "Retry save"))
                                    .clicked()
                            {
                                self.save_requested = true;
                            }
                            if editor.conflicted
                                && ui
                                    .button(tr(
                                        "Отменить черновик и обновить",
                                        "Discard draft and reload",
                                    ))
                                    .clicked()
                            {
                                self.reload_requested = true;
                            }
                        }
                        ui.weak(tr("Esc · назад к тегам", "Esc · back to tags"));
                    }
                });
            if let Some((painter, index)) = optical
                && let Some(shape) = super::theme_ui::optical_shape(ctx, result.response.rect, 12.0)
            {
                painter.set(index, shape);
            }
            if result.should_close() {
                self.close(ctx);
            }
            return None;
        }
        if self.fresh {
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("tag-menu-search")));
            self.fresh = false;
        }
        let mut rows: Vec<(String, Intent, bool)> = Vec::new();
        rows.push((
            tr("Фильтр · Все", "Filter · All").into(),
            Intent::Filter(None),
            true,
        ));
        for slot in &self.slots {
            rows.push((
                format!(
                    "{} · {} · {}{}",
                    tr("Фильтр", "Filter"),
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
                format!(
                    "{} · {} · {}",
                    tr("Назначить", "Assign"),
                    slot.number,
                    slot.name
                ),
                Intent::Assign(Some(slot.number)),
                eligible,
            ));
        }
        rows.push((
            format!(
                "{} · Ctrl 0 · {selection_count}",
                tr("Снять теги", "Clear tags")
            ),
            Intent::Assign(None),
            has_tag,
        ));
        if let Some((ids, _)) = &self.retry {
            rows.push((
                format!("{} · {}", tr("Повторить ошибки", "Retry failed"), ids.len()),
                Intent::Retry,
                true,
            ));
        }
        let query = self.query.to_lowercase();
        rows.retain(|(label, _, _)| label.to_lowercase().contains(&query));
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        if super::shortcuts_ui::take_assignment_key(ctx) {
            let number = rows
                .get(self.selected)
                .and_then(|(_, intent, _)| match intent {
                    Intent::Filter(Some(number)) | Intent::Assign(Some(number)) => Some(*number),
                    _ => None,
                });
            if let Some(number) = number {
                if self.editor.as_ref().is_some_and(|editor| {
                    editor.dirty && (editor.tab != self.tab || editor.slot.number != number)
                }) {
                    self.error = tr(
                        "Сначала сохраните предыдущий тег",
                        "Save the previous tag first",
                    )
                    .into();
                } else if let Some(slot) = self
                    .slots
                    .iter()
                    .find(|slot| slot.number == number)
                    .cloned()
                {
                    if !self.editor.as_ref().is_some_and(|editor| editor.dirty)
                        && let Some(revision) = self.revision.clone()
                    {
                        self.editor = Some(Editor {
                            tab: self.tab,
                            slot,
                            dirty: false,
                            revision,
                            conflicted: false,
                        });
                    }
                    self.editing = self.editor.is_some();
                    ctx.request_repaint();
                }
            }
            return None;
        }
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
                if shortcut == Some((0, false)) {
                    take_tag_close(input, true);
                } else if shortcut.is_some() {
                    super::shortcuts_ui::discard(input);
                }
                shortcut
            })
        };
        if let Some((number, assign)) = shortcut {
            if number == 0 && assign {
                if !has_tag {
                    self.error = tr(
                        "У выбранных объектов нет тегов",
                        "The selected items have no tags",
                    )
                    .into();
                } else {
                    self.close(ctx);
                    return Some(Intent::Assign(None));
                }
            } else if number == 0 {
                self.close(ctx);
                return Some(Intent::Filter(None));
            }
            if self.slots.iter().any(|slot| slot.number == number) {
                if assign && !eligible {
                    self.error = tr(
                        "Для назначения выделите только закреплённые объекты",
                        "Select only pinned items to assign a tag",
                    )
                    .into();
                } else {
                    self.close(ctx);
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
                    RichText::new(tr("Теги · фильтр и назначение", "Tags · filter and assign"))
                        .strong()
                        .size(13.0),
                );
                ui.weak(summary);
                if !self.error.is_empty() {
                    ui.colored_label(Color32::LIGHT_RED, &self.error);
                }
                if !eligible {
                    ui.weak(tr(
                        "Для назначения все выбранные объекты должны быть закреплены.",
                        "All selected items must be pinned to assign a tag.",
                    ));
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
                            ui.label(tr("Ничего не найдено", "Nothing found"));
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id(egui::Id::new("tag-menu-search"))
                            .hint_text(tr("Поиск тегов…", "Search tags…"))
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
            self.close(ctx);
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
            Ok(slots) => {
                self.tag_menu.start(slots);
                self.tag_menu.tab =
                    serde_json::from_value(self.state["tab"].clone()).unwrap_or_default();
                self.tag_menu.revision = response["revision"].as_str().map(str::to_owned);
            }
            Err(_) => {
                if self.message.is_empty() {
                    self.message = tr("Не удалось загрузить теги", "Could not load tags").into();
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
            tr("Ничего не выбрано", "Nothing selected").to_owned()
        } else if tags.len() > 1 {
            format!(
                "{}: {} · {}",
                tr("Выбрано", "Selected"),
                ids.len(),
                tr("разные теги", "different tags")
            )
        } else if let Some(Some(number)) = tags.iter().next().copied() {
            let name = self
                .tag_menu
                .slots
                .iter()
                .find(|s| s.number == number)
                .map(|s| s.name.as_str())
                .unwrap_or(tr("Тег", "Tag"));
            format!("{}: {} · {}", tr("Выбрано", "Selected"), ids.len(), name)
        } else {
            format!(
                "{}: {} · {}",
                tr("Выбрано", "Selected"),
                ids.len(),
                tr("без тега", "no tag")
            )
        };
        if let Some(intent) =
            self.tag_menu
                .show(ctx, eligible, ids.len(), has_tag, &summary, active)
        {
            let numeric = matches!(intent, Intent::Filter(Some(_)) | Intent::Assign(Some(_)))
                || matches!(intent, Intent::Retry)
                    && self
                        .tag_menu
                        .retry
                        .as_ref()
                        .is_some_and(|(_, slot)| slot.is_some());
            if numeric && !self.tag_menu_revision_current() {
                self.tag_menu.open = true;
                return;
            }
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
        if self.tag_menu.reload_requested {
            self.tag_menu.reload_requested = false;
            self.tag_menu.editor = None;
            self.tag_menu.editing = false;
            self.tag_menu.error.clear();
            self.open_tag_menu();
        }
        if self.tag_menu.save_requested {
            self.tag_menu.save_requested = false;
            if let Some(editor) = self.tag_menu.editor.as_ref() {
                let tab = editor.tab;
                let slot = editor.slot.clone();
                let revision = editor.revision.clone();
                if editor.conflicted {
                    return;
                }
                let before = self.call(katran::Command::Interaction);
                let result = self
                    .api
                    .as_mut()
                    .map(|api| {
                        api.execute(katran::Request {
                            api_version: 1,
                            request_id: "tag-menu-save".into(),
                            command: katran::Command::SaveTagSlotChecked {
                                tab,
                                slot: slot.clone(),
                                expected_revision: revision,
                            },
                        })
                    })
                    .unwrap_or_default();
                if result["ok"] != true {
                    let conflict = result["error"]["code"] == "tag_slots_changed";
                    if let Some(editor) = self.tag_menu.editor.as_mut() {
                        editor.conflicted = conflict;
                    }
                    self.tag_menu.error = if conflict {
                        tr(
                            "Теги изменились в другом окне; черновик не сохранён",
                            "Tags changed elsewhere; the draft was not saved",
                        )
                        .into()
                    } else {
                        result["error"]["message"]
                            .as_str()
                            .unwrap_or(tr("Не удалось сохранить тег", "Could not save tag"))
                            .into()
                    };
                } else {
                    let next_revision = result["result"]["revision"].as_str().map(str::to_owned);
                    if let Some(editor) = self.tag_menu.editor.as_mut() {
                        editor.dirty = false;
                        if let Some(revision) = next_revision.clone() {
                            editor.revision = revision;
                        }
                    }
                    self.tag_menu.revision = next_revision;
                    if self.tag_menu.tab == tab
                        && let Some(old) = self
                            .tag_menu
                            .slots
                            .iter_mut()
                            .find(|old| old.number == slot.number)
                    {
                        *old = slot;
                    }
                    self.tag_menu.error.clear();
                    let after = self.call(katran::Command::Interaction);
                    self.accept_tag_style_update(&before, after);
                    ctx.request_repaint();
                }
            }
        }
    }
    fn tag_menu_revision_current(&mut self) -> bool {
        let tab = self.tag_menu.tab;
        if serde_json::from_value::<ShelfTab>(self.state["tab"].clone()).ok() != Some(tab) {
            self.tag_menu.error = tr(
                "Вкладка изменилась; откройте теги снова",
                "Tab changed; reopen tags",
            )
            .into();
            return false;
        }
        let response = self.call(katran::Command::ListTagSlotsForTab { tab });
        let revision = response["revision"].as_str().map(str::to_owned);
        if revision.is_some() && revision == self.tag_menu.revision {
            return true;
        }
        if let (Some(revision), Ok(slots)) = (
            revision,
            serde_json::from_value::<Vec<Slot>>(response["slots"].clone()),
        ) {
            self.tag_menu.revision = Some(revision);
            self.tag_menu.slots = slots;
            self.tag_menu.selected = 0;
            self.tag_menu.retry = None;
            self.tag_menu.error = tr(
                "Теги изменились; выберите действие снова",
                "Tags changed; choose the action again",
            )
            .into();
        } else {
            self.tag_menu.error = tr("Не удалось проверить теги", "Could not check tags").into();
        }
        false
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
                .map(|v| {
                    format!(
                        "{}: {}",
                        v["id"],
                        v["outcome"].as_str().unwrap_or(tr("ошибка", "error"))
                    )
                })
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
    #[test]
    fn closing_active_tag_modal_clears_popup_focus_but_keeps_dirty_draft() {
        use eframe::egui;
        use katran::interaction::ShelfTab;
        let ctx = egui::Context::default();
        let popup = egui::Id::new("test-tag-color-popup");
        let field = egui::Id::new("test-tag-name");
        let mut state = super::State::default();
        state.start(vec![slot(1)]);
        state.tab = ShelfTab::Text;
        state.editor = Some(super::Editor {
            tab: ShelfTab::Text,
            slot: slot(1),
            dirty: true,
            revision: "original-revision".into(),
            conflicted: true,
        });
        state.editing = true;
        state.error = "Synthetic conflict".into();
        state.save_requested = true;
        state.reload_requested = true;
        egui::Popup::open_id(&ctx, popup);
        ctx.memory_mut(|m| m.request_focus(field));
        state.close(&ctx);
        assert!(!state.open && !state.editing && !state.save_requested && !state.reload_requested);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert!(!ctx.memory(|m| m.has_focus(field)));
        let editor = state.editor.as_ref().unwrap();
        assert!(editor.dirty && editor.conflicted);
        assert_eq!(editor.revision, "original-revision");
        state.start(vec![slot(1)]);
        assert!(state.open && !state.editing);
        assert_eq!(state.error, "Synthetic conflict");
        assert_eq!(state.editor.as_ref().unwrap().revision, "original-revision");
        state.open = false;
        egui::Popup::open_id(&ctx, popup);
        ctx.memory_mut(|m| m.request_focus(field));
        state.close(&ctx);
        assert!(egui::Popup::is_id_open(&ctx, popup));
        assert!(ctx.memory(|m| m.has_focus(field)));
    }

    #[test]
    fn slash_or_backslash_close_editor_and_palette_preserving_paste_and_draft() {
        use eframe::egui::{self, Key, Modifiers};
        use katran::interaction::ShelfTab;
        for editing in [false, true] {
            for key in [Key::Slash, Key::Backslash] {
                if !editing && key == Key::Slash {
                    continue;
                }
                let ctx = egui::Context::default();
                let mut state = super::State::default();
                state.start(vec![slot(1)]);
                state.editor = Some(super::Editor {
                    tab: ShelfTab::Images,
                    slot: slot(1),
                    dirty: true,
                    revision: "r1".into(),
                    conflicted: false,
                });
                state.error = "Save failed".into();
                state.editing = editing;
                let field = egui::Id::new("tag-editor-field");
                ctx.memory_mut(|m| m.request_focus(field));
                egui::Popup::open_id(&ctx, egui::Id::new("tag-popup"));
                let mut remaining = Vec::new();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events: vec![
                            egui::Event::Key {
                                key,
                                physical_key: Some(key),
                                pressed: true,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            },
                            egui::Event::Text("/".into()),
                            egui::Event::Paste("/kept".into()),
                        ],
                        ..Default::default()
                    },
                    |ui| {
                        assert!(state.show(ui.ctx(), false, 0, false, "", None).is_none());
                        remaining = ui.ctx().input(|i| i.events.clone());
                    },
                );
                output.textures_delta.clear();
                assert!(!state.open && !state.editing && !state.save_requested);
                assert!(state.editor.as_ref().unwrap().dirty);
                assert_eq!(state.error, "Save failed");
                assert!(!egui::Popup::is_any_open(&ctx));
                assert!(!ctx.memory(|m| m.has_focus(field)));
                assert!(
                    remaining
                        .iter()
                        .any(|event| matches!(event, egui::Event::Paste(text) if text == "/kept"))
                );
                assert!(
                    !remaining
                        .iter()
                        .any(|event| matches!(event, egui::Event::Text(_)))
                );
            }
        }
    }

    #[test]
    fn escape_leaves_editor_for_palette_and_closes_nested_popup() {
        use eframe::egui::{self, Key, Modifiers};
        let ctx = egui::Context::default();
        let mut state = super::State::default();
        state.start(vec![slot(1)]);
        state.editor = Some(super::Editor {
            tab: katran::interaction::ShelfTab::Images,
            slot: slot(1),
            dirty: true,
            revision: "r1".into(),
            conflicted: false,
        });
        state.editing = true;
        egui::Popup::open_id(&ctx, egui::Id::new("material-combo"));
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: Some(Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                state.show(ui.ctx(), false, 0, false, "", None);
            },
        );
        out.textures_delta.clear();
        assert!(state.open && !state.editing && state.editor.as_ref().unwrap().dirty);
        assert!(!egui::Popup::is_any_open(&ctx));
    }

    #[test]
    fn ctrl_shift_slash_is_not_consumed_as_modal_close() {
        use eframe::egui::{self, Key, Modifiers};
        let ctx = egui::Context::default();
        let mut state = super::State::default();
        state.start(vec![slot(1)]);
        state.editor = Some(super::Editor {
            tab: katran::interaction::ShelfTab::Images,
            slot: slot(1),
            dirty: false,
            revision: "r1".into(),
            conflicted: false,
        });
        state.editing = true;
        let mut retained = false;
        let mut out = ctx.run_ui(egui::RawInput { events: vec![egui::Event::Key { key: Key::Slash, physical_key: Some(Key::Slash), pressed: true, repeat: false, modifiers: Modifiers::CTRL | Modifiers::SHIFT }], ..Default::default() }, |ui| {
            state.show(ui.ctx(), false, 0, false, "", None);
            retained = ui.ctx().input(|i| i.events.iter().any(|event| matches!(event, egui::Event::Key { key: Key::Slash, pressed: true, modifiers, .. } if *modifiers == (Modifiers::CTRL | Modifiers::SHIFT))));
        });
        out.textures_delta.clear();
        assert!(state.open && state.editing && retained);
    }
    #[test]
    fn palette_reloads_and_rejects_numeric_action_after_reorder() {
        use katran::{Command, interaction::ShelfTab};
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.state = app.call(Command::SetTab {
            tab: ShelfTab::Images,
        });
        app.open_tag_menu();
        let revision = app.tag_menu.revision.clone().unwrap();
        let mut draft = app.tag_menu.slots[0].clone();
        draft.name = "Draft".into();
        app.tag_menu.editor = Some(super::Editor {
            tab: ShelfTab::Images,
            slot: draft,
            dirty: true,
            revision: revision.clone(),
            conflicted: false,
        });
        let reordered = app.call(Command::ReorderTagSlots {
            tab: ShelfTab::Images,
            order: vec![2, 1, 3, 4, 5, 6, 7, 8, 9],
            expected_revision: revision,
        });
        assert!(!app.tag_menu_revision_current());
        assert_eq!(
            app.tag_menu.revision.as_deref(),
            reordered["revision"].as_str()
        );
        assert_eq!(app.tag_menu.editor.as_ref().unwrap().slot.name, "Draft");
        assert!(!app.tag_menu.error.is_empty());
        assert!(app.tag_menu_revision_current());
    }
    #[test]
    fn stale_compact_editor_cannot_overwrite_tag_after_reorder() {
        use katran::{Command, interaction::ShelfTab};
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.state = app.call(Command::SetTab {
            tab: ShelfTab::Images,
        });
        app.open_tag_menu();
        let old_revision = app.tag_menu.revision.clone().unwrap();
        let mut draft = app.tag_menu.slots[0].clone();
        draft.name = "Unsaved draft".into();
        app.tag_menu.editor = Some(super::Editor {
            tab: ShelfTab::Images,
            slot: draft,
            dirty: true,
            revision: old_revision.clone(),
            conflicted: false,
        });
        app.tag_menu.editing = true;
        let reordered = app.call(Command::ReorderTagSlots {
            tab: ShelfTab::Images,
            order: vec![2, 1, 3, 4, 5, 6, 7, 8, 9],
            expected_revision: old_revision,
        });
        assert!(reordered["revision"].is_string());
        let before = app.call(Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        });
        app.tag_menu.save_requested = true;
        let ctx = eframe::egui::Context::default();
        let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |_| {
            app.tag_menu_panel(&ctx)
        });
        out.textures_delta.clear();
        let editor = app.tag_menu.editor.as_ref().unwrap();
        assert!(editor.conflicted && editor.dirty);
        assert_eq!(editor.slot.name, "Unsaved draft");
        assert!(!app.tag_menu.error.is_empty());
        assert_eq!(
            app.call(Command::ListTagSlotsForTab {
                tab: ShelfTab::Images
            })["slots"],
            before["slots"]
        );
        app.tag_menu.save_requested = true;
        let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |_| {
            app.tag_menu_panel(&ctx)
        });
        out.textures_delta.clear();
        assert_eq!(
            app.call(Command::ListTagSlotsForTab {
                tab: ShelfTab::Images
            })["slots"],
            before["slots"]
        );
    }
    #[test]
    fn shell_shortcut_opens_compact_editor_without_settings_or_preview() {
        use eframe::egui::{self, Key, Modifiers};
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.state = app.call(katran::Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        app.open_tag_menu();
        app.tag_menu.selected = 1;
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            events: vec![egui::Event::Key {
                key: Key::Comma,
                physical_key: Some(Key::Comma),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL | Modifiers::SHIFT,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
        assert!(app.tag_menu.editing && app.tag_menu.open);
        assert_eq!(
            app.call(katran::Command::Interaction)["settings_open"],
            false
        );
        fn collect(shape: &egui::Shape, text: &mut String) {
            match shape {
                egui::Shape::Text(value) => {
                    text.push_str(&value.galley.job.text);
                    text.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, text);
                    }
                }
                _ => {}
            }
        }
        let mut text = String::new();
        for _ in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(940.0, 680.0),
                    )),
                    ..Default::default()
                },
                |ui| app.render(ui),
            );
            text.clear();
            for shape in &output.shapes {
                collect(&shape.shape, &mut text);
            }
            output.textures_delta.clear();
            if text.contains("Название") || text.contains("Name") {
                break;
            }
        }
        assert!(
            text.contains("Название") || text.contains("Name"),
            "editor controls missing; emitted text: {text:?}"
        );
        assert!(!text.contains("Предпросмотр материала") && !text.contains("Material preview"));
        assert!(
            !text.contains("Настройки тегов · Ctrl Shift")
                && !text.contains("Tag settings · Ctrl Shift")
        );
    }
    #[test]
    fn local_editor_persists_captured_scope_and_keeps_failed_draft() {
        use katran::{Command, interaction::ShelfTab};
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.state = app.call(Command::SetTab {
            tab: ShelfTab::Images,
        });
        let images = app.call(Command::ListTagSlotsForTab {
            tab: ShelfTab::Images,
        });
        let text = app.call(Command::ListTagSlotsForTab {
            tab: ShelfTab::Text,
        });
        let slot: katran::tag_slots::Slot =
            serde_json::from_value(images["slots"][0].clone()).unwrap();
        app.tag_menu.start(vec![slot.clone()]);
        app.tag_menu.tab = ShelfTab::Images;
        app.tag_menu.editing = true;
        app.tag_menu.editor = Some(super::Editor {
            tab: ShelfTab::Images,
            slot,
            dirty: true,
            revision: images["revision"].as_str().unwrap().to_owned(),
            conflicted: false,
        });
        app.tag_menu.editor.as_mut().unwrap().slot.color = "invalid".into();
        app.tag_menu.save_requested = true;
        let ctx = eframe::egui::Context::default();
        let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |_| {
            app.tag_menu_panel(&ctx)
        });
        out.textures_delta.clear();
        assert!(app.tag_menu.editor.as_ref().unwrap().dirty);
        assert!(!app.tag_menu.error.is_empty());
        assert_eq!(app.call(Command::Interaction)["settings_open"], false);
        app.tag_menu.editor.as_mut().unwrap().slot.color = "#123456".into();
        app.tag_menu.save_requested = true;
        let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |_| {
            app.tag_menu_panel(&ctx)
        });
        out.textures_delta.clear();
        assert!(!app.tag_menu.editor.as_ref().unwrap().dirty);
        assert_eq!(app.tag_menu.slots[0].color, "#123456");
        app.tag_menu.editor.as_mut().unwrap().slot.color = "#654321".into();
        app.tag_menu.save_requested = true;
        let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |_| {
            app.tag_menu_panel(&ctx)
        });
        out.textures_delta.clear();
        assert!(!app.tag_menu.editor.as_ref().unwrap().dirty);
        assert_eq!(app.tag_menu.slots[0].color, "#654321");
        assert_eq!(
            app.call(Command::ListTagSlotsForTab {
                tab: ShelfTab::Images
            })["slots"][0]["color"],
            "#654321"
        );
        assert_eq!(
            app.call(Command::ListTagSlotsForTab {
                tab: ShelfTab::Text
            })["slots"],
            text["slots"]
        );
        drop(app);
        let mut reopened = katran::Service::open(root.path(), 100_000).unwrap();
        let persisted = reopened.execute(katran::Request {
            api_version: 1,
            request_id: "local-editor-restart".into(),
            command: Command::ListTagSlotsForTab {
                tab: ShelfTab::Images,
            },
        });
        assert_eq!(persisted["result"]["slots"][0]["color"], "#654321");
    }
    #[test]
    fn shortcut_edits_highlighted_tag_and_escape_returns_to_palette() {
        use eframe::egui::{self, Key, Modifiers};
        let ctx = egui::Context::default();
        let mut state = super::State::default();
        state.start(vec![slot(1), slot(2)]);
        state.revision = Some("test".into());
        state.tab = katran::interaction::ShelfTab::Text;
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        let frame = |state: &mut super::State, event| {
            let mut result = None;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![event],
                    ..Default::default()
                },
                |ui| result = state.show(ui.ctx(), true, 1, false, "", None),
            );
            out.textures_delta.clear();
            assert!(result.is_none());
        };
        frame(
            &mut state,
            key(Key::Comma, Modifiers::CTRL | Modifiers::SHIFT),
        );
        assert!(!state.editing); // All is not a tag.
        frame(
            &mut state,
            egui::Event::Key {
                key: Key::Comma,
                physical_key: Some(Key::Comma),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::CTRL | Modifiers::SHIFT,
            },
        );
        state.selected = 2;
        frame(
            &mut state,
            key(Key::Comma, Modifiers::CTRL | Modifiers::SHIFT),
        );
        assert!(state.editing && state.open);
        let editor = state.editor.as_ref().unwrap();
        assert_eq!(editor.slot.number, 2);
        assert_eq!(editor.tab, katran::interaction::ShelfTab::Text);
        frame(&mut state, key(Key::Escape, Modifiers::NONE));
        assert!(state.open && !state.editing);
    }
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
        app.state = app.call(katran::Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
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

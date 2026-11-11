//! Working-folder picker: presentation and dispatcher jobs only.
use super::*;
use katran::domain_jobs::JobId;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Search,
    History,
    Favorites,
}
#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    mode: Mode,
    query: String,
    items: Vec<PathBuf>,
    selected: usize,
    focus: bool,
    scroll: bool,
    revision: u64,
    due: Option<Instant>,
    job: Option<(JobId, u64)>,
    loading: bool,
    error: Option<String>,
    opened_frame: u64,
    pub last_refresh: Option<Instant>,
    pub recovery: Option<Vec<Value>>,
    pub recovery_root: Option<String>,
}
impl Shelf {
    fn workspace_cancel(&mut self) {
        if let Some((job_id, _)) = self.workspace.job.take() {
            self.call(Command::CancelJob { job_id });
        }
        self.workspace.revision = self.workspace.revision.wrapping_add(1);
        self.workspace.loading = false;
        self.workspace.due = None;
    }
    fn workspace_reload(&mut self, ctx: &egui::Context) {
        self.workspace_cancel();
        self.workspace.items.clear();
        self.workspace.selected = 0;
        self.workspace.error = None;
        if self.workspace.mode == Mode::Search {
            if !self.workspace.query.trim().is_empty() {
                self.workspace.loading = true;
                self.workspace.due = Some(Instant::now() + Duration::from_millis(120));
                ctx.request_repaint_after(Duration::from_millis(120));
            }
        } else {
            let query = self.workspace.query.clone();
            let command = if self.workspace.mode == Mode::History {
                Command::FolderHistory { query }
            } else {
                Command::FavoriteFolders { query }
            };
            let result = self.call(command);
            if result.is_null() {
                self.workspace.error = Some(self.message.clone());
            } else {
                self.workspace_set_items(&result);
            }
        }
    }
    fn workspace_set_items(&mut self, result: &Value) {
        self.workspace.items = result["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["path"].as_str().map(PathBuf::from))
            .collect();
        self.workspace.selected = self
            .workspace
            .selected
            .min(self.workspace.items.len().saturating_sub(1));
    }
    fn workspace_mode(&mut self, mode: Mode, ctx: &egui::Context) {
        self.workspace.mode = mode;
        self.workspace.query.clear();
        self.workspace.focus = true;
        self.workspace_reload(ctx);
    }
    fn workspace_close(&mut self) {
        self.workspace_cancel();
        self.workspace.open = false;
    }
    fn workspace_transfer_result(&mut self, result: &Value, import: bool) {
        let Some(items) = result["items"].as_array() else {
            return;
        };
        let mut lines = Vec::new();
        for item in items {
            let id = item["id"].as_i64().unwrap_or_default();
            let status = if import {
                if item["ok"] == true {
                    "добавлено"
                } else {
                    "ошибка"
                }
            } else {
                match item["status"].as_str().unwrap_or("failed") {
                    "copied" => "скопировано",
                    "moved" => "перемещено",
                    "pinned_copy" => "закреплено: создана копия",
                    "same_folder" => "уже в этой папке",
                    "source_not_moved" => "копия создана, источник сохранён",
                    "source_removed_shelf_retained" => {
                        "копия создана, исходный файл удалён; запись осталась на полке"
                    }
                    _ => "ошибка",
                }
            };
            let detail = item["error"]["message"]
                .as_str()
                .or_else(|| item["error"].as_str())
                .unwrap_or("");
            let name = item["path"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("#{id}"));
            lines.push(if detail.is_empty() {
                format!("{name}: {status}")
            } else {
                format!("{name}: {status} · {detail}")
            });
        }
        self.message = lines.join("; ");
        self.refresh();
    }
    fn workspace_transfer_input(&mut self, ctx: &egui::Context) {
        if self.workspace.open
            || self.actions
            || self.open_apps.open
            || self.save
            || self.shortcut_editor.is_some()
            || !self.pending_delete.is_empty()
            || self.state["settings_open"] == true
            || ctx
                .memory(|m| m.focused())
                .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
        {
            return;
        }
        if self.state["tab"] == "folder"
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::C)
            })
        {
            ctx.input_mut(|i| i.events.retain(|event| !matches!(event, egui::Event::Copy)));
            let ids = self.selected_ids();
            let result = self.call(Command::AddFolderFiles { ids });
            self.workspace_transfer_result(&result, true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::X)) {
            if let Some(items) = self.selection_snapshot() {
                let result = self.call(Command::StageTransfer { items, cut: true });
                if let Some(count) = result["staged"].as_u64() {
                    self.message = format!(
                        "Подготовлено к перемещению: {count} · откройте Folder и нажмите Ctrl V"
                    );
                }
            } else {
                self.message = "Выбор устарел; обновите полку".into();
            }
        } else if self.state["tab"] == "folder"
            && ctx.input_mut(|i| {
                let key = i.consume_key(egui::Modifiers::CTRL, egui::Key::V);
                let pasted = i
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Paste(_)));
                if key || pasted {
                    i.events
                        .retain(|event| !matches!(event, egui::Event::Paste(_)));
                }
                key || pasted
            })
        {
            let result = self.call(Command::PasteToFolder);
            self.workspace_transfer_result(&result, false);
        }
    }
    pub(super) fn workspace_input(&mut self, ctx: &egui::Context) {
        self.workspace_transfer_input(ctx);
        let search = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::K)
        });
        if search
            && self.shortcut_editor.is_none()
            && !self.save
            && self.pending_delete.is_empty()
            && self.state["settings_open"] != true
            && !self.open_apps.open
        {
            self.actions = false;
            self.workspace.open = true;
            self.workspace.opened_frame = ctx.cumulative_frame_nr();
            self.workspace_mode(Mode::Search, ctx);
        }
        if self.workspace.open {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::H)) {
                self.workspace_mode(Mode::History, ctx);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                self.workspace_mode(Mode::Favorites, ctx);
            }
        } else if !self.actions
            && !self.open_apps.open
            && !self.save
            && self.pending_delete.is_empty()
            && self.state["settings_open"] != true
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Period))
        {
            if self.state["tab"] == "folder" {
                let result = self.call(Command::ToggleFavoriteFolder);
                if let Some(favorite) = result["favorite"].as_bool() {
                    self.message = if favorite {
                        "Папка добавлена в избранное"
                    } else {
                        "Папка удалена из избранного"
                    }
                    .into();
                }
            } else {
                let ids = self.state["selected"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_i64)
                    .collect::<Vec<_>>();
                let ids = if ids.is_empty() {
                    self.state["cursor"].as_i64().into_iter().collect()
                } else {
                    ids
                };
                let result = self.call(Command::TogglePins { ids });
                if result.is_null() {
                    return;
                }
                let failures = result["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|item| !item["error"].is_null())
                    .count();
                self.message = if failures > 0 {
                    format!("Не удалось изменить закрепление: {failures}")
                } else {
                    "Закрепление обновлено".into()
                };
                self.refresh();
            }
        }
    }
    pub(super) fn workspace_tick(&mut self, ctx: &egui::Context) {
        if !self.workspace.open {
            return;
        }
        if let Some((job_id, revision)) = self.workspace.job {
            let result = self.call(Command::PollJob { job_id });
            match result["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(50))
                }
                Some("succeeded") => {
                    self.workspace.job = None;
                    if revision == self.workspace.revision {
                        self.workspace.loading = false;
                        self.workspace_set_items(&result["result"]);
                    }
                }
                _ => {
                    self.workspace.job = None;
                    if revision == self.workspace.revision {
                        self.workspace.loading = false;
                        self.workspace.error = Some(
                            result["error"]["message"]
                                .as_str()
                                .unwrap_or("Поиск недоступен")
                                .into(),
                        );
                    }
                }
            }
        }
        if self.workspace.due.is_some_and(|due| Instant::now() >= due)
            && self.workspace.job.is_none()
        {
            self.workspace.due = None;
            let query = self.workspace.query.clone();
            let result = self.call(Command::BeginJob {
                command: Box::new(Command::SearchFolders { query, limit: 100 }),
            });
            if result["state"] == "succeeded" {
                self.workspace.loading = false;
                self.workspace_set_items(&result["result"]);
            } else if let Some(id) = result["job_id"].as_u64() {
                self.workspace.job = Some((JobId(id), self.workspace.revision));
                ctx.request_repaint_after(Duration::from_millis(50));
            } else {
                self.workspace.loading = false;
                self.workspace.error = Some(self.message.clone());
            }
        }
    }
    pub(super) fn workspace_panel(&mut self, ctx: &egui::Context) {
        if !self.workspace.open {
            return;
        }
        let state = &mut self.workspace;
        let count = state.items.len();
        let previous = state.selected;
        let armed = ctx.cumulative_frame_nr() > state.opened_frame;
        let enter = ctx.input_mut(|i| {
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                state.selected = (state.selected + 1) % count;
            }
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                state.selected = (state.selected + count - 1) % count;
            }
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) && armed
        });
        state.scroll |= previous != state.selected;
        let mut chosen = if enter && !state.loading {
            state.items.get(state.selected).cloned()
        } else {
            None
        };
        let mut query = state.query.clone();
        let width = (ctx.content_rect().width() - 28.0).clamp(240.0, 600.0);
        let modal = egui::Modal::new(egui::Id::new("workspace-picker"))
            .area(egui::Modal::default_area(egui::Id::new("workspace-picker")).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -58.0]))
            .backdrop_color(Color32::from_black_alpha(24)).frame(crate::action_row::popup_frame(ctx)).show(ctx, |ui| {
                ui.set_width(width);
                ui.label(RichText::new(match state.mode { Mode::Search => "Найти папку · Everything", Mode::History => "История папок", Mode::Favorites => "Избранные папки" }).strong().size(15.0));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 220.0).clamp(80.0, 360.0)).show(ui, |ui| {
                    for (index, path) in state.items.iter().enumerate() {
                        let text = path.to_string_lossy();
                        let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 54.0), egui::Sense::click());
                        if index == state.selected || response.hovered() { ui.painter().rect_filled(rect, 7.0, if index == state.selected { Color32::from_rgb(91,91,99) } else { Color32::from_rgb(77,77,83) }); }
                        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(10.0, 5.0))).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                            ui.add(egui_lucide::Lucide::Folder.size(21.0).image().tint(Color32::from_rgb(224,188,127)));
                            ui.vertical(|ui| { ui.add(egui::Label::new(RichText::new(name).size(14.0)).truncate()); ui.add(egui::Label::new(RichText::new(text.as_ref()).size(11.0).color(Color32::from_gray(183))).truncate()); });
                        });
                        if index == state.selected && state.scroll { response.scroll_to_me(None); state.scroll = false; }
                        if response.clicked() && !state.loading { chosen = Some(path.clone()); }
                        response.on_hover_text(text);
                    }
                    if state.loading { ui.weak("Поиск папок…"); }
                    else if let Some(error) = &state.error { ui.colored_label(Color32::LIGHT_RED, error); }
                    else if state.items.is_empty() { ui.weak(if state.mode == Mode::Search && query.trim().is_empty() { "Введите имя или путь папки" } else { "Папки не найдены" }); }
                });
                ui.separator();
                let field = ui.add(egui::TextEdit::singleline(&mut query).id(egui::Id::new("workspace-query")).hint_text("Поиск папки…").desired_width(f32::INFINITY));
                if state.focus { field.request_focus(); state.focus = false; }
                ui.weak("Ctrl Shift K · поиск    Ctrl H · история    Ctrl F · избранное    Enter · открыть    Esc · закрыть");
            });
        if query != state.query {
            state.query = query;
            self.workspace_reload(ctx);
            return;
        }
        if let Some(path) = chosen {
            let result = self.call(Command::ChooseFolder { path });
            if !result.is_null() {
                self.state = result;
                self.workspace_close();
                self.refresh();
            } else {
                self.workspace.error = Some(self.message.clone());
            }
        } else if modal.should_close() {
            self.workspace_close();
        }
    }
}

//! Search picker: presentation and dispatcher jobs only.
use super::i18n::tr;
use super::*;
use katran::domain_jobs::JobId;

#[path = "workspace_notion_database_ui.rs"]
mod notion_database_ui;
#[path = "workspace_notion_ui.rs"]
mod notion_ui;

/// Consume the physical switch and only its paired text event. Paste stays intact.
fn take_neural_toggle(input: &mut egui::InputState) -> bool {
    let mut toggles = 0;
    let mut paired_text = false;
    input.events.retain(|event| match event {
        egui::Event::Key {
            key,
            physical_key,
            pressed,
            repeat,
            modifiers,
            ..
        } if physical_key.unwrap_or(*key) == egui::Key::Backslash && modifiers.is_none() => {
            paired_text = *pressed;
            if *pressed && !repeat {
                toggles += 1;
            }
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
    toggles % 2 == 1
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Search,
    History,
    Favorites,
    Standard,
}
#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    notion_source: bool,
    notion: notion_ui::State,
    database: Option<notion_database_ui::State>,
    mode: Mode,
    query: String,
    items: Vec<PathBuf>,
    standard_details: Vec<(String, Option<String>)>,
    incomplete: bool,
    neural: bool,
    indexed_count: Option<u64>,
    submitted_query: Option<String>,
    snapshot: Option<String>,
    next_offset: Option<u32>,
    page_offset: u32,
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
        self.workspace_pause_notion();
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
        self.workspace.standard_details.clear();
        self.workspace.incomplete = false;
        self.workspace.indexed_count = None;
        self.workspace.submitted_query = None;
        self.workspace.snapshot = None;
        self.workspace.next_offset = None;
        self.workspace.page_offset = 0;
        self.workspace.selected = 0;
        self.workspace.error = None;
        if self.workspace.mode == Mode::Search {
            if !self.workspace.neural && !self.workspace.query.trim().is_empty() {
                self.workspace.loading = true;
                let debounce = Duration::from_millis(120);
                self.workspace.due = Some(Instant::now() + debounce);
                ctx.request_repaint_after(debounce);
            }
        } else if self.workspace.mode == Mode::Standard {
            let result = self.call(Command::ListKnownFolders);
            if result.is_null() {
                self.workspace.error = Some(self.message.clone());
            } else if let Some(items) = result["items"].as_array() {
                for item in items {
                    self.workspace
                        .items
                        .push(item["path"].as_str().map(PathBuf::from).unwrap_or_default());
                    self.workspace.standard_details.push((
                        item["folder"].as_str().unwrap_or("").to_owned(),
                        item["error"].as_str().map(str::to_owned),
                    ));
                }
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
        self.workspace.incomplete = result["incomplete"] == true;
        self.workspace.indexed_count = result["indexed_count"].as_u64();
        let items: Vec<PathBuf> = result["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["path"].as_str().map(PathBuf::from))
            .collect();
        if self.workspace.neural && self.workspace.mode == Mode::Search {
            self.workspace.snapshot = result["snapshot"].as_str().map(str::to_owned);
            self.workspace.next_offset = result["next_offset"]
                .as_u64()
                .and_then(|offset| u32::try_from(offset).ok());
            if self.workspace.page_offset > 0 {
                let old_len = self.workspace.items.len();
                for path in items {
                    if !self.workspace.items.contains(&path) {
                        self.workspace.items.push(path);
                    }
                }
                if self.workspace.items.len() > old_len {
                    self.workspace.selected = old_len;
                    self.workspace.scroll = true;
                }
            } else {
                self.workspace.items = items;
            }
        } else {
            self.workspace.items = items;
        }
        self.workspace.selected = self
            .workspace
            .selected
            .min(self.workspace.items.len().saturating_sub(1));
    }
    fn workspace_submit_neural(&mut self, ctx: &egui::Context, next: bool) {
        if self.workspace.loading || self.workspace.query.trim().is_empty() {
            return;
        }
        if next {
            let Some(offset) = self.workspace.next_offset else {
                return;
            };
            self.workspace.page_offset = offset;
        } else {
            self.workspace_cancel();
            self.workspace.items.clear();
            self.workspace.selected = 0;
            self.workspace.snapshot = None;
            self.workspace.next_offset = None;
            self.workspace.page_offset = 0;
            self.workspace.submitted_query = Some(self.workspace.query.clone());
        }
        self.workspace.error = None;
        self.workspace.loading = true;
        self.workspace.due = Some(Instant::now());
        ctx.request_repaint();
    }
    fn workspace_mode(&mut self, mode: Mode, ctx: &egui::Context) {
        self.workspace.notion_source = false;
        self.workspace.mode = mode;
        self.workspace.query.clear();
        self.workspace.focus = true;
        self.workspace_reload(ctx);
    }
    fn workspace_action(&mut self, id: String, ctx: &egui::Context) {
        if self.run_notion_database_action(&id, ctx) {
            return;
        }
        let result = self.call(Command::RunAction { id });
        let mode = match result["intent"].as_str() {
            Some("folder_favorites") => Some(Mode::Favorites),
            Some("folder_history") => Some(Mode::History),
            Some("folder_standard") => Some(Mode::Standard),
            _ => None,
        };
        if let Some(mode) = mode {
            self.workspace.open = true;
            self.workspace.opened_frame = ctx.cumulative_frame_nr();
            self.workspace_mode(mode, ctx);
        }
    }
    /// Shared client for picker and Explorer launch; domain owns validation/history.
    pub(super) fn open_working_folder(&mut self, path: PathBuf) -> bool {
        let result = self.call(Command::ChooseFolder { path });
        if result.is_null() {
            return false;
        }
        self.close_notion_database();
        self.query.clear();
        self.state = result;
        self.actions = false;
        self.workspace_close();
        self.refresh();
        true
    }
    pub(super) fn workspace_close(&mut self) {
        self.workspace_cancel();
        self.workspace.open = false;
    }
    pub(super) fn workspace_close_notion(&mut self) {
        if self.workspace.notion_source && self.workspace.open {
            self.workspace_close();
        }
    }
    pub(super) fn workspace_transfer_result(&mut self, result: &Value, import: bool) {
        let Some(items) = result["items"].as_array() else {
            return;
        };
        let mut lines = Vec::new();
        for item in items {
            let id = item["id"].as_i64().unwrap_or_default();
            let status = if import {
                if item["ok"] == true {
                    tr("добавлено", "added")
                } else {
                    tr("ошибка", "error")
                }
            } else {
                match item["status"].as_str().unwrap_or("failed") {
                    "copied" => tr("скопировано", "copied"),
                    "moved" => tr("перемещено", "moved"),
                    "pinned_copy" => tr("закреплено: создана копия", "pinned: copy created"),
                    "same_folder" => tr("уже в этой папке", "already in this folder"),
                    "source_not_moved" => tr(
                        "копия создана, источник сохранён",
                        "copy created, source retained",
                    ),
                    "source_removed_shelf_retained" => tr(
                        "копия создана, исходный файл удалён; запись осталась на полке",
                        "copy created, original deleted; shelf entry retained",
                    ),
                    _ => tr("ошибка", "error"),
                }
            };
            let mut detail = item["error"]["message"]
                .as_str()
                .or_else(|| item["error"].as_str())
                .unwrap_or("")
                .to_owned();
            if let Some(path) = item["recovery_path"].as_str() {
                if !detail.is_empty() {
                    detail.push_str(" · ");
                }
                detail.push_str(tr("Файл сохранён: ", "File saved: "));
                detail.push_str(path);
            }
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
        let message = lines.join("; ");
        self.refresh();
        self.message = message;
    }
    fn workspace_transfer_input(&mut self, ctx: &egui::Context) {
        if self.transfer_input_reserved()
            || self.workspace.open
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
            if self.start_job(Command::AddFolderFiles { ids }, false, ctx.clone()) {
                self.message = tr("Добавление в Катран…", "Adding to Katran…").into();
            }
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::X)) {
            if let Some(items) = self.selection_snapshot() {
                let result = self.call(Command::StageTransfer { items, cut: true });
                if let Some(count) = result["staged"].as_u64() {
                    self.message = if super::i18n::is_english() {
                        format!("Staged for moving: {count} · open Folder and press Ctrl V")
                    } else {
                        format!(
                            "Подготовлено к перемещению: {count} · откройте Folder и нажмите Ctrl V"
                        )
                    };
                }
            } else {
                self.message = tr(
                    "Выбор устарел; обновите полку",
                    "Selection is out of date; refresh the shelf",
                )
                .into();
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
            && self.start_job(Command::PasteToFolder, false, ctx.clone())
        {
            self.message = tr("Вставка файлов…", "Pasting files…").into();
        }
    }
    pub(super) fn workspace_input(&mut self, ctx: &egui::Context) {
        if !self.notion_database_active() {
            self.workspace_transfer_input(ctx);
        }
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
            let already_in_notion = self.workspace.open && self.workspace.notion_source;
            self.workspace.open = true;
            self.workspace.opened_frame = ctx.cumulative_frame_nr();
            if already_in_notion {
                self.workspace_notion_mode(Mode::Search, ctx);
            } else {
                self.workspace_mode(Mode::Search, ctx);
            }
        }
        if (self.workspace.open || !self.modal_owns_input())
            && self.state["settings_open"] != true
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::N)
            })
        {
            self.workspace.open = true;
            self.workspace_open_notion(ctx);
        }
        if self.workspace.open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N))
        {
            if self.workspace.notion_source {
                self.workspace_mode(Mode::Search, ctx);
            } else {
                self.workspace_open_notion(ctx);
            }
        }
        if !self.workspace.open
            && self.state["tab"] == "folder"
            && !self.modal_owns_input()
            && self.state["settings_open"] != true
            && !ctx.egui_wants_keyboard_input()
            && !super::search_or_text_edit_focused(ctx)
        {
            let shortcuts: Vec<katran::shortcuts::ShortcutInfo> =
                serde_json::from_value(self.call(Command::Shortcuts)).unwrap_or_default();
            let folder_shortcuts: Vec<_> = shortcuts
                .into_iter()
                .filter(|s| {
                    matches!(
                        s.id.as_str(),
                        "folder_favorites" | "folder_history" | "folder_standard"
                    )
                })
                .collect();
            if let Some(id) = shortcuts_ui::take_action_with_editing(ctx, &folder_shortcuts, false)
            {
                self.workspace_action(id, ctx);
            }
        }
        if self.workspace.open && self.workspace.notion_source {
            self.workspace_notion_input(ctx);
            return;
        }
        if self.workspace.open {
            if !self.actions
                && !self.open_apps.open
                && !self.save
                && self.shortcut_editor.is_none()
                && self.pending_delete.is_empty()
                && !self.info.is_open()
                && !self.pinned_ui.open
                && !self.tag_menu.open
                && self.state["settings_open"] != true
            {
                let shortcuts: Vec<katran::shortcuts::ShortcutInfo> =
                    serde_json::from_value(self.call(Command::Shortcuts)).unwrap_or_default();
                let standard_shortcut: Vec<_> = shortcuts
                    .into_iter()
                    .filter(|shortcut| shortcut.id == "folder_standard")
                    .collect();
                if let Some(id) =
                    shortcuts_ui::take_action_with_editing(ctx, &standard_shortcut, false)
                {
                    self.workspace_action(id, ctx);
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Period))
                && let Some(path) = self.workspace.items.get(self.workspace.selected).cloned()
                && !path.as_os_str().is_empty()
            {
                let result = self.call(Command::ToggleFavoriteFolderPath { path });
                if let Some(favorite) = result["favorite"].as_bool() {
                    let text = if favorite {
                        tr("Добавлено в избранное", "Added to favorites")
                    } else {
                        tr("Удалено из избранного", "Removed from favorites")
                    };
                    self.favorite_notice = Some((text.into(), Instant::now()));
                    if self.workspace.mode == Mode::Favorites {
                        self.workspace_reload(ctx);
                    }
                } else {
                    self.workspace.error = Some(self.message.clone());
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::H)) {
                self.workspace_mode(Mode::History, ctx);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                self.workspace_mode(Mode::Favorites, ctx);
            }
            if self.workspace.mode == Mode::Search && ctx.input_mut(take_neural_toggle) {
                self.workspace.neural = !self.workspace.neural;
                self.workspace.focus = true;
                self.workspace_reload(ctx);
            }
        } else if !self.actions
            && !self.open_apps.open
            && !self.save
            && self.pending_delete.is_empty()
            && self.state["settings_open"] != true
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Period))
        {
            if self.notion_database_active() {
                self.pin_notion_database_page();
            } else if self.state["tab"] == "folder" {
                let result = self.call(Command::ToggleFavoriteFolder);
                if let Some(favorite) = result["favorite"].as_bool() {
                    self.favorite_notice = Some((
                        if favorite {
                            tr("Добавлено в избранное", "Added to favorites")
                        } else {
                            tr("Удалено из избранного", "Removed from favorites")
                        }
                        .into(),
                        Instant::now(),
                    ));
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
                let before = self.call(Command::Interaction);
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
                    format!(
                        "{}: {failures}",
                        tr("Не удалось изменить закрепление", "Could not change pins")
                    )
                } else {
                    tr("Закрепление обновлено", "Pins updated").into()
                };
                let after = self.call(Command::Interaction);
                self.accept_item_metadata_update(&before, after);
            }
        }
    }
    pub(super) fn workspace_tick(&mut self, ctx: &egui::Context) {
        if !self.workspace.open {
            return;
        }
        if self.workspace.notion_source {
            self.workspace_notion_tick(ctx);
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
                                .unwrap_or(tr("Поиск недоступен", "Search unavailable"))
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
            let command = if self.workspace.neural {
                Command::SearchFoldersSemanticPage {
                    query,
                    offset: self.workspace.page_offset,
                    limit: 10,
                    snapshot: self.workspace.snapshot.clone(),
                }
            } else {
                Command::SearchFolders { query, limit: 100 }
            };
            let result = self.call(Command::BeginJob {
                command: Box::new(command),
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
        if self.workspace.notion_source {
            self.workspace_notion_panel(ctx);
            return;
        }
        let state = &mut self.workspace;
        let count = state.items.len();
        let previous = state.selected;
        let armed = ctx.cumulative_frame_nr() > state.opened_frame;
        let mut next_page = false;
        let enter = ctx.input_mut(|i| {
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                if state.neural && state.mode == Mode::Search && state.selected + 1 == count {
                    next_page = !state.loading && state.next_offset.is_some();
                } else {
                    state.selected = (state.selected + 1) % count;
                }
            }
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                state.selected = (state.selected + count - 1) % count;
            }
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) && armed
        });
        let submit = enter
            && state.neural
            && state.mode == Mode::Search
            && !state.loading
            && (state.submitted_query.as_deref() != Some(state.query.as_str())
                || state.items.is_empty()
                || state.error.is_some());
        state.scroll |= previous != state.selected;
        let mut chosen = if enter && !submit && !state.loading {
            state
                .items
                .get(state.selected)
                .filter(|path| !path.as_os_str().is_empty())
                .cloned()
        } else {
            None
        };
        let mut query = state.query.clone();
        let mut open_notion = false;
        let width = (ctx.content_rect().width() - 28.0).clamp(240.0, 600.0);
        let modal = egui::Modal::new(egui::Id::new("workspace-picker"))
            .area(
                egui::Modal::default_area(egui::Id::new("workspace-picker"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -58.0]),
            )
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(width);
                let neural = state.mode == Mode::Search && state.neural;
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(match state.mode {
                            Mode::Search if neural => tr(
                                "Нейропоиск папок · локально",
                                "Neural folder search · local",
                            ),
                            Mode::Search => {
                                tr("Найти папку · Everything", "Find a folder · Everything")
                            }
                            Mode::History => tr("История папок", "Folder history"),
                            Mode::Favorites => tr("Избранные папки", "Favorite folders"),
                            Mode::Standard => tr("Стандартные папки", "Standard folders"),
                        })
                        .strong()
                        .size(15.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        open_notion = notion_ui::source_button(ui, false);
                    });
                });
                ui.add_space(6.0);
                let mut mode_change = None;
                ui.horizontal_wrapped(|ui| {
                    for (mode, label) in [
                        (Mode::Search, tr("Поиск", "Search")),
                        (Mode::Favorites, tr("Избранное", "Favorites")),
                        (Mode::History, tr("История", "History")),
                        (Mode::Standard, tr("Стандартные папки", "Standard folders")),
                    ] {
                        if ui.selectable_label(state.mode == mode, label).clicked() {
                            mode_change = Some(mode);
                        }
                    }
                });
                if let Some(mode) = mode_change {
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("workspace-mode-change"), mode));
                }
                if state.incomplete {
                    ui.weak(tr("Показаны частичные результаты", "Partial results shown"));
                }
                if neural {
                    ui.weak(tr(
                        "По смыслу имён и путей · без чтения содержимого файлов",
                        "Meaning of names and paths · file contents are not read",
                    ));
                    if let Some(count) = state.indexed_count {
                        ui.weak(format!(
                            "{}: {count}",
                            tr("Папок в ограниченном индексе", "Folders in bounded index")
                        ));
                    }
                }
                egui::ScrollArea::vertical()
                    .max_height(
                        (ctx.content_rect().height() - if neural { 300.0 } else { 240.0 })
                            .clamp(40.0, 360.0),
                    )
                    .show(ui, |ui| {
                        for (index, path) in state.items.iter().enumerate() {
                            let text = super::info_bar::friendly_path(&path.to_string_lossy());
                            let detail = state.standard_details.get(index);
                            let name = detail
                                .map(|(name, _)| {
                                    match name.as_str() {
                                        "desktop" => tr("Рабочий стол", "Desktop"),
                                        "downloads" => tr("Загрузки", "Downloads"),
                                        "documents" => tr("Документы", "Documents"),
                                        "pictures" => tr("Изображения", "Pictures"),
                                        "videos" => tr("Видео", "Videos"),
                                        "music" => tr("Музыка", "Music"),
                                        _ => name.as_str(),
                                    }
                                    .to_owned()
                                })
                                .unwrap_or_else(|| {
                                    path.file_name()
                                        .map(|name| name.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| text.clone())
                                });
                            let detail_text = detail
                                .and_then(|(_, error)| error.as_deref())
                                .unwrap_or(&text);
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 54.0),
                                egui::Sense::click(),
                            );
                            if index == state.selected || response.hovered() {
                                let role = if index == state.selected {
                                    super::theme_ui::Role::Selection
                                } else {
                                    super::theme_ui::Role::Raised
                                };
                                ui.painter().rect_filled(
                                    rect,
                                    7.0,
                                    super::theme_ui::color(ui.ctx(), role, Color32::from_gray(74)),
                                );
                            }
                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(rect.shrink2(egui::vec2(10.0, 5.0)))
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                |ui| {
                                    ui.add(
                                        egui_lucide::Lucide::Folder
                                            .size(21.0)
                                            .image()
                                            .tint(Color32::from_rgb(224, 188, 127)),
                                    );
                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new(name).size(14.0))
                                                .truncate(),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(detail_text)
                                                    .size(11.0)
                                                    .color(Color32::from_gray(183)),
                                            )
                                            .truncate(),
                                        );
                                    });
                                },
                            );
                            if index == state.selected && state.scroll {
                                response.scroll_to_me(None);
                                state.scroll = false;
                            }
                            if response.clicked() && !state.loading && !path.as_os_str().is_empty()
                            {
                                chosen = Some(path.clone());
                            }
                            response.on_hover_text(detail_text);
                        }
                        if state.loading {
                            ui.weak(if neural {
                                tr(
                                    "Подготовка локального индекса и поиск…",
                                    "Preparing local index and searching…",
                                )
                            } else {
                                tr("Поиск папок…", "Searching folders…")
                            });
                        } else if let Some(error) = &state.error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        } else if state.items.is_empty() {
                            ui.weak(if neural && state.submitted_query.is_none() {
                                tr(
                                    "Введите запрос и нажмите Enter",
                                    "Type a query and press Enter",
                                )
                            } else if state.mode == Mode::Search && query.trim().is_empty() {
                                tr("Введите имя или путь папки", "Enter a folder name or path")
                            } else {
                                tr("Папки не найдены", "No folders found")
                            });
                        }
                    });
                ui.separator();
                let gold = Color32::from_rgba_unmultiplied(229, 181, 74, 110);
                let frame = egui::Frame::new().inner_margin(4).corner_radius(9.0);
                let frame = if neural {
                    frame
                        .shadow(egui::Shadow {
                            offset: [0, 0],
                            blur: 16,
                            spread: 1,
                            color: gold,
                        })
                        .stroke(egui::Stroke::new(1.0, gold))
                } else {
                    frame
                };
                let field = frame
                    .show(ui, |ui| {
                        ui.add(super::ui_foundation::text_field(
                            egui::TextEdit::singleline(&mut query)
                                .id(egui::Id::new("workspace-query"))
                                .hint_text(if neural {
                                    tr(
                                        "Опишите, какую папку ищете…",
                                        "Describe the folder you need…",
                                    )
                                } else {
                                    tr("Имя или полный путь папки…", "Folder name or full path…")
                                })
                                .desired_width(f32::INFINITY),
                        ))
                    })
                    .inner;
                if state.focus {
                    field.request_focus();
                    state.focus = false;
                }
                let mut hints = vec![
                    (
                        "Enter",
                        if neural {
                            tr("Искать / открыть", "Search / open")
                        } else {
                            tr("Открыть", "Open")
                        },
                    ),
                    ("Ctrl .", tr("Закрепить", "Favorite")),
                    ("Ctrl F", tr("Избранное", "Favorites")),
                    ("Ctrl H", tr("История", "History")),
                    ("Esc", tr("Закрыть", "Close")),
                ];
                if state.mode == Mode::Search {
                    hints.insert(
                        1,
                        (
                            "\\",
                            if neural {
                                tr("Обычный поиск", "Everything")
                            } else {
                                tr("Нейропоиск", "Neural search")
                            },
                        ),
                    );
                    if neural && state.next_offset.is_some() {
                        hints.insert(2, ("↓", tr("Ещё 10 с конца списка", "10 more at end")));
                    }
                } else {
                    hints.insert(1, ("Ctrl Shift K", tr("Поиск", "Search")));
                }
                super::special_mode_bar::shortcut_hints(ui, &hints, false);
            });
        if open_notion {
            self.workspace_open_notion(ctx);
            return;
        }
        if let Some(mode) =
            ctx.data_mut(|d| d.remove_temp::<Mode>(egui::Id::new("workspace-mode-change")))
        {
            self.workspace_mode(mode, ctx);
            return;
        }
        let mut direct = false;
        let mut direct_error = None;
        if enter && state.mode == Mode::Search && !state.neural {
            match katran::everything::direct_folder_query(&query) {
                Ok(Some(path)) => {
                    chosen = Some(path);
                    direct = true;
                }
                Ok(None) => {}
                Err(error) => {
                    chosen = None;
                    direct = true;
                    direct_error = Some(error.message);
                }
            }
        }
        if query != state.query {
            state.query = query;
            if !direct {
                self.workspace_reload(ctx);
                if enter && self.workspace.neural && self.workspace.mode == Mode::Search {
                    self.workspace_submit_neural(ctx, false);
                }
                return;
            }
        }
        if direct {
            self.workspace_cancel();
            self.workspace.items.clear();
            if let Some(error) = direct_error {
                self.workspace.error = Some(error);
                return;
            }
        }
        if submit || next_page {
            self.workspace_submit_neural(ctx, next_page);
        } else if let Some(path) = chosen {
            if !self.open_working_folder(path) {
                self.workspace.error = Some(self.message.clone());
            }
        } else if modal.should_close() {
            self.workspace_close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_notion_shortcut_opens_picker_and_respects_modal_ownership() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let press = egui::Event::Key {
            key: egui::Key::N,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        };
        app.save = true;
        send_workspace_key(&mut app, &egui::Context::default(), press.clone());
        assert!(!app.workspace.open);
        app.save = false;
        send_workspace_key(&mut app, &egui::Context::default(), press.clone());
        assert!(app.workspace.open);
        assert!(app.workspace.notion_source);
        send_workspace_key(&mut app, &egui::Context::default(), press);
        assert!(
            app.workspace.notion_source,
            "direct search never toggles to Everything"
        );
    }

    #[test]
    fn neural_query_waits_for_enter_and_last_down_appends_ten() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.workspace.open = true;
        app.workspace.neural = true;
        app.workspace.query = "travel".into();
        let ctx = egui::Context::default();
        app.workspace_reload(&ctx);
        assert!(app.workspace.due.is_none());
        assert!(!app.workspace.loading);
        let mut initial = ctx.run_ui(egui::RawInput::default(), |_| app.workspace_panel(&ctx));
        initial.textures_delta.clear();
        let mut enter = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| app.workspace_panel(&ctx),
        );
        enter.textures_delta.clear();
        assert!(app.workspace.loading && app.workspace.due.is_some());
        assert_eq!(app.workspace.submitted_query.as_deref(), Some("travel"));
        let page = |offset| serde_json::json!({"items":(offset..offset+10).map(|i| serde_json::json!({"path":format!("C:/Synthetic/{i}")})).collect::<Vec<_>>(),"snapshot":"same","next_offset":offset+10,"has_more":true,"indexed_count":30});
        app.workspace.loading = false;
        app.workspace.due = None;
        app.workspace_set_items(&page(0));
        assert_eq!(app.workspace.items.len(), 10);
        app.workspace.selected = 9;
        let mut down = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::ArrowDown,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| app.workspace_panel(&ctx),
        );
        down.textures_delta.clear();
        assert!(app.workspace.loading && app.workspace.due.is_some());
        assert_eq!(app.workspace.page_offset, 10);
        assert_eq!(app.workspace.items.len(), 10);
        app.workspace.loading = false;
        app.workspace.due = None;
        app.workspace_set_items(&page(10));
        assert_eq!(app.workspace.items.len(), 20);
        assert_eq!(app.workspace.selected, 10);
        app.workspace.query = "changed query".into();
        app.workspace_reload(&ctx);
        assert!(app.workspace.due.is_none() && app.workspace.snapshot.is_none());
        assert!(app.workspace.items.is_empty());
    }

    #[test]
    fn folder_shortcuts_open_existing_modes_and_respect_text_focus() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::All,
        });
        let ctx = egui::Context::default();
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        for (key, expected) in [
            (egui::Key::F, Mode::Favorites),
            (egui::Key::H, Mode::History),
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![press(key)],
                    ..Default::default()
                },
                |_| app.workspace_input(&ctx),
            );
            output.textures_delta.clear();
            assert!(app.workspace.open);
            assert!(app.workspace.mode == expected);
            app.workspace_close();
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![press(egui::Key::F)],
                ..Default::default()
            },
            |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut String::new())
                        .id(egui::Id::new("folder-test-input")),
                );
                response.request_focus();
                app.workspace_input(&ctx);
            },
        );
        output.textures_delta.clear();
        assert!(!app.workspace.open);
    }

    #[test]
    fn standard_folder_shortcut_uses_custom_binding_inside_focused_picker() {
        struct FakeKnownFolders;
        impl katran::known_folders::Provider for FakeKnownFolders {
            fn resolve(
                &self,
                folder: katran::known_folders::KnownFolder,
            ) -> std::result::Result<PathBuf, String> {
                Ok(PathBuf::from(format!("synthetic/{folder:?}")))
            }
        }

        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.api
            .as_mut()
            .unwrap()
            .set_known_folder_provider(Box::new(FakeKnownFolders));
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::All,
        });
        let assigned: Vec<katran::shortcuts::ShortcutInfo> =
            serde_json::from_value(app.call(Command::SetShortcut {
                id: "folder_standard".into(),
                binding: Some("Ctrl+Shift+Y".into()),
                replace: false,
                reset: false,
            }))
            .unwrap();
        assert_eq!(
            assigned
                .iter()
                .find(|shortcut| shortcut.id == "folder_standard")
                .unwrap()
                .binding
                .as_deref(),
            Some("Ctrl Shift Y")
        );
        let custom_press = egui::Event::Key {
            key: egui::Key::Y,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        };
        let default_press = egui::Event::Key {
            key: egui::Key::Y,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };

        let outside_ctx = egui::Context::default();
        send_workspace_key(&mut app, &outside_ctx, custom_press.clone());
        assert!(app.workspace.open);
        assert!(app.workspace.mode == Mode::Standard);
        assert_eq!(app.workspace.items.len(), 6);

        app.workspace_close();
        let old_chord_ctx = egui::Context::default();
        send_workspace_key(&mut app, &old_chord_ctx, default_press.clone());
        assert!(
            !app.workspace.open,
            "the old default must stop after rebinding"
        );

        app.workspace.open = true;
        app.workspace.mode = Mode::Search;
        let picker_ctx = egui::Context::default();
        send_workspace_key(&mut app, &picker_ctx, custom_press);
        assert!(app.workspace.open);
        assert!(app.workspace.mode == Mode::Standard);

        let reset: Vec<katran::shortcuts::ShortcutInfo> =
            serde_json::from_value(app.call(Command::SetShortcut {
                id: "folder_standard".into(),
                binding: None,
                replace: false,
                reset: true,
            }))
            .unwrap();
        assert_eq!(
            reset
                .iter()
                .find(|shortcut| shortcut.id == "folder_standard")
                .unwrap()
                .binding
                .as_deref(),
            Some("Ctrl Y")
        );
        app.workspace.mode = Mode::Search;
        let text_ctx = egui::Context::default();
        let mut query = String::new();
        let mut retained = false;
        let mut output = text_ctx.run_ui(
            egui::RawInput {
                events: vec![default_press],
                ..Default::default()
            },
            |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .id(egui::Id::new("workspace-query")),
                )
                .request_focus();
                app.workspace_input(&text_ctx);
                retained = text_ctx.input(|input| {
                    input.events.iter().any(|event| {
                        matches!(event, egui::Event::Key { key: egui::Key::Y, pressed: true, modifiers, .. } if modifiers.ctrl && !modifiers.shift)
                    })
                });
            },
        );
        output.textures_delta.clear();
        assert!(app.workspace.open);
        assert!(app.workspace.mode == Mode::Standard);
        assert!(
            !retained,
            "the configured picker shortcut works while its search is focused"
        );
    }

    #[test]
    fn physical_backslash_switches_folder_search_and_consumes_only_its_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(dir.path());
        app.workspace.open = true;
        app.workspace.mode = Mode::Search;
        app.workspace.query = "travel".into();
        let ctx = egui::Context::default();
        let mut remaining = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::Key {
                        key: egui::Key::Slash,
                        physical_key: Some(egui::Key::Backslash),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::Text("/".into()),
                    egui::Event::Paste("C:\\Synthetic\\Travel".into()),
                    egui::Event::Text("unrelated".into()),
                ],
                ..Default::default()
            },
            |_| {
                app.workspace_input(&ctx);
                remaining = ctx.input(|input| input.events.clone());
            },
        );
        output.textures_delta.clear();
        assert!(app.workspace.neural);
        assert_eq!(app.workspace.query, "travel");
        assert!(
            !remaining
                .iter()
                .any(|event| matches!(event, egui::Event::Text(text) if text == "/"))
        );
        assert!(remaining.iter().any(
            |event| matches!(event, egui::Event::Paste(text) if text == "C:\\Synthetic\\Travel")
        ));
        assert!(
            remaining
                .iter()
                .any(|event| matches!(event, egui::Event::Text(text) if text == "unrelated"))
        );
        app.workspace.mode = Mode::History;
        send_workspace_key(
            &mut app,
            &egui::Context::default(),
            egui::Event::Key {
                key: egui::Key::Backslash,
                physical_key: Some(egui::Key::Backslash),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        );
        assert!(app.workspace.neural, "history has no search-mode toggle");
    }

    fn send_workspace_key(app: &mut Shelf, ctx: &egui::Context, event: egui::Event) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![event],
                ..Default::default()
            },
            |_| app.workspace_input(ctx),
        );
        output.textures_delta.clear();
    }

    fn frame(app: &mut Shelf, ctx: &egui::Context, event: Option<egui::Event>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events: event.into_iter().collect(),
                ..Default::default()
            },
            |_| app.workspace_panel(ctx),
        );
        output.textures_delta.clear();
    }
    fn enter(pressed: bool) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn enter_opens_absolute_path_while_search_pending_and_keeps_old_folder_on_error() {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let folder = source.path().join("тестовая папка");
        std::fs::create_dir(&folder).unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        app.workspace.open = true;
        app.workspace.mode = Mode::Search;
        frame(&mut app, &ctx, None);
        app.workspace.query = format!("  \"{}\"  ", folder.display());
        app.workspace.loading = true;
        app.workspace.due = Some(Instant::now() + Duration::from_secs(60));
        let revision = app.workspace.revision;
        frame(&mut app, &ctx, Some(enter(true)));
        assert!(!app.workspace.open);
        assert!(!app.workspace.loading);
        assert!(app.workspace.due.is_none());
        assert!(app.workspace.revision > revision);
        let current = app.call(Command::CurrentFolder);
        assert_eq!(
            PathBuf::from(current["path"].as_str().unwrap()),
            folder.canonicalize().unwrap()
        );
        frame(&mut app, &ctx, Some(enter(false)));
        app.workspace.open = true;
        app.workspace.mode = Mode::Search;
        app.workspace.query = source.path().join("missing").to_string_lossy().into_owned();
        app.workspace.loading = true;
        app.workspace.due = Some(Instant::now() + Duration::from_secs(60));
        frame(&mut app, &ctx, Some(enter(true)));
        assert!(app.workspace.open);
        assert!(!app.workspace.loading);
        assert!(app.workspace.due.is_none());
        assert!(app.workspace.error.is_some());
        assert_eq!(app.call(Command::CurrentFolder), current);
    }
    #[test]
    fn ctrl_period_removes_missing_favorite_from_picker() {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let folder = source.path().join("favorite");
        std::fs::create_dir(&folder).unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::ToggleFavoriteFolderPath {
            path: folder.clone(),
        });
        let saved = app.call(Command::FavoriteFolders { query: "".into() });
        let saved_path = PathBuf::from(saved["items"][0]["path"].as_str().unwrap());
        std::fs::remove_dir(&folder).unwrap();
        app.workspace.open = true;
        app.workspace.mode = Mode::Favorites;
        app.workspace.items = vec![saved_path];
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Period,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |_| app.workspace_input(&ctx),
        );
        output.textures_delta.clear();
        assert!(app.workspace.open);
        assert!(app.workspace.items.is_empty());
        assert_eq!(
            app.favorite_notice.as_ref().unwrap().0,
            "Удалено из избранного"
        );
    }
}

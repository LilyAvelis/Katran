//! Read-only Folder projection for Notion data source rows.
use super::*;
use katran::{interaction::View, notion::Page};

#[derive(serde::Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(serde::Deserialize)]
struct Row {
    #[serde(flatten)]
    page: Page,
    summary: String,
    #[serde(skip)]
    excerpt: Option<String>,
    properties: Vec<Property>,
    created_at: String,
    edited_at: String,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Sort {
    #[default]
    Source,
    Name,
    Created,
    Edited,
}

impl Sort {
    fn key(self) -> Option<&'static str> {
        match self {
            Self::Source => None,
            Self::Name => Some("title"),
            Self::Created => Some("created"),
            Self::Edited => Some("edited"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Source => tr("Порядок Notion", "Notion order"),
            Self::Name => tr("Название", "Name"),
            Self::Created => tr("Дата создания", "Created"),
            Self::Edited => tr("Дата изменения", "Last edited"),
        }
    }
}

pub(super) struct State {
    id: String,
    title: String,
    query: String,
    sort: Sort,
    descending: bool,
    view: View,
    rows: Vec<Row>,
    favorites: std::collections::HashSet<String>,
    selected: usize,
    scroll: bool,
    next_cursor: Option<String>,
    incomplete: bool,
    pending: Option<Instant>,
    rows_job: Option<JobId>,
    excerpt_due: Option<(String, Instant)>,
    excerpt_job: Option<(String, JobId)>,
    preview: Option<Preview>,
    error: Option<String>,
}

#[derive(Default)]
struct Preview {
    blocks: Vec<katran::notion_reader::Block>,
    job: Option<JobId>,
    error: Option<String>,
}

impl State {
    fn new(id: String, title: String) -> Self {
        Self {
            id,
            title,
            query: String::new(),
            sort: Sort::Source,
            descending: false,
            view: View::Details,
            rows: Vec::new(),
            favorites: Default::default(),
            selected: 0,
            scroll: false,
            next_cursor: None,
            incomplete: false,
            pending: Some(Instant::now()),
            rows_job: None,
            excerpt_due: None,
            excerpt_job: None,
            preview: None,
            error: None,
        }
    }

    fn selected(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    fn append(&mut self, result: &Value) -> Result<(), String> {
        let rows: Vec<Row> = serde_json::from_value(result["rows"].clone()).map_err(|_| {
            tr(
                "Неверный ответ базы Notion",
                "Invalid Notion database response",
            )
            .to_owned()
        })?;
        let mut existing = self
            .rows
            .iter()
            .map(|row| row.page.id.clone())
            .collect::<std::collections::HashSet<_>>();
        self.rows.extend(
            rows.into_iter()
                .filter(|row| existing.insert(row.page.id.clone())),
        );
        let cursor = result["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_some() && cursor == self.next_cursor {
            self.next_cursor = None;
            self.incomplete = true;
        } else {
            self.next_cursor = cursor;
            self.incomplete = result["incomplete"] == true;
        }
        Ok(())
    }
}

impl Shelf {
    pub(super) fn open_notion_database(&mut self, ctx: &egui::Context, id: String, title: String) {
        let state = self.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::All,
        });
        if state["tab"] != "folder" {
            return;
        }
        self.state = state;
        self.close_notion_database();
        self.workspace_close();
        let mut database = State::new(id, title);
        let saved = self.call(Command::ListSavedNotionPages { favorites: true });
        database.favorites = saved["pages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|page| page["id"].as_str().map(str::to_owned))
            .collect();
        self.workspace.database = Some(database);
        self.actions = false;
        self.query.clear();
        self.loaded_query.clear();
        self.stop_media_preview();
        self.call(Command::ClosePreview);
        ctx.request_repaint();
    }

    pub(crate) fn notion_database_active(&self) -> bool {
        self.workspace.database.is_some()
    }

    pub(crate) fn close_notion_database(&mut self) {
        if let Some(mut database) = self.workspace.database.take() {
            self.cancel_notion_database_jobs(&mut database);
        }
    }

    fn cancel_notion_database_jobs(&mut self, database: &mut State) {
        if let Some(job_id) = database.rows_job.take() {
            self.call(Command::CancelJob { job_id });
        }
        self.close_notion_database_preview(database);
        self.cancel_notion_database_excerpt(database);
    }

    fn cancel_notion_database_excerpt(&mut self, database: &mut State) {
        database.excerpt_due = None;
        if let Some((_, job_id)) = database.excerpt_job.take() {
            self.call(Command::CancelJob { job_id });
        }
    }

    fn tick_notion_database_excerpt(&mut self, database: &mut State, ctx: &egui::Context) {
        let wanted =
            (database.view == View::Details && database.preview.is_none() && !self.workspace.open)
                .then(|| database.selected())
                .flatten()
                .filter(|row| row.summary.is_empty() && row.excerpt.is_none())
                .map(|row| row.page.id.clone());
        let active_for = database
            .excerpt_job
            .as_ref()
            .map(|pair| &pair.0)
            .or_else(|| database.excerpt_due.as_ref().map(|pair| &pair.0));
        if active_for != wanted.as_ref() {
            self.cancel_notion_database_excerpt(database);
            if let Some(id) = wanted {
                database.excerpt_due = Some((id, Instant::now() + Duration::from_millis(300)));
            }
        }
        if let Some((id, due)) = database.excerpt_due.take() {
            let now = Instant::now();
            if due > now {
                ctx.request_repaint_after(due - now);
                database.excerpt_due = Some((id, due));
            } else {
                let reply = self.call(Command::ReadNotionPageExcerpt {
                    page_id: id.clone(),
                });
                if let Some(job_id) = reply["job_id"].as_u64() {
                    database.excerpt_job = Some((id, JobId(job_id)));
                } else if let Some(row) = database.rows.iter_mut().find(|row| row.page.id == id) {
                    row.excerpt = Some(String::new());
                }
            }
        }
        if let Some((id, job_id)) = database.excerpt_job.take() {
            let reply = self.call(Command::PollJob { job_id });
            if matches!(
                reply["state"].as_str(),
                Some("queued" | "running" | "ready")
            ) {
                ctx.request_repaint_after(Duration::from_millis(45));
                database.excerpt_job = Some((id, job_id));
            } else if let Some(row) = database.rows.iter_mut().find(|row| row.page.id == id) {
                row.excerpt = Some(
                    reply["result"]["excerpt"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                );
            }
        }
    }

    fn close_notion_database_preview(&mut self, database: &mut State) {
        if let Some(job_id) = database.preview.take().and_then(|preview| preview.job) {
            self.call(Command::CancelJob { job_id });
        }
    }

    fn start_notion_database_query(&mut self, database: &mut State, next: bool) {
        database.error = None;
        let cursor = if next {
            database.next_cursor.clone()
        } else {
            None
        };
        let command = Command::QueryNotionDatabase {
            source_id: database.id.clone(),
            query: database.query.clone(),
            cursor,
            sort: database.sort.key().map(str::to_owned),
            descending: database.descending,
        };
        let result = self.call(Command::BeginJob {
            command: Box::new(command),
        });
        if let Some(id) = result["job_id"].as_u64() {
            database.rows_job = Some(JobId(id));
        } else {
            database.error = Some(self.message.clone());
        }
    }

    fn start_notion_database_preview(&mut self, database: &mut State) {
        self.close_notion_database_preview(database);
        let Some(row) = database.selected() else {
            return;
        };
        let result = self.call(Command::ReadNotionPage {
            page_id: row.page.id.clone(),
            title: row.page.title.clone(),
        });
        database.preview = Some(Preview {
            job: result["job_id"].as_u64().map(JobId),
            error: result.is_null().then(|| self.message.clone()),
            ..Default::default()
        });
    }

    fn tick_notion_database(&mut self, database: &mut State, ctx: &egui::Context) {
        if database.pending.is_some_and(|due| Instant::now() >= due) {
            database.pending = None;
            self.start_notion_database_query(database, false);
        }
        if let Some(job_id) = database.rows_job {
            let reply = self.call(Command::PollJob { job_id });
            match reply["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(45))
                }
                Some("succeeded") => {
                    database.rows_job = None;
                    database.error = database.append(&reply["result"]).err();
                }
                _ => {
                    database.rows_job = None;
                    database.error = Some(job_error(&reply));
                }
            }
        }
        if let Some(preview) = &mut database.preview
            && let Some(job_id) = preview.job
        {
            let reply = self.call(Command::PollJob { job_id });
            match reply["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(45))
                }
                Some("succeeded") => {
                    preview.job = None;
                    match katran::notion_reader::parse(
                        reply["result"]["markdown"].as_str().unwrap_or_default(),
                    ) {
                        Ok(blocks) => preview.blocks = blocks,
                        Err(error) => preview.error = Some(error.to_owned()),
                    }
                }
                _ => {
                    preview.job = None;
                    preview.error = Some(job_error(&reply));
                }
            }
        }
    }

    fn notion_database_keys(
        &mut self,
        database: &mut State,
        ctx: &egui::Context,
        search_id: egui::Id,
    ) -> bool {
        if self.workspace.open || self.overlay_ui.pending || self.state["settings_open"] == true {
            return false;
        }
        let editing = ctx.memory(|memory| memory.has_focus(search_id));
        let key = |mods, key| ctx.input_mut(|i| i.consume_key(mods, key));
        if key(egui::Modifiers::NONE, egui::Key::Escape) {
            if database.preview.is_some() {
                self.close_notion_database_preview(database);
            } else if editing {
                ctx.memory_mut(|mem| mem.surrender_focus(search_id));
            } else {
                return true;
            }
        }
        let down = key(egui::Modifiers::NONE, egui::Key::ArrowDown);
        let up = key(egui::Modifiers::NONE, egui::Key::ArrowUp);
        if (down || up) && !database.rows.is_empty() {
            if editing {
                ctx.memory_mut(|memory| memory.surrender_focus(search_id));
            } else {
                let max = database.rows.len() - 1;
                let old = database.selected;
                database.selected = if down {
                    (old + 1).min(max)
                } else {
                    old.saturating_sub(1)
                };
                database.scroll |= old != database.selected;
                if old != database.selected && database.preview.is_some() {
                    self.start_notion_database_preview(database);
                } else if down
                    && old == max
                    && database.rows_job.is_none()
                    && database.next_cursor.is_some()
                {
                    self.start_notion_database_query(database, true);
                }
            }
        }
        if let Some(page) = database.selected().map(|row| row.page.clone()) {
            if key(egui::Modifiers::CTRL, egui::Key::Period) {
                let id = page.id.clone();
                let result = self.call(Command::ToggleFavoriteNotionPage { page });
                match result["favorite"].as_bool() {
                    Some(true) => {
                        database.favorites.insert(id);
                    }
                    Some(false) => {
                        database.favorites.remove(&id);
                    }
                    None => database.error = Some(self.message.clone()),
                }
            } else if key(egui::Modifiers::ALT, egui::Key::Enter) {
                if self.call(Command::OpenNotionPage { page })["opened"] != true {
                    database.error = Some(self.message.clone());
                }
            } else if !editing && key(egui::Modifiers::NONE, egui::Key::Space) {
                if database.preview.is_some() {
                    self.close_notion_database_preview(database);
                } else {
                    self.start_notion_database_preview(database);
                }
            } else if !editing
                && key(egui::Modifiers::NONE, egui::Key::Enter)
                && !self.begin_paste_ui(
                    ctx,
                    Command::BeginPasteNotionPage {
                        owner_window: self.native_window,
                        page_id: page.id,
                        title: page.title,
                    },
                )
            {
                database.error = Some(self.message.clone());
            }
        }
        false
    }

    pub(crate) fn notion_database_frame(&mut self, root: &mut egui::Ui, ctx: &egui::Context) {
        // The picker is the only overlay over the database. Its local Folder
        // transfers stay disabled while remote pages own the main surface.
        let picker_key = ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(event,
            egui::Event::Key { key: egui::Key::K | egui::Key::N, pressed: true, modifiers, .. }
                if modifiers.ctrl && modifiers.shift && !modifiers.alt && !modifiers.mac_cmd)
            })
        });
        if self.workspace.open || picker_key {
            self.workspace_input(ctx);
            self.workspace_tick(ctx);
        }
        if !self.workspace.open && self.state["settings_open"] != true {
            let favorite = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F));
            let history = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::H));
            if favorite || history {
                self.workspace.open = true;
                self.workspace_notion_mode(
                    if favorite {
                        Mode::Favorites
                    } else {
                        Mode::History
                    },
                    ctx,
                );
            }
        }
        let Some(mut database) = self.workspace.database.take() else {
            return;
        };
        self.tick_notion_database(&mut database, ctx);
        let search_id = egui::Id::new("notion-database-search");
        let mut close = self.notion_database_keys(&mut database, ctx, search_id);
        let mut tab_command = None;
        let mut criteria_changed = false;
        let mut more = false;
        let mut toggle_preview = false;

        egui::Panel::top("notion-database-header")
            .frame(crate::action_row::popup_frame(ctx))
            .show(root, |ui| {
                if database.preview.is_none() {
                    tab_command = crate::tab_bar::show_with_glass(
                        ui,
                        &self.tab_order,
                        &self.state["tab"],
                        self.api.is_some(),
                        self.experimental_glass_tabs,
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    let back = if database.preview.is_some() {
                        tr("← К записям", "← Back to rows")
                    } else {
                        tr("← Folder", "← Folder")
                    };
                    if ui.button(back).clicked() {
                        if database.preview.is_some() {
                            toggle_preview = true;
                        } else {
                            close = true;
                        }
                    }
                    let title = database
                        .preview
                        .as_ref()
                        .and_then(|_| database.selected())
                        .map_or(database.title.as_str(), |row| row.page.title.as_str());
                    ui.add(egui::Label::new(RichText::new(title).heading()).wrap());
                });
                if database.preview.is_none() {
                    criteria_changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut database.query)
                                .id(search_id)
                                .hint_text(tr("Найти запись по названию…", "Find a row by title…"))
                                .desired_width(ui.available_width()),
                        )
                        .changed();
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(
                            &mut database.view,
                            View::Gallery,
                            tr("Галерея", "Gallery"),
                        );
                        ui.selectable_value(&mut database.view, View::List, tr("Поток", "Stream"));
                        ui.selectable_value(
                            &mut database.view,
                            View::Details,
                            tr("Мини-поток", "Mini stream"),
                        );
                        egui::ComboBox::from_id_salt("notion-database-sort")
                            .selected_text(database.sort.label())
                            .show_ui(ui, |ui| {
                                for sort in [Sort::Source, Sort::Name, Sort::Created, Sort::Edited]
                                {
                                    criteria_changed |= ui
                                        .selectable_value(&mut database.sort, sort, sort.label())
                                        .changed();
                                }
                            });
                        if database.sort != Sort::Source {
                            let direction = if database.descending { "↓" } else { "↑" };
                            if ui
                                .button(direction)
                                .on_hover_text(tr(
                                    "Изменить порядок сортировки",
                                    "Reverse sort order",
                                ))
                                .clicked()
                            {
                                database.descending = !database.descending;
                                criteria_changed = true;
                            }
                        }
                    });
                }
                if let Some(error) = &database.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                    if database.rows_job.is_none() && ui.button(tr("Повторить", "Retry")).clicked()
                    {
                        more = database.next_cursor.is_some();
                        criteria_changed |= !more;
                    }
                }
            });
        if let Some(command) = tab_command {
            self.apply_tab_bar_command(command, ctx);
            close |= self.state["tab"] != "folder";
        }
        if criteria_changed {
            self.cancel_notion_database_jobs(&mut database);
            database.rows.clear();
            database.selected = 0;
            database.next_cursor = None;
            database.incomplete = false;
            database.error = None;
            let delay = Duration::from_millis(300);
            database.pending = Some(Instant::now() + delay);
            ctx.request_repaint_after(delay);
        }
        if close {
            self.cancel_notion_database_jobs(&mut database);
            self.refresh();
        } else {
            if toggle_preview {
                self.close_notion_database_preview(&mut database);
            }
            self.tick_notion_database_excerpt(&mut database, ctx);
            if let Some(preview) = &database.preview {
                egui::ScrollArea::vertical()
                    .id_salt(("notion-database-reader", database.selected))
                    .max_width(root.available_width())
                    .show(root, |ui| {
                        if let Some(error) = &preview.error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                        super::notion_ui::show_reader(ui, &preview.blocks);
                    });
            } else {
                let mut open_row = false;
                more |= render_database_rows(root, &mut database, &mut open_row);
                if open_row {
                    self.start_notion_database_preview(&mut database);
                }
                if more && database.rows_job.is_none() && database.next_cursor.is_some() {
                    self.start_notion_database_query(&mut database, true);
                }
            }
            self.workspace.database = Some(database);
        }
        self.workspace_panel(ctx);
    }
}

fn job_error(reply: &Value) -> String {
    reply["error"]["message"]
        .as_str()
        .unwrap_or(tr("База Notion недоступна", "Notion database unavailable"))
        .to_owned()
}

fn render_database_rows(ui: &mut egui::Ui, database: &mut State, open_row: &mut bool) -> bool {
    if database.rows.is_empty() {
        if database.next_cursor.is_some() && database.rows_job.is_none() {
            return ui.button(tr("Загрузить ещё", "Load more")).clicked();
        }
        if database.pending.is_none() && database.rows_job.is_none() && database.error.is_none() {
            ui.weak(tr("Записи не найдены", "No rows found"));
        }
        return false;
    }
    if database.view != View::Details {
        return row_list(ui, database, open_row);
    }
    let size = ui.available_size().max(egui::vec2(1.0, 1.0));
    let mut more = false;
    let height = size.y;
    if size.x >= 480.0 {
        let left = (size.x * 0.38).clamp(175.0, size.x - 200.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(left);
                    more = row_list(ui, database, open_row);
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    details(ui, database);
                },
            );
        });
    } else {
        ui.allocate_ui_with_layout(
            egui::vec2(size.x, height * 0.4),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_max_height(height * 0.4);
                more = row_list(ui, database, open_row);
            },
        );
        ui.separator();
        details(ui, database);
    }
    more
}

fn row_list(ui: &mut egui::Ui, database: &mut State, open_row: &mut bool) -> bool {
    let mut more = false;
    let width = ui.available_width();
    let scroll_to_selected = std::mem::take(&mut database.scroll);
    egui::ScrollArea::vertical()
        .id_salt("notion-database-rows")
        .max_width(width)
        .show(ui, |ui| {
            let columns = if database.view == View::Gallery {
                (width / 200.0).floor().max(1.0) as usize
            } else {
                1
            };
            let cell_width = (width / columns as f32 - 8.0).max(34.0);
            for (offset, group) in database.rows.chunks(columns).enumerate() {
                ui.horizontal_top(|ui| {
                    for (column, row) in group.iter().enumerate() {
                        let index = offset * columns + column;
                        let title = if database.favorites.contains(&row.page.id) {
                            format!("●  {}", row.page.title)
                        } else {
                            row.page.title.clone()
                        };
                        let gallery = database.view == View::Gallery;
                        let height = if gallery { 74.0 } else { 35.0 };
                        let label = if gallery && !row.summary.is_empty() {
                            let excerpt: String = row.summary.chars().take(85).collect();
                            format!("{title}\n{excerpt}")
                        } else {
                            title
                        };
                        let button = egui::Button::new(label).selected(database.selected == index);
                        let response = ui.add_sized(
                            [cell_width, height],
                            if gallery {
                                button.wrap()
                            } else {
                                button.truncate()
                            },
                        );
                        if response.clicked() || response.double_clicked() {
                            database.selected = index;
                            *open_row |= response.double_clicked();
                        }
                        if scroll_to_selected && index == database.selected {
                            response.scroll_to_me(None);
                        }
                    }
                });
            }
            if database.next_cursor.is_some() {
                more = ui
                    .add_enabled(
                        database.rows_job.is_none(),
                        egui::Button::new(tr("Загрузить ещё", "Load more")),
                    )
                    .clicked();
            }
            if database.incomplete {
                ui.weak(tr(
                    "Notion вернул часть записей",
                    "Notion returned partial results",
                ));
            }
        });
    more
}

fn details(ui: &mut egui::Ui, database: &State) {
    let Some(row) = database.selected() else {
        return;
    };
    let preview = if row.summary.is_empty() {
        row.excerpt
            .as_deref()
            .filter(|text| !text.is_empty())
            .unwrap_or(&row.page.title)
    } else {
        &row.summary
    };
    egui::ScrollArea::vertical()
        .id_salt("notion-database-metadata")
        .max_width(ui.available_width())
        .show(ui, |ui| {
            egui::Frame::new()
                .fill(Color32::from_gray(35))
                .inner_margin(10)
                .corner_radius(6)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(preview).wrap());
                });
            ui.separator();
            for property in &row.properties {
                ui.weak(&property.name);
                ui.add(egui::Label::new(&property.value).wrap());
            }
            for (name, value) in [
                (tr("Создано", "Created"), &row.created_at),
                (tr("Изменено", "Last edited"), &row.edited_at),
            ] {
                if !value.is_empty() {
                    ui.weak(name);
                    ui.label(value);
                }
            }
            ui.weak(tr(
                "Space · читать    Ctrl+. · закрепить страницу",
                "Space · read    Ctrl+. · pin page",
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notion_database_folder_keeps_rows_distinct_and_pins_the_selected_page() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let context = egui::Context::default();
        app.open_notion_database(
            &context,
            "12345678-0000-0000-0000-000000000001".into(),
            "Fixture".into(),
        );
        assert_eq!(app.state["tab"], "folder");
        let mut database = app.workspace.database.take().unwrap();
        database.pending = None;
        let page_id = "12345678-0000-0000-0000-000000000002";
        let payload = serde_json::json!({"rows":[{
            "id":page_id,"title":"Fixture entry","summary":"Excerpt",
            "properties":[{"name":"Status","value":"Ready"}],
            "created_at":"","edited_at":""
        }],"next_cursor":"cursor1"});
        database.append(&payload).unwrap();
        database.append(&payload).unwrap();
        assert_eq!(database.rows.len(), 1);
        assert_eq!(database.selected().unwrap().properties[0].name, "Status");
        assert!(database.incomplete);
        let mut output = context.run_ui(
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
            |_| {
                app.notion_database_keys(&mut database, &context, egui::Id::new("test-filter"));
            },
        );
        output.textures_delta.clear();
        let favorites = app.call(Command::ListSavedNotionPages { favorites: true });
        assert_eq!(favorites["pages"][0]["id"], page_id);
        assert!(database.favorites.contains(page_id));
    }
}

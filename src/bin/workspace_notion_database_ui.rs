//! Read-only Folder projection for Notion data source rows.
use super::*;
use katran::{api::NotionPageFields, notion::Page};

#[derive(serde::Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(serde::Deserialize)]
struct Row {
    #[serde(skip)]
    projection_id: i64,
    #[serde(flatten)]
    page: Page,
    summary: String,
    properties: Vec<Property>,
    created_at: String,
    edited_at: String,
}

impl Row {
    fn fields(&self) -> Vec<(String, String)> {
        let mut fields: Vec<_> = self
            .properties
            .iter()
            .map(|field| (field.name.clone(), field.value.clone()))
            .collect();
        for (name, value) in [
            (tr("Создано", "Created"), &self.created_at),
            (tr("Изменено", "Last edited"), &self.edited_at),
        ] {
            if !value.is_empty() {
                fields.push((name.into(), value.clone()));
            }
        }
        fields
    }
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
}

pub(super) struct State {
    id: String,
    title: String,
    query: String,
    sort: Sort,
    descending: bool,
    rows: Vec<Row>,
    favorites: std::collections::HashSet<String>,
    selected: usize,
    dirty: bool,
    next_cursor: Option<String>,
    incomplete: bool,
    pending: Option<Instant>,
    rows_job: Option<JobId>,
    visited: bool,
    preview: Option<Preview>,
    error: Option<String>,
}

#[derive(Default)]
struct Preview {
    page: i64,
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
            rows: Vec::new(),
            favorites: Default::default(),
            selected: 0,
            dirty: true,
            next_cursor: None,
            incomplete: false,
            pending: Some(Instant::now()),
            rows_job: None,
            visited: false,
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
        self.rows.extend(rows.into_iter().filter_map(|mut row| {
            if !existing.insert(row.page.id.clone()) {
                return None;
            }
            row.projection_id = katran::notion::next_folder_page_id();
            Some(row)
        }));
        self.dirty = true;
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
        self.events.clear();
        self.next_cursor = None;
        self.cancel_folder_page();
        self.stop_media_preview();
        self.call(Command::ClosePreview);
        ctx.request_repaint();
    }

    pub(crate) fn notion_database_active(&self) -> bool {
        self.workspace.database.is_some() && self.state["tab"] == "folder"
    }

    pub(crate) fn close_notion_database(&mut self) {
        if let Some(mut database) = self.workspace.database.take() {
            self.cancel_notion_database_jobs(&mut database);
            self.events.retain(|event| event["source"] != "notion_page");
            if self.state["tab"] == "folder" {
                self.next_cursor = None;
                self.call(Command::ClearSelection);
                self.state = self.call(Command::SetVisible { ids: Vec::new() });
            }
        }
    }

    fn cancel_notion_database_jobs(&mut self, database: &mut State) {
        if let Some(job_id) = database.rows_job.take() {
            self.call(Command::CancelJob { job_id });
        }
        self.close_notion_database_preview(database);
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
            page: row.projection_id,
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
                    if database.error.is_none() && !database.visited {
                        database.visited = self.call(Command::RecordNotionDatabaseVisit {
                            source_id: database.id.clone(),
                            title: database.title.clone(),
                        })["recorded"]
                            == true;
                    }
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

    pub(crate) fn notion_database_sync(&mut self, ctx: &egui::Context) {
        if self.workspace.database.is_none() {
            return;
        }
        if self.state["tab"] != "folder" {
            self.close_notion_database();
            return;
        }
        let Some(mut database) = self.workspace.database.take() else {
            return;
        };
        let cursor = self.state["preview"]
            .as_i64()
            .or_else(|| self.state["cursor"].as_i64());
        database.selected = cursor
            .and_then(|id| database.rows.iter().position(|row| row.projection_id == id))
            .unwrap_or(0);
        let preview = self.state["preview"].as_i64().filter(|id| {
            database
                .selected()
                .is_some_and(|row| row.projection_id == *id)
        });
        if let Some(id) = preview {
            if database
                .preview
                .as_ref()
                .is_none_or(|preview| preview.page != id)
            {
                self.start_notion_database_preview(&mut database);
            }
        } else {
            self.close_notion_database_preview(&mut database);
        }
        self.tick_notion_database(&mut database, ctx);
        if let Some(due) = database.pending {
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
        }
        if std::mem::take(&mut database.dirty) {
            self.events = database
                .rows
                .iter()
                .map(|row| {
                    let fields: Vec<_> = row
                        .fields()
                        .into_iter()
                        .map(|(name, value)| serde_json::json!({"name":name,"value":value}))
                        .collect();
                    serde_json::json!({
                        "id":row.projection_id, "name":row.page.title, "kind":"text_fragment",
                        "source":"notion_page", "text_preview":row.summary,
                        "revision":row.page.id,"notion_fields":fields, "detected_at_ms":0,
                        "properties":{"pinned":database.favorites.contains(&row.page.id)}
                    })
                })
                .collect();
            self.next_cursor = database.next_cursor.clone();
            if database.incomplete {
                self.message = tr(
                    "Notion вернул часть записей",
                    "Notion returned partial results",
                )
                .into();
            }
        }
        if let Some(error) = &database.error {
            self.message.clone_from(error);
        }
        self.workspace.database = Some(database);
    }

    pub(crate) fn refresh_notion_database(&mut self) {
        let Some(mut database) = self.workspace.database.take() else {
            return;
        };
        self.cancel_notion_database_jobs(&mut database);
        database.query = self.query.trim().to_owned();
        database.rows.clear();
        database.selected = 0;
        database.next_cursor = None;
        database.error = None;
        database.incomplete = false;
        database.dirty = true;
        database.pending = Some(Instant::now() + Duration::from_millis(250));
        self.events.clear();
        self.next_cursor = None;
        self.call(Command::ClearSelection);
        self.state = self.call(Command::SetVisible { ids: Vec::new() });
        self.workspace.database = Some(database);
    }

    pub(crate) fn sync_notion_database_query(&mut self) {
        if self
            .workspace
            .database
            .as_ref()
            .is_some_and(|database| database.query != self.query.trim())
        {
            self.refresh_notion_database();
        }
    }

    pub(crate) fn load_more_notion_database(&mut self) {
        let Some(mut database) = self.workspace.database.take() else {
            return;
        };
        if database.rows_job.is_none()
            && database.pending.is_none()
            && database.next_cursor.is_some()
        {
            self.start_notion_database_query(&mut database, true);
        }
        self.workspace.database = Some(database);
    }

    pub(crate) fn show_notion_folder_reader(&mut self, ui: &mut egui::Ui) {
        let Some(database) = self.workspace.database.as_ref() else {
            return;
        };
        let Some(row) = database
            .rows
            .iter()
            .find(|row| Some(row.projection_id) == self.state["preview"].as_i64())
        else {
            return;
        };
        let width = ui.available_width().min(660.0);
        let height = ui.available_height();
        ui.vertical_centered(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width);
                    ui.set_max_width(width);
                    ui.add(egui::Label::new(egui::RichText::new(&row.page.title).heading()).wrap());
                    let Some(preview) = database
                        .preview
                        .as_ref()
                        .filter(|preview| preview.page == row.projection_id)
                    else {
                        return;
                    };
                    egui::ScrollArea::vertical()
                        .id_salt(("notion-folder-reader", row.projection_id))
                        .max_width(width)
                        .show(ui, |ui| {
                            if let Some(error) = &preview.error {
                                ui.colored_label(Color32::LIGHT_RED, error);
                            } else if preview.job.is_none() && preview.blocks.is_empty() {
                                ui.weak(tr(
                                    "У страницы нет содержимого",
                                    "This page has no content",
                                ));
                            } else {
                                super::notion_ui::show_reader(ui, &preview.blocks);
                            }
                        });
                },
            );
        });
    }

    pub(crate) fn notion_database_title(&self) -> Option<&str> {
        self.workspace
            .database
            .as_ref()
            .filter(|_| self.notion_database_active())
            .map(|db| db.title.as_str())
    }

    fn current_notion_page(&self) -> Option<Page> {
        let database = self.workspace.database.as_ref()?;
        let id = self.state["preview"]
            .as_i64()
            .or_else(|| self.state["cursor"].as_i64())?;
        database
            .rows
            .iter()
            .find(|row| row.projection_id == id)
            .map(|row| row.page.clone())
    }

    fn selected_notion_rows(&self, ids: &[i64]) -> Vec<&Row> {
        let Some(database) = self.workspace.database.as_ref() else {
            return Vec::new();
        };
        let rows: std::collections::HashMap<_, _> = database
            .rows
            .iter()
            .map(|row| (row.projection_id, row))
            .collect();
        ids.iter().filter_map(|id| rows.get(id).copied()).collect()
    }

    pub(crate) fn prepare_notion_database_drag(&mut self, ids: &[i64], ctx: &egui::Context) {
        let pages: Vec<_> = self
            .selected_notion_rows(ids)
            .into_iter()
            .map(|row| row.page.clone())
            .collect();
        if !pages.is_empty() {
            self.start_job(
                Command::PrepareNotionPagesDrag { pages },
                false,
                ctx.clone(),
            );
        }
    }

    fn copy_notion_database_fields(&mut self) {
        let ids = self.selected_ids();
        let pages = self
            .selected_notion_rows(&ids)
            .into_iter()
            .map(|row| NotionPageFields {
                title: row.page.title.clone(),
                fields: row.fields(),
            })
            .collect::<Vec<_>>();
        if pages.is_empty() {
            return;
        }
        let reply = self.call(Command::CopyNotionPageFields { pages });
        if reply["text_copied"] == true {
            self.message = tr("Свойства страниц скопированы", "Page properties copied").into();
        }
    }

    pub(super) fn pin_notion_database_page(&mut self) {
        let Some(page) = self.current_notion_page() else {
            return;
        };
        let id = page.id.clone();
        let result = self.call(Command::ToggleFavoriteNotionPage { page });
        if let (Some(database), Some(favorite)) = (
            self.workspace.database.as_mut(),
            result["favorite"].as_bool(),
        ) {
            if favorite {
                database.favorites.insert(id);
            } else {
                database.favorites.remove(&id);
            }
            database.dirty = true;
        }
    }

    pub(crate) fn notion_catalogue_action(id: &str) -> bool {
        matches!(
            id,
            "shelf_search"
                | "preview"
                | "quicklook"
                | "open"
                | "copy"
                | "paste_previous"
                | "refresh"
                | "gallery"
                | "list"
                | "details"
                | "settings"
                | "tabs_settings"
                | "commands"
                | "capture"
                | "quit"
                | "restart"
                | "select_all"
                | "density_more"
                | "density_less"
                | "folder_favorites"
                | "folder_history"
                | "folder_standard"
                | "tab_previous"
                | "tab_next"
                | "tab_toggle"
                | "all"
                | "images"
                | "screenshots"
                | "downloads"
                | "text"
                | "links"
                | "tables"
                | "pods"
                | "files"
                | "desktop"
                | "translate"
                | "sort_arrival"
                | "sort_arrival_reverse"
                | "sort_name"
                | "sort_name_reverse"
                | "sort_modified"
                | "sort_modified_reverse"
                | "sort_notion_order"
        ) || id.starts_with("tab_position_")
    }

    pub(crate) fn run_notion_database_action(&mut self, id: &str, ctx: &egui::Context) -> bool {
        if !self.notion_database_active() {
            return false;
        }
        if !Self::notion_catalogue_action(id) {
            self.actions = false;
            return true;
        }
        if let Some((mode, reverse)) = katran::sorting::action_sort(id) {
            if let Some(database) = &mut self.workspace.database {
                database.sort = match mode {
                    katran::sorting::SortMode::Name => Sort::Name,
                    katran::sorting::SortMode::Modified => Sort::Edited,
                    _ => Sort::Created,
                };
                database.descending = if database.sort == Sort::Created {
                    !reverse
                } else {
                    reverse
                };
            }
            self.refresh_notion_database();
            self.actions = false;
            ctx.request_repaint();
            return true;
        }
        match id {
            "sort_notion_order" => {
                if let Some(database) = &mut self.workspace.database {
                    database.sort = Sort::Source;
                    database.descending = false;
                }
                self.refresh_notion_database();
            }
            "preview" | "quicklook" => self.state = self.call(Command::TogglePreview),
            "select_all" => self.state = self.call(Command::SelectAll),
            "refresh" => self.refresh_notion_database(),
            "copy" => self.copy_notion_database_fields(),
            "open" | "paste_previous" => {
                if let Some(page) = self.current_notion_page() {
                    match id {
                        "open" => {
                            self.call(Command::OpenNotionPage { page });
                        }
                        _ => {
                            self.begin_paste_ui(
                                ctx,
                                Command::BeginPasteNotionPage {
                                    owner_window: self.native_window,
                                    page_id: page.id,
                                    title: page.title,
                                },
                            );
                        }
                    }
                }
            }
            "folder_favorites" | "folder_history" => {
                self.workspace.open = true;
                self.workspace_notion_mode(
                    if id == "folder_favorites" {
                        Mode::Favorites
                    } else {
                        Mode::History
                    },
                    ctx,
                );
            }
            "all" => {
                self.close_notion_database();
                self.query.clear();
                self.loaded_query.clear();
                self.refresh();
            }
            _ => return false,
        }
        self.actions = false;
        ctx.request_repaint();
        true
    }
}

fn job_error(reply: &Value) -> String {
    reply["error"]["message"]
        .as_str()
        .unwrap_or(tr("База Notion недоступна", "Notion database unavailable"))
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Clipboard(Arc<Mutex<String>>);
    impl katran::text_fragments::ClipboardSink for Clipboard {
        fn write(&self, text: &str) -> katran::Result<u32> {
            *self.0.lock().unwrap() = text.to_owned();
            Ok(1)
        }
    }

    #[test]
    fn notion_rows_use_native_folder_render_search_and_actions() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let context = egui::Context::default();
        crate::ui_fonts::install(&context);
        app.open_notion_database(
            &context,
            "12345678-0000-0000-0000-000000000001".into(),
            "Fixture DB".into(),
        );
        let page = "12345678-0000-0000-0000-000000000002";
        let database = app.workspace.database.as_mut().unwrap();
        database.pending = None;
        database
            .append(&serde_json::json!({"rows":[{
            "id":page,"title":"Fixture row","summary":"Excerpt",
            "properties":[{"name":"Status","value":"Ready"}],"created_at":"","edited_at":""
        }],"next_cursor":null}))
            .unwrap();
        let mut frame = |events| {
            let mut out = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.render(ui),
            );
            out.textures_delta.clear();
        };
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(vec![]);
        assert!(
            context
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("test-search-rect")))
                .is_some()
        );
        assert!(
            context
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("test-footer-rect")))
                .is_some()
        );
        frame(vec![key(egui::Key::T, egui::Modifiers::CTRL)]);
        assert!(context.memory(|mem| mem.has_focus(egui::Id::new("shelf-search"))));
        context.memory_mut(|mem| mem.surrender_focus(egui::Id::new("shelf-search")));
        frame(vec![key(egui::Key::K, egui::Modifiers::CTRL)]);
        assert!(app.actions);
        assert_eq!(app.events[0]["name"], "Fixture row");
        assert!(katran::notion::is_folder_page_id(
            app.state["cursor"].as_i64().unwrap()
        ));
        let actions = app.call(Command::Actions);
        let actions = actions.as_array().unwrap();
        assert!(actions.iter().any(|action| action["id"] == "preview"));
        assert!(!actions.iter().any(|action| action["id"] == "delete"));
        let sorting = app.call(Command::SortActions);
        let sorting = sorting.as_array().unwrap();
        assert!(sorting.iter().any(|action| action["id"] == "sort_name"));
        assert!(!sorting.iter().any(|action| action["id"] == "sort_size"));
        assert!(
            app.call(Command::Get {
                id: app.state["cursor"].as_i64().unwrap()
            })
            .is_null()
        );
        app.pin_notion_database_page();
        assert_eq!(
            app.call(Command::ListSavedNotionPages { favorites: true })["pages"][0]["id"],
            page
        );
        assert!(!app.events.is_empty());
        let clipboard = Arc::new(Mutex::new(String::new()));
        app.api
            .as_mut()
            .unwrap()
            .set_text_clipboard_sink(Box::new(Clipboard(clipboard.clone())));
        app.workspace.database.as_mut().unwrap().append(&serde_json::json!({"rows":[{
            "id":"12345678-0000-0000-0000-000000000003","title":"Second fixture","summary":"Body marker",
            "properties":[{"name":"Status","value":"Draft"}],"created_at":"","edited_at":""
        }],"next_cursor":null})).unwrap();
        app.notion_database_sync(&context);
        let ids: Vec<_> = app
            .events
            .iter()
            .filter_map(|event| event["id"].as_i64())
            .collect();
        app.state = app.call(Command::SetVisible { ids: ids.clone() });
        app.state = app.call(Command::SelectAll);
        app.copy_request(&context);
        assert_eq!(
            *clipboard.lock().unwrap(),
            "Fixture row\nStatus: Ready\n\nSecond fixture\nStatus: Draft\n"
        );
        app.start_drag(ids, &context);
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        app.cancel_released_notion_drag(
            &context,
            &egui::RawInput {
                events: vec![egui::Event::PointerButton {
                    pos: egui::Pos2::ZERO,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
        );
        assert!(app.job.is_none() && app.native_drag.is_none() && app.transfer_busy.is_none());
        app.actions = false;
        context.memory_mut(|mem| mem.request_focus(egui::Id::new("shelf-search")));
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Text("matching title".into())],
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        output.textures_delta.clear();
        let database = app.workspace.database.as_ref().unwrap();
        assert_eq!(database.query, "matching title");
        assert!(database.pending.is_some());
    }
}

use super::*;

#[path = "notion_reader_ui.rs"]
mod reader;

pub(super) fn show_reader_scaled(
    ui: &mut egui::Ui,
    blocks: &[katran::notion_reader::Block],
    scale: f32,
) {
    reader::show_scaled(ui, blocks, scale);
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SearchKind {
    #[default]
    Pages,
    Databases,
}

#[derive(Clone)]
struct ResultItem {
    page: katran::notion::Page,
    database: bool,
}

impl ResultItem {
    fn from_value(value: &Value) -> Option<Self> {
        let database = match value["kind"].as_str()? {
            "page" => false,
            "database" => true,
            _ => return None,
        };
        Some(Self {
            page: katran::notion::Page {
                id: value["id"].as_str()?.to_owned(),
                title: value["title"].as_str()?.to_owned(),
            },
            database,
        })
    }
}

#[derive(Default)]
pub(super) struct State {
    mode: Mode,
    query: String,
    pages: Vec<katran::notion::Page>,
    sources: Vec<katran::notion::Page>,
    search_kind: SearchKind,
    exact: Option<ResultItem>,
    history: Vec<ResultItem>,
    selected: usize,
    configured: bool,
    focus: bool,
    more: bool,
    preview_open: bool,
    preview: Option<Vec<katran::notion_reader::Block>>,
    error: Option<String>,
    notice: Option<&'static str>,
    search_due: Option<Instant>,
    job: Option<(JobId, Operation)>,
}

impl State {
    fn results(&self) -> Vec<ResultItem> {
        if self.mode != Mode::Search {
            return self.history.clone();
        }
        if self.mode == Mode::Search
            && let Some(exact) = &self.exact
        {
            return vec![exact.clone()];
        }
        let database = self.mode == Mode::Search && self.search_kind == SearchKind::Databases;
        let pages = if database { &self.sources } else { &self.pages };
        pages
            .iter()
            .cloned()
            .map(|page| ResultItem { page, database })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Browser(Arc<Mutex<Vec<String>>>);
    impl katran::open_file::Launcher for Browser {
        fn launch(
            &self,
            path: &std::path::Path,
            operation: katran::open_file::Operation,
        ) -> katran::Result<()> {
            assert_eq!(operation, katran::open_file::Operation::Open);
            self.0
                .lock()
                .unwrap()
                .push(path.to_string_lossy().into_owned());
            Ok(())
        }
    }

    #[test]
    fn typed_exact_results_override_search_mode_and_history_retains_order() {
        let mut state = State::default();
        state.exact = ResultItem::from_value(
            &serde_json::json!({"kind":"database","id":"db","title":"Database"}),
        );
        assert!(state.results()[0].database);
        state.search_kind = SearchKind::Databases;
        state.exact =
            ResultItem::from_value(&serde_json::json!({"kind":"page","id":"page","title":"Page"}));
        assert!(!state.results()[0].database);
        state.mode = Mode::History;
        state.history = [
            serde_json::json!({"kind":"page","id":"p","title":"First"}),
            serde_json::json!({"kind":"database","id":"d","title":"Second"}),
        ]
        .iter()
        .filter_map(ResultItem::from_value)
        .collect();
        let results = state.results();
        assert_eq!(results[0].page.id, "p");
        assert!(results[1].database);
        state.mode = Mode::Favorites;
        assert!(state.results()[1].database);
        assert!(
            ResultItem::from_value(&serde_json::json!({"kind":"other","id":"x","title":"X"}))
                .is_none()
        );
    }

    #[test]
    fn backslash_switches_search_and_consumes_its_text() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.workspace.open = true;
        app.workspace.notion_source = true;
        app.workspace.notion = State {
            configured: true,
            query: "fixture".into(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut retained = true;
        ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::Key {
                        key: egui::Key::Backslash,
                        physical_key: Some(egui::Key::Backslash),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::Text("\\".into()),
                ],
                ..Default::default()
            },
            |_| {
                app.workspace_notion_input(&ctx);
                retained = ctx.input(|input| {
                    input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Text(_)))
                });
            },
        )
        .textures_delta
        .clear();
        assert!(app.workspace.notion.search_kind == SearchKind::Databases);
        assert_eq!(app.workspace.notion.query, "fixture");
        assert!(app.workspace.notion.search_due.is_some());
        assert!(!retained);
    }

    #[test]
    fn notion_reader_wraps_long_paragraphs_when_the_window_shrinks() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.workspace.open = true;
        app.workspace.notion_source = true;
        app.workspace.notion = State {
            configured: true,
            preview_open: true,
            preview: Some(
                katran::notion_reader::parse(&format!(
                    "Reader width fixture. {}\n\n> {}",
                    "A long paragraph should wrap. ".repeat(32),
                    "A long quote should also wrap. ".repeat(24)
                ))
                .unwrap(),
            ),
            pages: vec![katran::notion::Page {
                id: "12345678-0000-0000-0000-000000000000".into(),
                title: "Fixture".into(),
            }],
            ..Default::default()
        };
        let context = egui::Context::default();
        crate::ui_fonts::install(&context);
        fn inspect(shape: &egui::Shape, width: f32) -> bool {
            match shape {
                egui::Shape::Text(text)
                    if text.galley.job.text.contains("Reader width fixture.") =>
                {
                    assert!(text.galley.rows.len() > 1, "long paragraph must wrap");
                    assert!(
                        text.galley.rect.width() <= width,
                        "paragraph must fit the window"
                    );
                    assert!(
                        text.pos.x >= 0.0 && text.pos.x + text.galley.rect.width() <= width,
                        "paragraph must stay in view"
                    );
                    true
                }
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| inspect(shape, width)),
                _ => false,
            }
        }
        for width in [800.0, 460.0] {
            for pass in 0..3 {
                let mut result = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 650.0),
                        )),
                        ..Default::default()
                    },
                    |_| app.workspace_notion_panel(&context),
                );
                result.textures_delta.clear();
                if pass == 2 {
                    assert!(
                        result
                            .shapes
                            .iter()
                            .any(|shape| inspect(&shape.shape, width))
                    );
                }
            }
        }
    }

    #[test]
    fn notion_alt_enter_opens_selected_page_from_results_and_reader() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let opened = Arc::new(Mutex::new(Vec::new()));
        app.api
            .as_mut()
            .unwrap()
            .set_file_launcher(Box::new(Browser(opened.clone())));
        let context = egui::Context::default();
        for preview_open in [false, true] {
            app.workspace.open = true;
            app.workspace.notion_source = true;
            app.workspace.opened_frame = 0;
            app.workspace.notion = State {
                configured: true,
                preview_open,
                pages: vec![katran::notion::Page {
                    id: "12345678-0000-0000-0000-000000000000".into(),
                    title: "Fixture".into(),
                }],
                ..Default::default()
            };
            context
                .run_ui(egui::RawInput::default(), |_| {})
                .textures_delta
                .clear();
            let modifiers = egui::Modifiers::ALT;
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            };
            context
                .run_ui(input, |_| app.workspace_notion_panel(&context))
                .textures_delta
                .clear();
            assert!(!app.workspace.open);
            assert!(!app.overlay_ui.pending);
        }
        assert_eq!(
            *opened.lock().unwrap(),
            ["https://www.notion.so/12345678000000000000000000000000"; 2]
        );
    }
}

#[derive(Clone, Copy)]
enum Operation {
    SearchPages,
    SearchDatabases,
    Preview,
}

pub(super) fn source_button(ui: &mut egui::Ui, active: bool) -> bool {
    let (background, foreground) = if active {
        (Color32::WHITE, Color32::BLACK)
    } else {
        (Color32::from_gray(25), Color32::WHITE)
    };
    ui.add(
        egui::Button::new(RichText::new("N").strong().size(15.0).color(foreground))
            .min_size(egui::vec2(26.0, 26.0))
            .corner_radius(13)
            .fill(background)
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(105))),
    )
    .on_hover_text(if active {
        tr(
            "Вернуться в Everything · Ctrl+N",
            "Back to Everything · Ctrl+N",
        )
    } else {
        tr("Поиск Notion · Ctrl+N", "Search Notion · Ctrl+N")
    })
    .clicked()
}

fn monochrome(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    visuals.override_text_color = Some(Color32::from_gray(235));
    visuals.extreme_bg_color = Color32::from_gray(17);
    visuals.selection.bg_fill = Color32::from_gray(74);
    visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
    for (widget, gray) in [
        (&mut visuals.widgets.noninteractive, 32),
        (&mut visuals.widgets.inactive, 44),
        (&mut visuals.widgets.hovered, 62),
        (&mut visuals.widgets.active, 80),
        (&mut visuals.widgets.open, 50),
    ] {
        widget.bg_fill = Color32::from_gray(gray);
        widget.weak_bg_fill = Color32::from_gray(gray);
        widget.bg_stroke.color = Color32::from_gray(87);
        widget.fg_stroke.color = Color32::from_gray(220);
    }
}

impl Shelf {
    pub(super) fn workspace_open_notion(&mut self, ctx: &egui::Context) {
        self.workspace_cancel();
        let configured = self.call(Command::NotionStatus)["configured"] == true;
        self.workspace.notion_source = true;
        self.workspace.notion.configured = configured;
        self.workspace.notion.focus = configured;
        self.workspace.opened_frame = ctx.cumulative_frame_nr();
        ctx.request_repaint();
    }

    pub(super) fn workspace_notion_mode(&mut self, mode: Mode, ctx: &egui::Context) {
        self.workspace_open_notion(ctx);
        if self.workspace.notion.mode != mode {
            self.workspace.notion.preview_open = false;
            self.workspace.notion.preview = None;
            self.workspace.notion.error = None;
            self.workspace.notion.notice = None;
            self.workspace.notion.selected = 0;
        }
        self.workspace.notion.mode = mode;
        if mode != Mode::Search && self.workspace.notion.configured {
            self.workspace_notion_saved(mode);
        }
    }

    fn workspace_notion_saved(&mut self, mode: Mode) {
        let reply = self.call(Command::ListSavedNotionPages {
            favorites: mode == Mode::Favorites,
        });
        let state = &mut self.workspace.notion;
        if reply.is_null() {
            state.error = Some(self.message.clone());
        } else {
            state.pages = serde_json::from_value(reply["pages"].clone()).unwrap_or_default();
            {
                state.history = if let Some(history) = reply["history"].as_array() {
                    history.iter().filter_map(ResultItem::from_value).collect()
                } else {
                    state
                        .pages
                        .iter()
                        .cloned()
                        .map(|page| ResultItem {
                            page,
                            database: false,
                        })
                        .collect()
                };
            }
            state.selected = state.selected.min(state.results().len().saturating_sub(1));
        }
    }

    pub(super) fn workspace_notion_input(&mut self, ctx: &egui::Context) {
        if self.workspace.notion.mode == Mode::Search
            && !self.workspace.notion.preview_open
            && ctx.input_mut(take_neural_toggle)
        {
            self.workspace_cancel_notion();
            let state = &mut self.workspace.notion;
            state.search_kind = match state.search_kind {
                SearchKind::Pages => SearchKind::Databases,
                SearchKind::Databases => SearchKind::Pages,
            };
            state.pages.clear();
            state.sources.clear();
            state.exact = None;
            state.selected = 0;
            state.more = false;
            state.error = None;
            state.notice = None;
            state.search_due = if state.query.trim().is_empty() {
                None
            } else {
                Some(Instant::now() + Duration::from_millis(300))
            };
            state.focus = true;
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        let (favorites, history, pin) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::CTRL, egui::Key::F),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::H),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::Period),
            )
        });
        if favorites || history {
            self.workspace_notion_mode(
                if favorites {
                    Mode::Favorites
                } else {
                    Mode::History
                },
                ctx,
            );
        } else if pin {
            let state = &self.workspace.notion;
            if let Some(item) = state.results().get(state.selected).cloned() {
                let mode = state.mode;
                let database = item.database;
                let reply = if database {
                    self.call(Command::ToggleNotionDatabaseFavorite {
                        source_id: item.page.id,
                        title: item.page.title,
                    })
                } else {
                    self.call(Command::ToggleFavoriteNotionPage { page: item.page })
                };
                if let Some(favorite) = reply["favorite"].as_bool() {
                    if mode == Mode::Favorites {
                        self.workspace_notion_mode(mode, ctx);
                    }
                    self.workspace.notion.notice = Some(match (database, favorite) {
                        (true, true) => tr("База закреплена", "Database pinned"),
                        (true, false) => tr("База откреплена", "Database unpinned"),
                        (false, true) => tr("Страница закреплена", "Page pinned"),
                        (false, false) => tr("Страница откреплена", "Page unpinned"),
                    });
                } else {
                    self.workspace.notion.error = Some(self.message.clone());
                }
            }
        }
    }

    pub(super) fn workspace_cancel_notion(&mut self) {
        if let Some((job_id, _)) = self.workspace.notion.job.take() {
            self.call(Command::CancelJob { job_id });
        }
    }

    pub(super) fn workspace_pause_notion(&mut self) {
        if let Some((_, operation)) = self.workspace.notion.job {
            self.workspace_cancel_notion();
            match operation {
                Operation::SearchPages | Operation::SearchDatabases => {
                    self.workspace.notion.search_due = Some(Instant::now());
                }
                Operation::Preview => self.workspace.notion.preview_open = false,
            }
        }
    }

    pub(crate) fn workspace_notion_paste_failed(&mut self) {
        if self.workspace.open && self.workspace.notion_source {
            self.workspace.notion.error = Some(self.message.clone());
        }
    }

    fn workspace_notion_start(&mut self, command: Command, operation: Operation) {
        self.workspace_cancel_notion();
        let reply = self.call(Command::BeginJob {
            command: Box::new(command),
        });
        if reply["state"] == "succeeded" {
            self.workspace_notion_accept(&reply["result"], operation);
        } else if let Some(id) = reply["job_id"].as_u64() {
            self.workspace.notion.job = Some((JobId(id), operation));
        } else {
            self.workspace.notion.error = Some(self.message.clone());
        }
    }

    fn workspace_notion_accept(&mut self, result: &Value, operation: Operation) {
        let state = &mut self.workspace.notion;
        if matches!(
            operation,
            Operation::SearchPages | Operation::SearchDatabases
        ) {
            state.exact = ResultItem::from_value(&result["exact"]);
            if result["exact"]["source_count"]
                .as_u64()
                .is_some_and(|count| count > 1)
            {
                state.notice = Some(tr(
                    "В базе несколько источников. Показан первый.",
                    "This database has several sources. Showing the first.",
                ));
            }
            state.selected = 0;
        }
        match operation {
            Operation::SearchPages => {
                state.pages = serde_json::from_value(result["pages"].clone()).unwrap_or_default();
                state.more = result["more"] == true;
            }
            Operation::SearchDatabases => {
                state.sources =
                    serde_json::from_value(result["sources"].clone()).unwrap_or_default();
                state.more = result["more"] == true;
            }
            Operation::Preview => {
                let markdown = result["markdown"].as_str().unwrap_or_default();
                match katran::notion_reader::parse(markdown) {
                    Ok(doc) => state.preview = Some(doc),
                    Err(message) => state.error = Some(message.into()),
                }
            }
        }
    }

    pub(super) fn workspace_notion_tick(&mut self, ctx: &egui::Context) {
        if let Some((job_id, operation)) = self.workspace.notion.job {
            let reply = self.call(Command::PollJob { job_id });
            match reply["state"].as_str() {
                Some("queued" | "running" | "ready") => {}
                Some("succeeded") => {
                    self.workspace.notion.job = None;
                    self.workspace_notion_accept(&reply["result"], operation);
                }
                _ => {
                    self.workspace.notion.job = None;
                    self.workspace.notion.error = Some(
                        reply["error"]["message"]
                            .as_str()
                            .unwrap_or(tr("Notion недоступен", "Notion unavailable"))
                            .into(),
                    );
                }
            }
        }
        if self
            .workspace
            .notion
            .search_due
            .is_some_and(|due| Instant::now() >= due)
            && self.workspace.notion.mode == Mode::Search
            && self.workspace.notion.configured
        {
            self.workspace.notion.search_due = None;
            let query = self.workspace.notion.query.clone();
            let (command, operation) = match self.workspace.notion.search_kind {
                SearchKind::Pages => (Command::SearchNotionPages { query }, Operation::SearchPages),
                SearchKind::Databases => (
                    Command::SearchNotionDatabases { query },
                    Operation::SearchDatabases,
                ),
            };
            self.workspace_notion_start(command, operation);
        }
        if self.workspace.notion.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    pub(super) fn workspace_notion_panel(&mut self, ctx: &egui::Context) {
        let query_id = egui::Id::new("workspace-notion-query");
        let editing = ctx.memory(|mem| mem.has_focus(query_id));
        let state = &mut self.workspace.notion;
        let armed = ctx.cumulative_frame_nr() > self.workspace.opened_frame;
        let (move_down, move_up, space, open_in_browser, enter) = ctx.input_mut(|input| {
            if !armed {
                return (false, false, false, false, false);
            }
            let modifiers = egui::Modifiers::NONE;
            let open = input.consume_key(egui::Modifiers::ALT, egui::Key::Enter);
            (
                input.consume_key(modifiers, egui::Key::ArrowDown),
                input.consume_key(modifiers, egui::Key::ArrowUp),
                !editing && input.consume_key(modifiers, egui::Key::Space),
                open,
                !open && input.consume_key(modifiers, egui::Key::Enter),
            )
        });
        let old_selection = state.selected;
        let results = state.results();
        let count = results.len();
        if count > 0 && (move_down || move_up) {
            ctx.memory_mut(|mem| mem.surrender_focus(query_id));
            if !editing {
                state.selected = if move_down {
                    (state.selected + 1) % count
                } else {
                    (state.selected + count - 1) % count
                };
            }
        }
        let mut everything = false;
        let mut mode_change = None;
        let mut query_changed = false;
        let id = egui::Id::new("workspace-notion-picker");
        let width = (ctx.content_rect().width() - 28.0).clamp(1.0, 600.0);
        let max_height = (ctx.content_rect().height() - 205.0).clamp(1.0, 480.0);
        let modal = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -58.0]))
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(crate::action_row::popup_frame(ctx)
                .fill(Color32::from_gray(32))
                .stroke(egui::Stroke::new(1.0, Color32::from_gray(100))))
            .show(ctx, |ui| {
                monochrome(ui);
                ui.set_width(width);
                ui.horizontal(|ui| {
                    let title = if state.search_kind == SearchKind::Databases && state.mode == Mode::Search {
                        tr("Найти базу данных · Notion", "Find a database · Notion")
                    } else {
                        tr("Найти страницу · Notion", "Find a page · Notion")
                    };
                    ui.label(RichText::new(title).strong().size(15.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        everything = source_button(ui, true);
                    });
                });
                if !state.configured {
                    ui.label(tr(
                        "Подключите Notion в настройках интеграций.",
                        "Connect Notion in Integrations settings.",
                    ));
                    return;
                }
                ui.horizontal(|ui| {
                    for (mode, label) in [
                        (Mode::Search, tr("Поиск", "Search")),
                        (Mode::Favorites, tr("Избранное", "Favorites")),
                        (Mode::History, tr("История", "History")),
                    ] {
                        if ui.selectable_label(state.mode == mode, label).clicked() && state.mode != mode {
                            mode_change = Some(mode);
                        }
                    }
                });
                if state.mode == Mode::Search && !state.preview_open {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut state.query)
                            .id(query_id)
                            .desired_width(f32::INFINITY)
                            .hint_text(if state.search_kind == SearchKind::Databases {
                                tr("Название базы данных…", "Database title…")
                            } else {
                                tr("Название страницы…", "Page title…")
                            }),
                    );
                    if state.focus {
                        field.request_focus();
                        state.focus = false;
                    }
                    query_changed = field.changed();
                }
                egui::ScrollArea::vertical()
                    .max_width(width)
                    .max_height(max_height)
                    .show(ui, |ui| {
                        if state.preview_open {
                            if let Some(item) = results.get(state.selected) {
                                ui.heading(&item.page.title);
                            }
                            if let (Some(blocks), Some(page)) =
                                (&state.preview, results.get(state.selected))
                            {
                                if blocks.is_empty() {
                                    ui.weak(tr("Страница пуста", "Page is empty"));
                                } else {
                                    ui.push_id(&page.page.id, |ui| reader::show(ui, blocks));
                                }
                            }
                        } else {
                            let databases = state.mode == Mode::Search && state.search_kind == SearchKind::Databases;
                            for (index, item) in results.iter().enumerate() {
                                let row = ui.horizontal(|ui| {
                                    let icon = if item.database { egui_lucide::Lucide::Database } else { egui_lucide::Lucide::FileText };
                                    ui.add(icon.size(16.0).image().tint(Color32::from_gray(200)));
                                    ui.selectable_label(state.selected == index, &item.page.title)
                                }).inner;
                                if row.clicked() {
                                    state.selected = index;
                                    ctx.memory_mut(|mem| mem.surrender_focus(query_id));
                                }
                                if index == state.selected && old_selection != state.selected {
                                    row.scroll_to_me(None);
                                }
                            }
                            if state.more {
                                ui.weak(tr(
                                    "Уточните название для остальных результатов.",
                                    "Refine the title to see other matches.",
                                ));
                            }
                            if results.is_empty()
                                && !state.query.trim().is_empty()
                                && state.search_due.is_none()
                                && state.job.is_none()
                                && state.error.is_none()
                            {
                                ui.weak(if databases { tr("Базы данных не найдены", "No databases found") } else { tr("Страницы не найдены", "No pages found") });
                            }
                            if results.is_empty() && state.mode != Mode::Search && state.error.is_none() {
                                ui.weak(tr("Здесь пока нет записей", "No items yet"));
                            }
                        }
                        if let Some(error) = &state.error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                    });
                if let Some(notice) = state.notice {
                    ui.weak(notice);
                }
                if results.get(state.selected).is_some_and(|item| item.database) || (state.mode == Mode::Search && state.search_kind == SearchKind::Databases && results.is_empty()) {
                    ui.weak(tr("Enter · открыть базу    Ctrl+. · закрепить", "Enter · open database    Ctrl+. · pin"));
                    if state.mode == Mode::Search {
                        ui.weak(tr("\\ · страницы", "\\ · pages"));
                    }
                } else {
                ui.weak(tr(
                    "Space · превью    Enter · приложить .md    Alt+Enter · открыть в Notion    Ctrl+. · закрепить    Ctrl+F · избранное    Ctrl+H · история",
                    "Space · preview    Enter · attach .md    Alt+Enter · open in Notion    Ctrl+. · pin    Ctrl+F · favorites    Ctrl+H · history",
                ));
                if state.mode == Mode::Search && !state.preview_open {
                    ui.weak(tr("\\ · базы данных", "\\ · databases"));
                }
                }
            });
        if everything {
            self.workspace_mode(Mode::Search, ctx);
        } else if let Some(mode) = mode_change {
            self.workspace_notion_mode(mode, ctx);
        } else if query_changed {
            self.workspace_cancel_notion();
            let state = &mut self.workspace.notion;
            state.pages.clear();
            state.sources.clear();
            state.exact = None;
            state.selected = 0;
            state.more = false;
            state.error = None;
            state.notice = None;
            state.search_due = None;
            if !state.query.trim().is_empty() {
                let delay = Duration::from_millis(300);
                state.search_due = Some(Instant::now() + delay);
                ctx.request_repaint_after(delay);
            }
        } else if modal.should_close() {
            self.workspace_close();
        } else if let Some(item) = results.get(state.selected).cloned() {
            let page = item.page;
            if item.database {
                if self.workspace.notion.preview_open {
                    self.workspace_cancel_notion();
                    self.workspace.notion.preview_open = false;
                    self.workspace.notion.preview = None;
                    self.workspace.notion.error = None;
                }
                if enter {
                    self.open_notion_database(ctx, page.id, page.title);
                }
            } else if open_in_browser {
                if self.call(Command::OpenNotionPage { page })["opened"] == true {
                    self.workspace_close();
                } else {
                    self.workspace.notion.error = Some(self.message.clone());
                }
            } else if enter {
                let command = Command::BeginPasteNotionPage {
                    owner_window: self.native_window,
                    page_id: page.id,
                    title: page.title,
                };
                // Keep the picker open so foreground lease failures can be retried.
                if !self.begin_paste_ui(ctx, command) {
                    self.workspace.notion.error = Some(self.message.clone());
                }
            } else if space || (state.preview_open && old_selection != state.selected) {
                let open = !state.preview_open || old_selection != state.selected;
                self.workspace_cancel_notion();
                self.workspace.notion.preview = None;
                self.workspace.notion.error = None;
                self.workspace.notion.preview_open = open;
                if open {
                    self.workspace_notion_start(
                        Command::ReadNotionPage {
                            page_id: page.id,
                            title: page.title,
                        },
                        Operation::Preview,
                    );
                }
                ctx.request_repaint();
            }
        }
    }
}

use super::*;

#[path = "notion_reader_ui.rs"]
mod reader;

#[derive(Default)]
pub(super) struct State {
    mode: Mode,
    query: String,
    pages: Vec<katran::notion::Page>,
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
    Search,
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
        self.workspace.notion = State {
            configured,
            focus: configured,
            ..State::default()
        };
        self.workspace.opened_frame = ctx.cumulative_frame_nr();
        ctx.request_repaint();
    }

    pub(super) fn workspace_notion_mode(&mut self, mode: Mode, ctx: &egui::Context) {
        self.workspace_open_notion(ctx);
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
            state.selected = state.selected.min(state.pages.len().saturating_sub(1));
        }
    }

    pub(super) fn workspace_notion_input(&mut self, ctx: &egui::Context) {
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
            if let Some(page) = state.pages.get(state.selected).cloned() {
                let mode = state.mode;
                let reply = self.call(Command::ToggleFavoriteNotionPage { page });
                if let Some(favorite) = reply["favorite"].as_bool() {
                    if mode == Mode::Favorites {
                        self.workspace_notion_mode(mode, ctx);
                    }
                    self.workspace.notion.notice = Some(if favorite {
                        tr("Страница закреплена", "Page pinned")
                    } else {
                        tr("Страница откреплена", "Page unpinned")
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
        match operation {
            Operation::Search => {
                state.pages = serde_json::from_value(result["pages"].clone()).unwrap_or_default();
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
        {
            self.workspace.notion.search_due = None;
            let query = self.workspace.notion.query.clone();
            self.workspace_notion_start(Command::SearchNotionPages { query }, Operation::Search);
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
        let count = state.pages.len();
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
                    ui.label(RichText::new(tr("Найти страницу · Notion", "Find a page · Notion")).strong().size(15.0));
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
                            .hint_text(tr("Название страницы…", "Page title…")),
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
                            if let Some(page) = state.pages.get(state.selected) {
                                ui.heading(&page.title);
                            }
                            if let (Some(blocks), Some(page)) =
                                (&state.preview, state.pages.get(state.selected))
                            {
                                ui.push_id(&page.id, |ui| reader::show(ui, blocks));
                            }
                        } else {
                            for (index, page) in state.pages.iter().enumerate() {
                                let row = ui.selectable_label(state.selected == index, &page.title);
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
                            if state.pages.is_empty()
                                && !state.query.trim().is_empty()
                                && state.search_due.is_none()
                                && state.job.is_none()
                                && state.error.is_none()
                            {
                                ui.weak(tr("Страницы не найдены", "No pages found"));
                            }
                            if state.pages.is_empty() && state.mode != Mode::Search && state.error.is_none() {
                                ui.weak(tr("Здесь пока нет страниц", "No pages yet"));
                            }
                        }
                        if let Some(error) = &state.error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                    });
                if let Some(notice) = state.notice {
                    ui.weak(notice);
                }
                ui.weak(tr(
                    "Space · превью    Enter · приложить .md    Alt+Enter · открыть в Notion    Ctrl+. · закрепить    Ctrl+F · избранное    Ctrl+H · история",
                    "Space · preview    Enter · attach .md    Alt+Enter · open in Notion    Ctrl+. · pin    Ctrl+F · favorites    Ctrl+H · history",
                ));
            });
        if everything {
            self.workspace_mode(Mode::Search, ctx);
        } else if let Some(mode) = mode_change {
            self.workspace_notion_mode(mode, ctx);
        } else if query_changed {
            self.workspace_cancel_notion();
            let state = &mut self.workspace.notion;
            state.pages.clear();
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
        } else if let Some(page) = state.pages.get(state.selected).cloned() {
            if open_in_browser {
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

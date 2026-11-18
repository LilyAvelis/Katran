use super::*;

#[derive(Default)]
pub(super) struct State {
    query: String,
    pages: Vec<katran::notion::Page>,
    selected: usize,
    configured: bool,
    focus: bool,
    more: bool,
    preview_open: bool,
    preview: Option<katran::markdown::Document>,
    error: Option<String>,
    search_due: Option<Instant>,
    job: Option<(JobId, Operation)>,
}

#[derive(Clone, Copy)]
enum Operation {
    Search,
    Preview,
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

    pub(super) fn workspace_cancel_notion(&mut self) {
        if let Some((job_id, _)) = self.workspace.notion.job.take() {
            self.call(Command::CancelJob { job_id });
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
                match katran::markdown::parse(markdown) {
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
        let (move_down, move_up, space, enter) = ctx.input_mut(|input| {
            if !armed {
                return (false, false, false, false);
            }
            let modifiers = egui::Modifiers::NONE;
            (
                input.consume_key(modifiers, egui::Key::ArrowDown),
                input.consume_key(modifiers, egui::Key::ArrowUp),
                !editing && input.consume_key(modifiers, egui::Key::Space),
                input.consume_key(modifiers, egui::Key::Enter),
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
        let mut query_changed = false;
        let id = egui::Id::new("workspace-notion-picker");
        let width = (ctx.content_rect().width() - 28.0).clamp(240.0, 600.0);
        let max_height = (ctx.content_rect().height() - 205.0).clamp(75.0, 480.0);
        let modal = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -58.0]))
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    everything = ui.selectable_label(false, "Everything").clicked();
                    ui.selectable_label(true, "Notion").clicked();
                });
                if !state.configured {
                    ui.label(tr(
                        "Подключите Notion в настройках интеграций.",
                        "Connect Notion in Integrations settings.",
                    ));
                    return;
                }
                if !state.preview_open {
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
                    .max_height(max_height)
                    .show(ui, |ui| {
                        if state.preview_open {
                            if let Some(page) = state.pages.get(state.selected) {
                                ui.heading(&page.title);
                            }
                            if let Some(doc) = &state.preview {
                                crate::markdown_ui::show(ui, doc);
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
                        }
                        if let Some(error) = &state.error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                    });
                ui.weak(tr(
                    "↓ ↑ · выбрать    Space · превью    Enter · вставить",
                    "↓ ↑ · select    Space · preview    Enter · paste",
                ));
            });
        if everything {
            self.workspace_cancel_notion();
            self.workspace.notion = State::default();
            self.workspace_mode(Mode::Search, ctx);
        } else if query_changed {
            self.workspace_cancel_notion();
            let state = &mut self.workspace.notion;
            state.pages.clear();
            state.selected = 0;
            state.more = false;
            state.error = None;
            state.search_due = None;
            if !state.query.trim().is_empty() {
                let delay = Duration::from_millis(300);
                state.search_due = Some(Instant::now() + delay);
                ctx.request_repaint_after(delay);
            }
        } else if modal.should_close() {
            self.workspace_close();
        } else if let Some(page_id) = state.pages.get(state.selected).map(|page| page.id.clone()) {
            if enter {
                let command = Command::BeginPasteNotionPage {
                    owner_window: self.native_window,
                    page_id,
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
                        Command::ReadNotionPage { page_id },
                        Operation::Preview,
                    );
                }
                ctx.request_repaint();
            }
        }
    }
}

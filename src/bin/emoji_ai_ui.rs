//! Katran presentation adapter for the domain's local emoji search job.
use super::*;

#[derive(Default)]
pub(super) struct State {
    pub enabled: bool,
    pub model_missing: bool,
    pub next_offset: Option<u32>,
    pub loading: bool,
    pub(super) submitted_query: Option<String>,
    job: Option<SearchJob>,
    literal: Vec<Value>,
    semantic: Vec<Value>,
}

struct SearchJob {
    id: katran::domain_jobs::JobId,
    query: String,
    offset: u32,
}

impl Shelf {
    pub(super) fn emoji_ai_input(&mut self, ctx: &egui::Context) {
        if self.api.is_none()
            || self.state["tab"] != "pods"
            || self.state["emoji_catalogue"] != true
            || self.modal_owns_input()
            || self.state["preview"].is_i64()
            || self.sorting_input.is_open()
        {
            return;
        }
        let search_id = egui::Id::new("shelf-search");
        let focused = ctx.memory(|memory| memory.focused());
        let other_text = focused
            .is_some_and(|id| id != search_id && egui::TextEdit::load_state(ctx, id).is_some());
        if other_text {
            return;
        }
        if self.emoji_ai.enabled
            && ctx.memory(|memory| memory.has_focus(search_id))
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            if self.query.trim().is_empty() {
                return;
            }
            if self.emoji_ai.submitted_query.as_deref() != Some(self.query.as_str()) {
                self.emoji_ai_submit();
            } else if !self.emoji_ai.loading
                && self.state["cursor"].is_i64()
                && !self.events.is_empty()
            {
                self.paste_previous(ctx);
            }
            return;
        }
        if self.emoji_ai.enabled
            && !self.emoji_ai.loading
            && self.state["selected"]
                .as_array()
                .is_some_and(|selected| !selected.is_empty())
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            self.paste_previous(ctx);
            return;
        }
        let binding: Vec<_> = self
            .shortcuts
            .iter()
            .filter(|shortcut| shortcut.id == "search_with_ai")
            .cloned()
            .collect();
        if shortcuts_ui::take_action_with_editing(ctx, &binding, false).is_none() {
            return;
        }
        self.emoji_ai_toggle(ctx);
    }

    pub(super) fn emoji_ai_toggle(&mut self, ctx: &egui::Context) {
        if self.emoji_ai.enabled {
            self.emoji_ai_leave();
            self.query.clear();
            self.loaded_query.clear();
            self.refresh();
            ctx.memory_mut(|memory| memory.surrender_focus(egui::Id::new("shelf-search")));
            ctx.request_repaint();
        } else {
            self.emoji_ai_activate(ctx);
        }
    }

    pub(super) fn emoji_ai_activate(&mut self, ctx: &egui::Context) {
        self.pods_ui.category_open = false;
        self.emoji_ai.enabled = true;
        self.emoji_ai.model_missing = self.call(Command::EmojiAiStatus)["available"] != true;
        self.query.clear();
        self.loaded_query.clear();
        self.state = self.call(Command::SetEmojiCategory { group: None });
        self.state = self.call(Command::ClearSelection);
        self.emoji_ai_clear_input();
        self.loaded_query.clear();
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("shelf-search")));
        ctx.request_repaint();
    }

    pub(super) fn emoji_ai_leave(&mut self) {
        if let Some(job) = self.emoji_ai.job.take() {
            self.call(Command::CancelJob { job_id: job.id });
        }
        self.emoji_ai = emoji_ai_ui::State::default();
    }

    pub(super) fn emoji_ai_refresh(&mut self) {
        if self.query == self.loaded_query {
            return;
        }
        self.state = self.call(Command::ClearSelection);
        self.emoji_ai_clear_input();
        self.loaded_query = self.query.clone();
    }

    fn emoji_ai_clear_input(&mut self) {
        if let Some(job) = self.emoji_ai.job.take() {
            self.call(Command::CancelJob { job_id: job.id });
        }
        self.emoji_ai.literal.clear();
        self.emoji_ai.semantic.clear();
        self.emoji_ai.submitted_query = None;
        self.emoji_ai.next_offset = None;
        self.emoji_ai.loading = false;
        self.events.clear();
        self.next_cursor = None;
        self.pods_ui.category_total = None;
        self.state = self.call(Command::SetVisible { ids: Vec::new() });
    }

    fn emoji_ai_submit(&mut self) {
        if self.emoji_ai.model_missing {
            self.message = i18n::tr(
                "Модель не установлена. Инструкция: docs/emoji-ai.md",
                "Model missing. Installation: docs/emoji-ai.md",
            )
            .into();
            return;
        }
        if self.query.trim().is_empty() || self.emoji_ai.loading {
            return;
        }
        if self.query != self.loaded_query {
            self.emoji_ai_refresh();
        }
        self.emoji_ai_clear_input();
        self.emoji_ai.submitted_query = Some(self.query.clone());
        let literal = self.call(Command::BrowseShelfSorted {
            query: self.query.clone(),
            limit: 10,
            cursor: None,
        });
        if let Some(rows) = literal["events"].as_array() {
            self.emoji_ai.literal.extend(rows.iter().cloned());
        }
        self.emoji_ai_start(0);
    }

    fn emoji_ai_start(&mut self, offset: u32) {
        if self.query.trim().is_empty()
            || self.emoji_ai.job.is_some()
            || self.emoji_ai.submitted_query.as_deref() != Some(self.query.as_str())
        {
            return;
        }
        let result = self.call(Command::BeginJob {
            command: Box::new(Command::SearchEmojiAi {
                query: self.query.clone(),
                offset,
                limit: 10,
            }),
        });
        if let Some(id) = result["job_id"].as_u64() {
            self.emoji_ai.job = Some(SearchJob {
                id: katran::domain_jobs::JobId(id),
                query: self.query.clone(),
                offset,
            });
            self.emoji_ai.loading = true;
        } else {
            self.emoji_ai.loading = false;
        }
    }

    pub(super) fn emoji_ai_load_more(&mut self) {
        if let Some(offset) = self.emoji_ai.next_offset {
            self.emoji_ai_start(offset);
        }
    }

    pub(super) fn emoji_ai_tick(&mut self, ctx: &egui::Context) {
        if self.emoji_ai.enabled
            && (self.state["tab"] != "pods" || self.state["emoji_catalogue"] != true)
        {
            self.emoji_ai_leave();
            return;
        }
        let Some(job) = self.emoji_ai.job.take() else {
            return;
        };
        if !self.emoji_ai.enabled || self.query != job.query {
            self.call(Command::CancelJob { job_id: job.id });
            return;
        }
        let status = self.call(Command::PollJob { job_id: job.id });
        match status["state"].as_str() {
            Some("queued" | "running" | "ready") => {
                self.emoji_ai.job = Some(job);
                ctx.request_repaint_after(Duration::from_millis(40));
            }
            Some("succeeded") => {
                let result = &status["result"];
                if result["offset"].as_u64() != Some(job.offset as u64) {
                    self.message =
                        i18n::tr("Ответ поиска устарел", "Search response is stale").into();
                    self.emoji_ai.loading = false;
                    return;
                }
                if let Some(rows) = result["events"].as_array() {
                    self.emoji_ai.semantic.extend(rows.iter().cloned());
                    self.emoji_ai.next_offset = result["next_offset"]
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok());
                    self.emoji_ai_rebuild_events();
                    if job.offset == 0
                        && let Some(first) =
                            self.events.first().and_then(|event| event["id"].as_i64())
                    {
                        self.state = self.call(Command::Select {
                            id: first,
                            mode: SelectionMode::Replace,
                        });
                        ctx.memory_mut(|memory| {
                            memory.surrender_focus(egui::Id::new("shelf-search"))
                        });
                        self.scroll = true;
                    }
                }
                self.emoji_ai.loading = false;
                ctx.request_repaint();
            }
            _ => {
                self.emoji_ai.loading = false;
                self.message = status["error"]["message"]
                    .as_str()
                    .unwrap_or(i18n::tr(
                        "Поиск с ИИ недоступен",
                        "AI search is unavailable",
                    ))
                    .into();
            }
        }
    }

    fn emoji_ai_rebuild_events(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.events.clear();
        for (label, rows) in [
            (
                i18n::tr("Обычный поиск", "Literal search"),
                &self.emoji_ai.literal,
            ),
            (i18n::tr("Поиск с ИИ", "AI search"), &self.emoji_ai.semantic),
        ] {
            for row in rows {
                let Some(id) = row["id"].as_i64() else {
                    continue;
                };
                if !seen.insert(id) {
                    continue;
                }
                let mut event = row.clone();
                event["original_category"] = event["category"].clone();
                event["category"] = serde_json::json!({"group":label,"subgroup":""});
                self.events.push(event);
            }
        }
        self.state = self.call(Command::SetVisible {
            ids: self
                .events
                .iter()
                .filter_map(|event| event["id"].as_i64())
                .collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct PasteState {
        text: String,
        sent: usize,
    }
    struct Clipboard(Arc<Mutex<PasteState>>);
    impl katran::text_fragments::ClipboardSink for Clipboard {
        fn write(&self, text: &str) -> katran::Result<u32> {
            self.0.lock().unwrap().text = text.into();
            Ok(77)
        }
    }
    struct Previous(Arc<Mutex<PasteState>>);
    impl katran::overlay::Host for Previous {
        fn foreground(&mut self) -> katran::overlay::Result<u64> {
            Ok(7)
        }
        fn identity(&mut self, hwnd: u64) -> katran::overlay::Result<katran::overlay::Target> {
            Ok(katran::overlay::Target {
                hwnd,
                pid: hwnd as u32,
                process_started: hwnd,
            })
        }
        fn work_area(&mut self, _: u64) -> katran::overlay::Result<katran::overlay::WorkArea> {
            Ok(katran::overlay::WorkArea {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            })
        }
        fn wait_modifiers_released(&mut self) -> katran::overlay::Result<()> {
            Ok(())
        }
        fn activate(&mut self, _: u64) -> katran::overlay::Result<()> {
            Ok(())
        }
        fn send_paste(
            &mut self,
            _: katran::overlay::Target,
            sequence: u32,
        ) -> katran::overlay::Result<()> {
            assert_eq!(sequence, 77);
            self.0.lock().unwrap().sent += 1;
            Ok(())
        }
    }

    struct SampleBackend(Vec<String>);
    impl katran::emoji_ai::Backend for SampleBackend {
        fn search(&mut self, _query: &str, offset: u32, limit: u32) -> katran::Result<Value> {
            let rows: Vec<_> = self.0[offset as usize..offset as usize + limit as usize]
                .iter()
                .enumerate()
                .map(|(index, emoji)| {
                    serde_json::json!({
                        "rank":offset as usize + index + 1,
                        "emoji":emoji,
                        "score":0.95 - index as f64 * 0.01,
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "results":rows,
                "offset":offset,
                "next_offset":offset + limit,
            }))
        }
    }

    fn key(app: &mut Shelf, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
    }

    fn release(app: &mut Shelf, ctx: &egui::Context, key: egui::Key) {
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
    }

    #[test]
    fn ctrl_shift_t_is_scoped_and_ai_results_keep_the_catalogue_objects() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        key(
            &mut app,
            &ctx,
            egui::Key::T,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert!(!app.emoji_ai.enabled);
        release(&mut app, &ctx, egui::Key::T);

        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Pods,
            enabled: true,
        });
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Pods,
        });
        app.state = app.call(Command::SetEmojiCatalogue { enabled: true });
        app.refresh();
        let texts: Vec<String> = app
            .events
            .iter()
            .take(30)
            .map(|row| row["text_preview"].as_str().unwrap().to_owned())
            .collect();
        app.api
            .as_mut()
            .unwrap()
            .set_emoji_ai_backend(Box::new(SampleBackend(texts)));
        assert_eq!(app.state["tab"], "pods");
        assert_eq!(app.state["emoji_catalogue"], true);
        assert!(!app.modal_owns_input());
        key(
            &mut app,
            &ctx,
            egui::Key::T,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert!(app.emoji_ai.enabled, "{}", app.message);
        assert!(ctx.memory(|memory| memory.has_focus(egui::Id::new("shelf-search"))));
        release(&mut app, &ctx, egui::Key::T);

        app.query = "co".into();
        app.refresh();
        assert!(app.events.is_empty());
        assert!(app.emoji_ai.job.is_none());
        assert!(!app.emoji_ai.loading);
        app.query = "cow".into();
        app.refresh();
        assert!(app.events.is_empty());
        assert!(app.emoji_ai.job.is_none());
        key(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(app.emoji_ai.submitted_query.as_deref(), Some("cow"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.emoji_ai.loading {
            app.emoji_ai_tick(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            app.events
                .iter()
                .any(|row| row["category"]["group"] == "Обычный поиск")
        );
        assert_eq!(
            app.events
                .iter()
                .filter(|row| row["category"]["group"] == "Поиск с ИИ")
                .count(),
            10
        );
        assert_eq!(app.state["selected"][0], app.events[0]["id"]);
        assert!(!ctx.memory(|memory| memory.has_focus(egui::Id::new("shelf-search"))));
        egui_extras::install_image_loaders(&ctx);
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
        output.textures_delta.clear();
        assert_eq!(app.state["selected"][0], app.events[0]["id"]);
        key(&mut app, &ctx, egui::Key::ArrowRight, egui::Modifiers::NONE);
        assert_eq!(app.state["selected"][0], app.events[1]["id"]);
        app.emoji_ai_load_more();
        while app.emoji_ai.loading {
            app.emoji_ai_tick(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            app.events
                .iter()
                .filter(|row| row["category"]["group"] == "Поиск с ИИ")
                .count(),
            20
        );
        let id = app
            .events
            .iter()
            .find(|row| row["category"]["group"] == "Поиск с ИИ")
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        assert_eq!(app.state["selected"][0], id);

        let pasted = Arc::new(Mutex::new(PasteState::default()));
        app.api
            .as_mut()
            .unwrap()
            .set_text_clipboard_sink(Box::new(Clipboard(pasted.clone())));
        app.api
            .as_mut()
            .unwrap()
            .set_overlay_host(Box::new(Previous(pasted.clone())));
        assert_eq!(
            app.call(Command::SummonOverlay { owner_window: 77 })["previous"]["hwnd"],
            7
        );
        let emoji = app.events.iter().find(|row| row["id"] == id).unwrap()["text_preview"]
            .as_str()
            .unwrap()
            .to_owned();
        key(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(app.overlay_ui.pending, "{}", app.message);
        while app.overlay_ui.pending {
            app.overlay_tick(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(pasted.lock().unwrap().text, emoji);
        assert_eq!(pasted.lock().unwrap().sent, 1);

        key(&mut app, &ctx, egui::Key::T, egui::Modifiers::CTRL);
        assert!(!app.emoji_ai.enabled);
        assert!(ctx.memory(|memory| memory.has_focus(egui::Id::new("shelf-search"))));
        release(&mut app, &ctx, egui::Key::T);
        app.call(Command::SetShortcut {
            id: "search_with_ai".into(),
            binding: Some("Alt T".into()),
            replace: false,
            reset: false,
        });
        app.shortcuts = serde_json::from_value(app.call(Command::Shortcuts)).unwrap();
        key(
            &mut app,
            &ctx,
            egui::Key::T,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert!(!app.emoji_ai.enabled);
        release(&mut app, &ctx, egui::Key::T);
        key(&mut app, &ctx, egui::Key::T, egui::Modifiers::ALT);
        assert!(app.emoji_ai.enabled);
        release(&mut app, &ctx, egui::Key::T);
        key(&mut app, &ctx, egui::Key::T, egui::Modifiers::ALT);
        assert!(!app.emoji_ai.enabled);
        assert!(app.query.is_empty());
        assert_eq!(app.state["emoji_catalogue"], true);
        assert!(!ctx.memory(|memory| memory.has_focus(egui::Id::new("shelf-search"))));
    }
}

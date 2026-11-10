//! Bounded, asynchronous read-only client for text preview.
use super::*;
use std::sync::{Arc, Condvar, Mutex};

type Key = (i64, String);
struct Queue {
    next: Option<(Key, egui::Context)>,
    closed: bool,
}
struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: mpsc::Receiver<(Key, Value)>,
}
pub(super) struct TextPreview {
    root: PathBuf,
    enabled: bool,
    worker: Option<Worker>,
    wanted: Option<Key>,
    content: Option<Result<String, String>>,
    document: Option<katran::markdown::Document>,
    rendered: bool,
}
impl TextPreview {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            enabled: false,
            worker: None,
            wanted: None,
            content: None,
            document: None,
            rendered: true,
        }
    }
    fn start(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let root = self.root.clone();
        let queue = Arc::new((
            Mutex::new(Queue {
                next: None,
                closed: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = mpsc::channel();
        let worker = queue.clone();
        std::thread::spawn(move || {
            let mut api = Service::open_read_only(&root);
            loop {
                let (key, ctx) = {
                    let (lock, wake) = &*worker;
                    let mut q = lock.lock().unwrap();
                    while q.next.is_none() && !q.closed {
                        q = wake.wait(q).unwrap();
                    }
                    if q.closed {
                        return;
                    }
                    q.next.take().unwrap()
                };
                let result = match &mut api {
                    Ok(api) => api.execute(Request {
                        api_version: 1,
                        request_id: "text-preview".into(),
                        command: Command::GetDocumentText { id: key.0 },
                    }),
                    Err(error) => serde_json::json!({"ok":false,"error":error}),
                };
                if tx.send((key, result)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        self.worker = Some(Worker { queue, rx });
    }
    pub fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let (lock, wake) = &*worker.queue;
            if let Ok(mut q) = lock.lock() {
                q.closed = true;
                q.next = None;
                wake.notify_one();
            }
        }
        self.wanted = None;
        self.content = None;
        self.document = None;
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.stop();
        }
    }
    /// Handle view-mode shortcuts before the shelf's global arrow navigation.
    /// `active` is true only while the preview is showing a text document.
    pub fn handle_view_shortcuts(&mut self, ctx: &egui::Context, active: bool) -> bool {
        if !self.enabled || !active || ctx.egui_wants_keyboard_input() {
            return false;
        }
        let key = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                else {
                    return None;
                };
                if modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd {
                    match key {
                        egui::Key::ArrowLeft => Some(egui::Key::ArrowLeft),
                        egui::Key::ArrowRight => Some(egui::Key::ArrowRight),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        });
        let Some(key) = key else {
            return false;
        };
        self.rendered = key == egui::Key::ArrowLeft;
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(event, egui::Event::Key {
                    key: event_key,
                    pressed: true,
                    modifiers,
                    ..
                } if *event_key == key
                    && modifiers.ctrl
                    && !modifiers.shift
                    && !modifiers.alt
                    && !modifiers.mac_cmd)
            });
            input.consume_key(egui::Modifiers::CTRL, key);
        });
        true
    }
    pub fn show(&mut self, ui: &mut egui::Ui, event: &Value) {
        if !self.enabled {
            return;
        }
        let (Some(id), Some(revision)) = (event["id"].as_i64(), event["revision"].as_str()) else {
            return;
        };
        let key = (id, revision.to_owned());
        let is_link = event["kind"] == "link_fragment";
        self.start();
        if self.wanted.as_ref() != Some(&key) {
            self.wanted = Some(key.clone());
            self.content = None;
            self.document = None;
            self.rendered = true;
            let (lock, wake) = &*self.worker.as_ref().unwrap().queue;
            lock.lock().unwrap().next = Some((key, ui.ctx().clone()));
            wake.notify_one();
        }
        while let Ok((key, result)) = self.worker.as_ref().unwrap().rx.try_recv() {
            if self.wanted.as_ref() == Some(&key) {
                self.content = Some(
                    if result["ok"] == true && result["result"]["revision"] == key.1 {
                        Ok(result["result"]["text"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(65_536)
                            .collect())
                    } else {
                        Err(result["error"]["message"]
                            .as_str()
                            .unwrap_or("Текст изменился или недоступен")
                            .into())
                    },
                );
                self.document = if is_link {
                    None
                } else {
                    self.content
                        .as_ref()
                        .and_then(|content| content.as_ref().ok())
                        .and_then(|text| katran::markdown::parse(text).ok())
                };
            }
        }
        let title = if is_link {
            "Ссылка".into()
        } else if event["kind"] == "text_fragment" {
            katran::text_fragments::title(event["text_preview"].as_str().unwrap_or(""))
        } else {
            event["name"].as_str().unwrap_or("").to_owned()
        };
        ui.heading(if title.is_empty() {
            "Текст"
        } else {
            &title
        });
        if !is_link {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.rendered, true, "Markdown");
                ui.selectable_value(&mut self.rendered, false, "Исходный текст");
            });
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height((ui.available_height() - 28.0).max(40.0))
            .show(ui, |ui| match &self.content {
                Some(Ok(text)) => {
                    if self.rendered
                        && let Some(document) = &self.document
                    {
                        crate::markdown_ui::show(ui, document);
                    } else {
                        ui.label(egui::RichText::new(text).monospace().size(15.0));
                    }
                    if event["size"].as_u64().unwrap_or(0) > text.len() as u64 {
                        ui.add_space(12.0);
                        ui.weak("Показана только часть текста; исходный текст сохранён полностью.");
                    }
                }
                Some(Err(message)) => {
                    ui.label(message);
                }
                None => {
                    ui.label("Загрузка текста…");
                }
            });
        ui.weak(if is_link {
            "Space — назад · ← → — листать"
        } else {
            "Space — назад · ← → — листать · Ctrl+← — Markdown · Ctrl+→ — Исходный текст"
        });
    }
}
impl Drop for TextPreview {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn paint_card(ui: &egui::Ui, event: &Value, rect: egui::Rect) {
    ui.painter().rect_filled(rect, 4.0, Color32::from_gray(58));
    let text = event["text_preview"].as_str().unwrap_or("");
    let font = egui::FontId::proportional(13.0);
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        font,
        Color32::from_gray(225),
        (rect.width() - 16.0).max(1.0),
    );
    job.wrap.max_rows = ((rect.height() - 16.0).max(0.0) / 17.0) as usize;
    job.wrap.overflow_character = Some('…');
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter()
        .with_clip_rect(ui.clip_rect().intersect(rect))
        .galley(rect.min + Vec2::splat(8.0), galley, Color32::from_gray(225));
}
pub(super) fn card_title(event: &Value) -> String {
    let title = katran::text_fragments::title(event["text_preview"].as_str().unwrap_or(""));
    if title.is_empty() {
        "Текст".into()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_preview_without_text_feature_or_markdown_controls() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Links,
            enabled: true,
        });
        let url = "https://example.test/путь?q=🐄#fragment";
        let event = app.call(Command::IngestLink {
            url: url.into(),
            source: "test".into(),
        });
        let id = event["id"].as_i64().unwrap();
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Links,
        });
        app.refresh();
        assert_eq!(app.events.len(), 1);
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940., 680.),
            )),
            ..Default::default()
        };
        let mut input = raw();
        input.events.push(egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut out = ctx.run_ui(input, |ui| app.render(ui));
        out.textures_delta.clear();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.text_preview.content.is_none() && Instant::now() < deadline {
            let mut out = ctx.run_ui(raw(), |ui| app.render(ui));
            out.textures_delta.clear();
            std::thread::yield_now();
        }
        assert_eq!(
            app.text_preview.content.as_ref().unwrap().as_ref().unwrap(),
            url
        );
        assert!(app.text_preview.document.is_none());
        let mut out = ctx.run_ui(raw(), |ui| app.render(ui));
        assert!(!out.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == "Markdown" || t.galley.text() == "Исходный текст")));
        out.textures_delta.clear();
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Links,
            enabled: false,
        });
        let mut out = ctx.run_ui(raw(), |ui| app.render(ui));
        out.textures_delta.clear();
        assert!(app.text_preview.worker.is_none());
    }
    #[test]
    fn ctrl_arrows_switch_document_mode_without_changing_preview_or_selection() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: true,
        });
        let event = app.call(Command::IngestText {
            text: "# text".into(),
            source: "test".into(),
        });
        let id = event["id"].as_i64().unwrap();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        app.state = app.call(Command::TogglePreview);
        app.text_preview.enabled = true;
        let selected = app.state["selected"].clone();
        let preview = app.state["preview"].clone();
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let input = |key| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        let mut switched = false;
        let mut still_visible_to_navigation = true;
        let mut output = ctx.run_ui(input(egui::Key::ArrowRight), |ui| {
            switched = app.text_preview.handle_view_shortcuts(ui.ctx(), true);
            still_visible_to_navigation = ui.ctx().input(|input| {
                input.events.iter().any(|event| {
                    matches!(event, egui::Event::Key {
                    key: egui::Key::ArrowRight,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd)
                })
            });
        });
        output.textures_delta.clear();
        assert!(switched);
        assert!(!still_visible_to_navigation);
        assert!(!app.text_preview.rendered);
        assert_eq!(app.state["selected"], selected);
        assert_eq!(app.state["preview"], preview);

        let mut switched = false;
        let mut output = ctx.run_ui(input(egui::Key::ArrowLeft), |ui| {
            switched = app.text_preview.handle_view_shortcuts(ui.ctx(), true);
        });
        output.textures_delta.clear();
        assert!(switched);
        assert!(app.text_preview.rendered);
        assert_eq!(app.state["selected"], selected);
        assert_eq!(app.state["preview"], preview);
    }

    #[test]
    fn ctrl_arrows_leave_text_fields_and_other_previews_alone() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.text_preview.enabled = true;
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        let mut handled = true;
        let mut value = String::new();
        let mut output = ctx.run_ui(input, |ui| {
            ui.add(egui::TextEdit::singleline(&mut value).id(egui::Id::new("shelf-search")))
                .request_focus();
            handled = app.text_preview.handle_view_shortcuts(ui.ctx(), true);
        });
        output.textures_delta.clear();
        assert!(!handled);
        assert!(app.text_preview.rendered);

        let mut handled = true;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::ArrowRight,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ui| handled = app.text_preview.handle_view_shortcuts(ui.ctx(), false),
        );
        output.textures_delta.clear();
        assert!(!handled);
        assert!(app.text_preview.rendered);
    }

    #[test]
    fn text_cards_and_space_preview_use_bounded_async_text() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        assert!(app.text_preview.worker.is_none());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: true,
        });
        let mut settings: katran::settings::Settings =
            serde_json::from_value(app.call(Command::GetSettings)).unwrap();
        settings.quota_bytes = 200_000;
        app.call(Command::SetSettings { settings });
        let content = format!("Первая строка 🐄\n{}", "x".repeat(70_000));
        let event = app.call(Command::IngestText {
            text: content,
            source: "test".into(),
        });
        let id = event["id"].as_i64().unwrap();
        assert_eq!(card_title(&event), "Первая строка 🐄");
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_card(
                ui,
                &event,
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(240.0, 120.0)),
            );
        });
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().contains("Первая строка"))));
        output.textures_delta.clear();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            ..Default::default()
        };
        let mut input = raw();
        input.events.push(egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
        assert_eq!(app.state["preview"], id);
        assert!(app.text_preview.worker.is_some());
        let queue = app.text_preview.worker.as_ref().unwrap().queue.clone();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.text_preview.content.is_none() && Instant::now() < deadline {
            let mut output = ctx.run_ui(raw(), |ui| app.render(ui));
            output.textures_delta.clear();
            std::thread::yield_now();
        }
        let shown = app.text_preview.content.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(shown.chars().count(), 65_536);
        assert!(shown.starts_with("Первая строка 🐄"));
        assert_eq!(app.state["preview_view"]["factor"], 1.0);
        assert!(Arc::ptr_eq(
            &queue,
            &app.text_preview.worker.as_ref().unwrap().queue
        ));
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: false,
        });
        let mut output = ctx.run_ui(raw(), |ui| app.render(ui));
        output.textures_delta.clear();
        assert!(app.text_preview.worker.is_none());
        assert!(app.text_preview.content.is_none());
        assert!(queue.0.lock().unwrap().closed);
    }
    #[test]
    fn markdown_file_previews_with_text_disabled_and_retains_exact_source() {
        let root = tempfile::tempdir().unwrap();
        let originals = tempfile::tempdir().unwrap();
        let path = originals.path().join("synthetic.md");
        let source = "# Привет 🐄\n\n- **важно** и `код`\n<script>literal</script>";
        std::fs::write(&path, source).unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let event = app.call(Command::IngestPath {
            path,
            source: "test".into(),
        });
        let id = event["id"].as_i64().unwrap();
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::All,
        });
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            ..Default::default()
        };
        let mut input = raw();
        input.events.push(egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut out = ctx.run_ui(input, |ui| app.render(ui));
        out.textures_delta.clear();
        assert_eq!(app.state["preview"], id);
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.text_preview.content.is_none() && Instant::now() < deadline {
            let mut out = ctx.run_ui(raw(), |ui| app.render(ui));
            out.textures_delta.clear();
            std::thread::yield_now();
        }
        assert_eq!(
            app.text_preview.content.as_ref().unwrap().as_ref().unwrap(),
            source
        );
        assert_eq!(app.text_preview.document.as_ref().unwrap().source, source);
        app.text_preview.rendered = false;
        let mut out = ctx.run_ui(raw(), |ui| app.render(ui));
        out.textures_delta.clear();
        assert_eq!(
            app.text_preview.content.as_ref().unwrap().as_ref().unwrap(),
            source
        );
    }
}

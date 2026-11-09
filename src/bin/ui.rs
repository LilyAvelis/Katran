#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod action_icons;
mod api_client;
mod app_identity;
mod capture_lamp;
#[cfg(feature = "clipboard-events")]
mod clipboard_events_ui;
mod gallery_card;
mod images;
mod input_log;
mod native_drag_ui;
mod palette;
mod settings_ui;
mod shelf_view;
mod shortcuts_ui;
mod single_instance;
mod ui_fonts;
mod ui_foundation;
mod ui_style;
use base64::Engine;
use eframe::egui::{self, Color32, RichText, Vec2};
use katran::{Command, DEFAULT_QUOTA, Request, Service, interaction::SelectionMode};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
#[cfg(any(not(feature = "clipboard-events"), test))]
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(any(not(feature = "clipboard-events"), test))]
fn poll_delay(elapsed: Duration) -> Duration {
    CLIPBOARD_POLL_INTERVAL.saturating_sub(elapsed)
}
const BUILD_LABEL: &str = if cfg!(feature = "clipboard-events-test") {
    "Katran · EVENTS TEST · 1.0.6"
} else if cfg!(feature = "clipboard-events") {
    "Katran · 1.0.6 · Release"
} else {
    "Katran · 1.0.6 · Legacy Polling"
};
fn clipboard_changed(result: &Value) -> bool {
    result["captured"].as_u64().is_some_and(|count| count > 0)
}
fn item_failures(items: &Value) -> String {
    items
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["ok"] == false || item.get("ok").is_none())
        .map(|item| {
            let name = item["path"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| item["id"].as_i64().map(|id| format!("#{id}")))
                .unwrap_or_else(|| "Объект".into());
            let error = item["error"]
                .as_str()
                .or_else(|| item["error"]["message"].as_str())
                .unwrap_or("Ошибка");
            format!("{name}: {error}")
        })
        .collect::<Vec<_>>()
        .join("; ")
}
fn cleanup_message(result: &Value) -> String {
    let pending = result["cleanup_pending"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if pending.is_empty() {
        String::new()
    } else {
        format!(
            " · Не удалось очистить временные данные: {}",
            pending.join("; ")
        )
    }
}
// Reveal only the part beyond a viewport edge; never recenter a visible row.
fn ensure_visible_offset(offset: f32, viewport: f32, top: f32, height: f32) -> f32 {
    if top < offset {
        top.max(0.0)
    } else if top + height > offset + viewport {
        (top + height - viewport).max(0.0).min(top)
    } else {
        offset
    }
}
use images::Images;
struct Shelf {
    #[cfg(feature = "clipboard-events")]
    clipboard_events: clipboard_events_ui::State,
    instance: Option<single_instance::Primary>,
    activation_rx: Option<mpsc::Receiver<()>>,
    activation_pending: bool,
    native_drag: Option<String>,
    native_window: u64,
    logger: Option<katran::diagnostics::Logger>,
    pending_delete: Vec<i64>,
    settings_ui: settings_ui::State,
    shortcut_editor: Option<shortcuts_ui::Editor>,
    api: Option<Service>,
    job: Option<mpsc::Receiver<(Service, Value, bool)>>,
    pending_drops: Vec<PathBuf>,
    pending_copy: Option<Vec<katran::SelectionRef>>,
    busy_text_editing: bool,
    shortcuts: Vec<katran::shortcuts::ShortcutInfo>,
    card_width: f32,
    images: Images,
    events: Vec<Value>,
    next_cursor: Option<i64>,
    state: Value,
    query: String,
    loaded_query: String,
    actions: bool,
    action_query: String,
    action_selected: usize,
    focus_actions: bool,
    scroll_actions: bool,
    save: bool,
    destination: String,
    message: String,
    capture: bool,
    #[cfg(any(not(feature = "clipboard-events"), test))]
    last_poll: Instant,
    scroll: bool,
}
fn mode(m: egui::Modifiers) -> SelectionMode {
    if m.shift {
        SelectionMode::Range
    } else if m.ctrl {
        SelectionMode::Toggle
    } else {
        SelectionMode::Replace
    }
}
impl Shelf {
    fn render(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        input_log::record(&ctx, self.logger.as_ref());
        if self.api.is_none()
            && self.job.is_some()
            && self.shortcut_editor.is_none()
            && !self.busy_text_editing
            && !self.actions
            && !self.save
            && self.pending_delete.is_empty()
        {
            let copy_shortcuts: Vec<_> = self
                .shortcuts
                .iter()
                .filter(|shortcut| shortcut.id == "copy")
                .cloned()
                .collect();
            if shortcuts_ui::take_action(&ctx, &copy_shortcuts).is_some() {
                self.copy_request(&ctx);
            }
        }
        self.finish_job();
        if self.native_drag.is_some() {
            ctx.request_repaint();
        }
        if self.api.is_some()
            && let Some(command) = self.take_queued_copy()
        {
            self.start_job(command, false, ctx.clone());
            self.message = "Копирование выполняется…".into();
        }
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect::<Vec<_>>()
        });
        self.pending_drops.extend(dropped);
        if self.api.is_some() {
            for path in std::mem::take(&mut self.pending_drops) {
                self.call(Command::IngestPath {
                    path,
                    source: "drop".into(),
                });
                self.refresh();
            }
        }
        if self.api.is_some() {
            if self.shortcut_editor.is_none() && ctx.input_mut(|i| {
            let exact=i.events.iter().any(|e|matches!(e,egui::Event::Key{key:egui::Key::Comma,pressed:true,modifiers,..} if *modifiers==egui::Modifiers::CTRL));
            exact && i.consume_key(egui::Modifiers::CTRL,egui::Key::Comma)
        }) {
            self.actions=false;
            self.call(Command::ShowSettings);
        }
            if let Some(editor) = self.shortcut_editor.as_mut()
                && let Some(intent) = editor.input(&ctx)
                && self
                    .api
                    .as_mut()
                    .is_some_and(|api| shortcuts_ui::apply(api, editor, intent))
            {
                self.shortcut_editor = None;
            }
            if self.shortcut_editor.is_none()
                && !self.actions
                && !self.save
                && self.pending_delete.is_empty()
            {
                self.shortcuts =
                    serde_json::from_value(self.call(Command::Shortcuts)).unwrap_or_default();
                if let Some(id) = shortcuts_ui::take_action(&ctx, &self.shortcuts) {
                    self.run(id, &ctx);
                }
            }
        }
        if self.api.is_some() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K)) {
                self.actions = !self.actions;
                self.focus_actions = self.actions;
                self.action_query.clear();
                self.action_selected = 0;
            }
            if self.shortcut_editor.is_none()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                if !self.pending_delete.is_empty() {
                    self.pending_delete.clear();
                } else if self.actions {
                    self.actions = false;
                } else if self.save {
                    self.save = false;
                } else {
                    self.call(Command::ClosePreview);
                }
            }
        }
        if self.api.is_none() {
            root.disable();
            // Busy commands keep their layout and paint; the status message
            // communicates progress without dimming the source of a drag.
            root.set_opacity(1.0);
        }
        self.state = self.call(Command::Interaction);
        egui::Panel::top("header")
            .frame(egui::Frame::new().inner_margin(12))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    let (rect, r) = ui.allocate_exact_size(
                        egui::vec2((ui.available_width() - 42.0).max(20.0), 26.0),
                        egui::Sense::click_and_drag(),
                    );
                    ui.painter().text(
                        rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        BUILD_LABEL,
                        egui::FontId::proportional(13.0),
                        Color32::from_gray(180),
                    );
                    if r.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if ui.button("×").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Поиск на полке…")
                        .font(egui::FontId::proportional(22.0))
                        .frame(egui::Frame::NONE)
                        .desired_width(ui.available_width()),
                );
                if search.changed() {
                    self.refresh();
                }
                if self.next_cursor.is_some() && ui.button("Загрузить ещё").clicked() {
                    self.load_more();
                }
            });
        egui::Panel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(54, 54, 58))
                    .inner_margin(10),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} · {} выбрано",
                        if self.capture {
                            "Захват включён"
                        } else {
                            "Захват на паузе"
                        },
                        self.state["selected"].as_array().map_or(0, Vec::len)
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui_style::actions_button(ui, self.actions).clicked() {
                            self.actions = !self.actions;
                            self.focus_actions = self.actions;
                            self.action_query.clear();
                        }
                        capture_lamp::show(ui, self.capture);
                    });
                });
            });
        let visible = self.events.clone();
        self.state = self.call(Command::SetVisible {
            ids: visible.iter().filter_map(|e| e["id"].as_i64()).collect(),
        });
        let gallery = self.state["view"] == "gallery";
        let columns = if gallery {
            if self.api.is_some() {
                self.card_width = self.call(Command::GetSettings)["card_width"]
                    .as_f64()
                    .unwrap_or(210.0) as f32;
            }
            gallery_card::columns(root.available_width() - 24.0, self.card_width)
        } else {
            1
        };
        if self.api.is_some() && !self.actions && !self.save && self.pending_delete.is_empty() {
            let keys = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } = e
                        {
                            Some((*key, *modifiers))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            for (key, modifiers) in keys {
                if ctx.egui_wants_keyboard_input()
                    && matches!(key, egui::Key::ArrowLeft | egui::Key::ArrowRight)
                {
                    continue;
                }
                let preview = self.state["preview"].is_i64();
                let delta = match key {
                    egui::Key::ArrowDown | egui::Key::ArrowRight if preview => Some(1),
                    egui::Key::ArrowUp | egui::Key::ArrowLeft if preview => Some(-1),
                    egui::Key::ArrowDown => Some(columns as i32),
                    egui::Key::ArrowUp => Some(-(columns as i32)),
                    egui::Key::ArrowLeft if gallery => Some(-1),
                    egui::Key::ArrowRight if gallery => Some(1),
                    _ => None,
                };
                if let Some(delta) = delta {
                    if let Some(id) = ctx.memory(|m| m.focused()) {
                        ctx.memory_mut(|m| m.surrender_focus(id));
                    }
                    ctx.input_mut(|i| i.consume_key(modifiers, key));
                    self.state = self.call(if preview {
                        Command::NavigatePreview { delta }
                    } else {
                        Command::Navigate {
                            delta,
                            mode: mode(modifiers),
                        }
                    });
                    self.scroll = true;
                }
            }
        }
        self.shelf_view(root, &ctx, &visible, gallery, columns);
        if self.api.is_some() {
            self.palette(&ctx);
        }
        if let Some(api) = self.api.as_mut() {
            self.settings_ui.show(&ctx, api);
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.api.is_some() && !self.pending_delete.is_empty() {
            let result=egui::Modal::new(egui::Id::new("delete-confirmation")).show(&ctx,|ui| {
                ui.heading(format!("Убрать с полки {} объектов?",self.pending_delete.len()));
                ui.label("Исходные файлы останутся на месте. Временные копии без других ссылок будут удалены.");
                let enter=ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Enter));
                if ui.button("Убрать · Enter").clicked()||enter {
                    let ids=std::mem::take(&mut self.pending_delete);
                    let r=self.call(Command::DeleteEvents{ids,confirmed:true});
                    if let Some(count)=r["deleted"].as_u64(){self.message=format!("Убрано с полки: {count}{}", cleanup_message(&r));}
                    self.refresh();
                }
                if ui.button("Отмена · Esc").clicked(){self.pending_delete.clear();}
            });
            if result.should_close() {
                self.pending_delete.clear();
            }
        }
        #[cfg(not(feature = "clipboard-events"))]
        if self.capture {
            ctx.request_repaint_after(CLIPBOARD_POLL_INTERVAL);
        }
        if self.api.is_some() && self.save {
            let result = egui::Modal::new(egui::Id::new("destination")).show(&ctx, |ui| {
                ui.heading("Сохранить копии");
                ui.label("Существующая папка назначения");
                let r =
                    ui.add(egui::TextEdit::singleline(&mut self.destination).desired_width(420.0));
                r.request_focus();
                if ui.button("Сохранить · Enter").clicked()
                    || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                {
                    let ids = self.call(Command::RunAction { id: "save".into() })["ids"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_i64)
                        .collect();
                    let result = self.call(Command::Import {
                        ids,
                        destination: PathBuf::from(&self.destination),
                    });
                    if let Some(items) = result["items"].as_array() {
                        self.message = format!(
                            "Сохранено {}/{}",
                            items.iter().filter(|i| i["ok"] == true).count(),
                            items.len()
                        );
                        let errors = item_failures(&result["items"]);
                        if !errors.is_empty() {
                            self.message.push_str(&format!(" · Ошибки: {errors}"));
                        }
                        self.save = false;
                    }
                }
            });
            if result.should_close() {
                self.save = false;
            }
        }
        #[cfg(not(feature = "clipboard-events"))]
        if self.capture
            && self.api.is_some()
            && self.native_drag.is_none()
            && !self.actions
            && !self.save
            && self.shortcut_editor.is_none()
            && self.pending_delete.is_empty()
            && self.state["settings_open"] != true
        {
            if self.last_poll.elapsed() >= CLIPBOARD_POLL_INTERVAL {
                self.last_poll = Instant::now();
                self.start_job(Command::PollClipboard, true, ctx.clone());
            }
            ctx.request_repaint_after(poll_delay(self.last_poll.elapsed()));
        }
    }
}
impl eframe::App for Shelf {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.finish_job();
        if let Some(rx) = &self.activation_rx {
            self.activation_pending |= rx.try_iter().next().is_some();
        }
        if self.activation_pending {
            if self.api.is_some() {
                let reply = self.call(Command::ActivateWindow);
                if reply["activation_requested"] == true {
                    let foreground = single_instance::show(self.native_window);
                    if let Some(log) = &self.logger {
                        log.record(
                            "window_activation",
                            serde_json::json!({"serial":reply["serial"],"foreground":foreground}),
                        );
                    }
                }
                self.activation_pending = false;
            } else {
                ctx.request_repaint_after(Duration::from_millis(40));
            }
        }
        #[cfg(feature = "clipboard-events")]
        self.event_capture_tick(ctx);
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        // Eframe invokes this before Context::run_ui, outside egui locks.
        self.finish_job();
        self.perform_native_drag(ctx, input);
    }
    fn ui(&mut self, root: &mut egui::Ui, _: &mut eframe::Frame) {
        self.render(root);
    }
}
fn main() -> eframe::Result {
    app_identity::initialize();
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("katran-data"));
    let Some(instance) = single_instance::acquire(&root)
        .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?
    else {
        return Ok(());
    };
    let mut api = Service::open(&root, DEFAULT_QUOTA).unwrap_or_else(|e| {
        eprintln!("{}", e.message);
        std::process::exit(1)
    });
    let logger = katran::diagnostics::Logger::start(&root).ok();
    if let Some(logger) = &logger {
        api.attach_logger(logger.clone());
    }
    let mut app = Shelf {
        #[cfg(feature = "clipboard-events")]
        clipboard_events: Default::default(),
        instance: Some(instance),
        activation_rx: None,
        activation_pending: false,
        native_drag: None,
        native_window: 0,
        logger,
        pending_delete: Vec::new(),
        settings_ui: settings_ui::State::default(),
        shortcut_editor: None,
        api: Some(api),
        job: None,
        pending_drops: Vec::new(),
        pending_copy: None,
        busy_text_editing: false,
        shortcuts: Vec::new(),
        card_width: 210.0,
        images: Images::new(root),
        events: vec![],
        next_cursor: None,
        state: Value::Null,
        query: String::new(),
        loaded_query: String::new(),
        actions: false,
        action_query: String::new(),
        action_selected: 0,
        focus_actions: false,
        scroll_actions: false,
        save: false,
        destination: String::new(),
        message: String::new(),
        capture: false,
        #[cfg(any(not(feature = "clipboard-events"), test))]
        last_poll: Instant::now(),
        scroll: false,
    };
    app.refresh();
    eframe::run_native(
        "Katran",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(app_identity::window_icon())
                .with_inner_size([940.0, 680.0])
                .with_min_inner_size([640.0, 440.0])
                .with_decorations(false),
            ..Default::default()
        },
        Box::new(move |cc| {
            #[cfg(feature = "clipboard-events")]
            {
                app.clipboard_events =
                    clipboard_events_ui::State::start(&cc.egui_ctx, app.logger.clone());
            }
            #[cfg(windows)]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(handle) = cc.window_handle()
                    && let RawWindowHandle::Win32(window) = handle.as_raw()
                {
                    app.native_window = window.hwnd.get() as u64;
                    if let Some(instance) = app.instance.as_mut() {
                        instance.bind(app.native_window)?;
                        let (tx, rx) = mpsc::channel();
                        app.activation_rx = Some(rx);
                        let ctx = cc.egui_ctx.clone();
                        instance.listen(move || {
                            let _ = tx.send(());
                            ctx.request_repaint();
                        });
                    }
                }
            }
            ui_style::install(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn test_shelf(root: &std::path::Path) -> Shelf {
        let api = Service::open(root, 10000).unwrap();
        Shelf {
            #[cfg(feature = "clipboard-events")]
            clipboard_events: Default::default(),
            instance: None,
            activation_rx: None,
            activation_pending: false,
            native_drag: None,
            native_window: 77,
            logger: None,
            pending_delete: Vec::new(),
            settings_ui: settings_ui::State::default(),
            shortcut_editor: None,
            api: Some(api),
            job: None,
            pending_drops: Vec::new(),
            pending_copy: None,
            busy_text_editing: false,
            shortcuts: Vec::new(),
            card_width: 210.0,
            images: Images::new(root.into()),
            events: vec![],
            next_cursor: None,
            state: Value::Null,
            query: String::new(),
            loaded_query: String::new(),
            actions: false,
            action_query: String::new(),
            action_selected: 0,
            focus_actions: false,
            scroll_actions: false,
            save: false,
            destination: String::new(),
            message: String::new(),
            capture: false,
            last_poll: Instant::now(),
            scroll: false,
        }
    }

    fn frame(app: &mut Shelf, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            // Exercise the real UI without any native window or system clipboard.
            app.render(ui);
        });
        output.textures_delta.clear();
    }
    #[test]
    fn reports_individual_errors_and_pending_cleanup() {
        let items = serde_json::json!([{ "id": 4, "ok": false, "error": "disk full" }, { "path": "x.png", "ok": false, "error": { "message": "missing" } }, { "ok": true }]);
        assert_eq!(item_failures(&items), "#4: disk full; x.png: missing");
        assert!(
            cleanup_message(&serde_json::json!({"cleanup_pending": ["blocked"]}))
                .contains("blocked")
        );
    }
    fn visible_landmarks(app: &mut Shelf, ctx: &egui::Context) -> Vec<egui::Pos2> {
        fn find(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text().contains(needle) => Some(text.pos),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, needle)),
                _ => None,
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        let positions = ["Katran ·", "Захват на паузе", "image3.bin"]
            .iter()
            .map(|needle| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| find(&shape.shape, needle))
                    .expect("layout landmark must be painted")
            })
            .collect();
        output.textures_delta.clear();
        positions
    }
    #[test]
    fn reveal_rows_only_at_viewport_edges() {
        assert_eq!(ensure_visible_offset(300.0, 450.0, 450.0, 142.0), 300.0);
        assert_eq!(ensure_visible_offset(300.0, 450.0, 608.0, 142.0), 300.0);
        assert_eq!(ensure_visible_offset(300.0, 450.0, 616.0, 142.0), 308.0);
        assert_eq!(ensure_visible_offset(300.0, 450.0, 150.0, 142.0), 150.0);
        assert_eq!(ensure_visible_offset(0.0, 100.0, 450.0, 212.0), 450.0);
    }
    #[test]
    fn polling_keeps_deadline_and_refresh_requires_captured_items() {
        assert_eq!(poll_delay(Duration::ZERO), Duration::from_millis(250));
        assert_eq!(
            poll_delay(Duration::from_millis(200)),
            Duration::from_millis(50)
        );
        assert_eq!(poll_delay(Duration::from_secs(1)), Duration::ZERO);
        for reply in [
            serde_json::json!({"captured":0}),
            serde_json::json!({"state":"retry"}),
            Value::Null,
        ] {
            assert!(!clipboard_changed(&reply));
        }
        assert!(clipboard_changed(&serde_json::json!({"captured":1})));
    }
    #[test]
    fn keyboard_gallery_range_preview_and_actions() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        // A stale visual selection must never start a native clipboard write.
        app.state = serde_json::json!({"selected": [999], "cursor": 999});
        app.run("copy".into(), &egui::Context::default());
        assert!(app.api.is_some());
        assert!(app.message.contains("устарел"));
        app.message.clear();
        for n in 0..4 {
            app.call(Command::IngestBytes {
                name: format!("image{n}.bin"),
                bytes: vec![n],
                source: "test".into(),
            });
        }
        app.refresh();
        // A pending job keeps the same header, footer, and shelf geometry.
        let layout_ctx = egui::Context::default();
        ui_style::install(&layout_ctx);
        let normal = visible_landmarks(&mut app, &layout_ctx);
        let held_api = app.api.take().unwrap();
        let (pending_tx, pending_rx) = mpsc::channel();
        app.job = Some(pending_rx);
        app.last_poll = Instant::now() - Duration::from_secs(1);
        let busy = visible_landmarks(&mut app, &layout_ctx);
        assert_eq!(normal, busy);
        let selected_id = app.events[0]["id"].as_i64().unwrap();
        let selected_revision = app.events[0]["revision"].as_str().unwrap().to_owned();
        app.state["selected"] = serde_json::json!([selected_id]);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            events: vec![egui::Event::Copy],
            ..Default::default()
        };
        let mut output = layout_ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
        assert!(app.message.contains("очереди"));
        assert_eq!(app.pending_copy.as_ref().unwrap()[0].id, selected_id);
        // A later selection or repeated shortcut cannot replace the queued snapshot.
        app.state["selected"] = serde_json::json!([app.events[1]["id"]]);
        frame(&mut app, &layout_ctx, egui::Key::C, egui::Modifiers::CTRL);
        pending_tx
            .send((
                held_api,
                serde_json::json!({"ok": true, "result": {"captured": 0}}),
                true,
            ))
            .unwrap();
        app.finish_job();
        match app.take_queued_copy().unwrap() {
            Command::CopyEventsChecked { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, selected_id);
                assert_eq!(items[0].revision, selected_revision);
            }
            _ => panic!("queued copy must use the checked command"),
        }
        app.state = app.call(Command::Interaction);
        // Move the dispatcher through the actual worker using a harmless command.
        assert!(app.start_job(Command::Status, false, egui::Context::default()));
        assert!(app.api.is_none());
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.api.is_none() && Instant::now() < deadline {
            app.finish_job();
            std::thread::yield_now();
        }
        assert!(app.api.is_some());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::ArrowRight, egui::Modifiers::NONE);
        assert_eq!(app.state["cursor"], 3);
        frame(
            &mut app,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::SHIFT,
        );
        assert_eq!(app.state["selected"], serde_json::json!([3, 2]));
        frame(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(app.state["preview"], 2);
        frame(&mut app, &ctx, egui::Key::Space, egui::Modifiers::NONE);
        assert!(app.state["preview"].is_null());
        // A second tap requires release; holding Space must not flicker modes.
        let mut released = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        released.textures_delta.clear();
        frame(&mut app, &ctx, egui::Key::Space, egui::Modifiers::NONE);
        assert_eq!(app.state["preview"], 2);
        frame(&mut app, &ctx, egui::Key::ArrowRight, egui::Modifiers::NONE);
        assert_eq!(app.state["preview"], 1);
        frame(&mut app, &ctx, egui::Key::ArrowLeft, egui::Modifiers::NONE);
        assert_eq!(app.state["preview"], 2);
        frame(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(app.state["preview"].is_null());
        frame(&mut app, &ctx, egui::Key::K, egui::Modifiers::CTRL);
        assert!(app.actions);
        app.action_query = "Вид: поток".into();
        frame(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(!app.actions);
        assert_eq!(app.call(Command::Interaction)["view"], "list");
        // A paused idle frame must not re-query storage every old poll deadline.
        app.call(Command::IngestBytes {
            name: "external.bin".into(),
            bytes: vec![42],
            source: "test".into(),
        });
        app.last_poll = Instant::now() - Duration::from_secs(1);
        frame(&mut app, &ctx, egui::Key::A, egui::Modifiers::NONE);
        assert_eq!(app.events.len(), 4);
        frame(&mut app, &ctx, egui::Key::F5, egui::Modifiers::NONE);
        assert_eq!(app.events.len(), 5);
        // Palette selection requests a reveal once, then stays still on idle frames.
        frame(&mut app, &ctx, egui::Key::K, egui::Modifiers::CTRL);
        frame(&mut app, &ctx, egui::Key::A, egui::Modifiers::NONE);
        assert!(!app.scroll_actions);
        frame(&mut app, &ctx, egui::Key::ArrowDown, egui::Modifiers::NONE);
        assert_eq!(app.action_selected, 1);
        assert!(!app.scroll_actions);
        frame(&mut app, &ctx, egui::Key::K, egui::Modifiers::CTRL);
        // Preview navigation selects its current item; restore a group for the
        // independent batch-delete scenario below.
        app.state = app.call(Command::Select {
            id: 3,
            mode: SelectionMode::Replace,
        });
        app.state = app.call(Command::Select {
            id: 2,
            mode: SelectionMode::Range,
        });
        frame(&mut app, &ctx, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(app.pending_delete, vec![3, 2]);
        assert_eq!(app.events.len(), 5);
        frame(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(app.pending_delete.is_empty());
        assert_eq!(app.events.len(), 3);
        app.call(Command::SetShortcut {
            id: "open".into(),
            binding: Some("F6".into()),
            replace: false,
            reset: false,
        });
        frame(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(app.call(Command::Interaction)["preview"].is_null());
        frame(&mut app, &ctx, egui::Key::F6, egui::Modifiers::NONE);
        assert!(app.call(Command::Interaction)["preview"].is_i64());
        frame(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        frame(&mut app, &ctx, egui::Key::K, egui::Modifiers::CTRL);
        frame(
            &mut app,
            &ctx,
            egui::Key::Comma,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert!(app.shortcut_editor.is_some());
        assert_eq!(app.call(Command::Interaction)["settings_open"], false);
        app.shortcut_editor = None;
        frame(&mut app, &ctx, egui::Key::Comma, egui::Modifiers::CTRL);
        assert_eq!(app.call(Command::Interaction)["settings_open"], true);

        // Production handoff: a worker prepares files, then the window-thread
        // hook calls the same dispatcher using a synthetic native boundary.
        struct FakeDrag(
            std::thread::ThreadId,
            std::sync::Arc<std::sync::atomic::AtomicUsize>,
        );
        impl katran::drag_out::Runner for FakeDrag {
            fn run(
                &mut self,
                paths: &[PathBuf],
                _: Option<&katran::drag_out::Preview>,
                owner: u64,
            ) -> Result<katran::drag_out::Outcome, String> {
                assert_eq!(std::thread::current().id(), self.0);
                assert_eq!(owner, 77);
                assert_eq!(paths.len(), 1);
                assert!(paths[0].is_file());
                self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(katran::drag_out::Outcome {
                    dropped: true,
                    warnings: vec![],
                })
            }
        }
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        app.api.as_mut().unwrap().set_drag_runner(Box::new(FakeDrag(
            std::thread::current().id(),
            calls.clone(),
        )));
        app.start_drag(vec![1], &ctx);
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.job.is_some() && Instant::now() < deadline {
            app.finish_job();
            std::thread::yield_now();
        }
        assert!(app.api.is_some());
        assert!(app.native_drag.is_some());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        let mut input = egui::RawInput::default();
        app.perform_native_drag(&ctx, &mut input);
        assert!(app.native_drag.is_none());
        assert!(app.api.is_some());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(app.message.contains("завершено"));
        assert!(
            input
                .events
                .iter()
                .any(|e| matches!(e, egui::Event::PointerButton { pressed: false, .. }))
        );
    }
}

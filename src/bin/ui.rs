#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod action_icons;
mod action_row;
mod api_client;
mod app_identity;
#[cfg(feature = "clipboard-events")]
mod clipboard_events_ui;
mod gallery_card;
mod glass_lab;
mod images;
mod info_bar;
mod input_log;
mod markdown_ui;
mod native_drag_ui;
mod navigation_input;
mod open_apps_ui;
mod overlay_ui;
mod palette;
mod pinned_ui;
mod pods_ui;
mod restart_ui;
mod settings_ui;
mod shelf_view;
mod shortcuts_ui;
mod single_instance;
mod sorting_input;
mod sorting_ui;
mod source_jobs;
mod tab_bar;
mod table_preview_ui;
mod text_preview_ui;
mod theme_file_picker;
mod theme_ui;
mod ui_fonts;
mod ui_foundation;
mod ui_style;
mod updates_ui;
mod window_material;
mod workspace_ui;
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
    concat!("Katran · EVENTS TEST · ", env!("CARGO_PKG_VERSION"))
} else if cfg!(feature = "clipboard-events") {
    concat!("Katran · ", env!("CARGO_PKG_VERSION"))
} else {
    concat!("Katran · ", env!("CARGO_PKG_VERSION"), " · Legacy Polling")
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
            if let Some(path) = item["recovery_path"].as_str() {
                format!("{name}: {error} · Файл сохранён: {path}")
            } else {
                format!("{name}: {error}")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}
fn cleanup_message(result: &Value) -> String {
    fn collect(value: &Value, pending: &mut Vec<String>) {
        if pending.len() >= 128 {
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if key == "cleanup_pending" {
                        if let Some(paths) = child.as_array() {
                            pending.extend(
                                paths
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .take(128 - pending.len())
                                    .map(str::to_owned),
                            );
                        }
                    } else {
                        collect(child, pending);
                    }
                }
            }
            Value::Array(items) => {
                for child in items {
                    collect(child, pending);
                }
            }
            _ => {}
        }
    }
    let mut pending = Vec::new();
    collect(result, &mut pending);
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
enum UiJob {
    Domain {
        job_id: katran::domain_jobs::JobId,
        capture: bool,
    },
    // Synthetic transport-loss fixtures only; production never moves a Service.
    #[cfg(test)]
    UnavailableTransport(mpsc::Receiver<(Service, Value, bool)>),
}
struct Shelf {
    #[cfg(feature = "clipboard-events")]
    clipboard_events: clipboard_events_ui::State,
    instance: Option<single_instance::Primary>,
    activation_rx: Option<mpsc::Receiver<()>>,
    activation_pending: bool,
    native_drag: Option<String>,
    navigation_input: navigation_input::State,
    overlay_ui: overlay_ui::State,
    pods_ui: pods_ui::State,
    native_window: u64,
    logger: Option<katran::diagnostics::Logger>,
    pending_delete: Vec<i64>,
    pending_delete_originals: bool,
    pending_delete_permanent: bool,
    pending_delete_recycle: bool,
    settings_ui: settings_ui::State,
    open_apps: open_apps_ui::State,
    workspace: workspace_ui::State,
    info: info_bar::State,
    pinned_ui: pinned_ui::State,
    shortcut_editor: Option<shortcuts_ui::Editor>,
    api: Option<Service>,
    job: Option<UiJob>,
    capture_job: Option<katran::domain_jobs::JobId>,
    preview_warmup: Option<katran::domain_jobs::JobId>,
    pending_drops: Vec<PathBuf>,
    pending_copy: Option<Vec<katran::SelectionRef>>,
    pending_copy_ids: Option<Vec<i64>>,
    sources: source_jobs::Sources,
    busy_text_editing: bool,
    pending_tab_action: Option<String>,
    tab_order: Vec<katran::interaction::ShelfTab>,
    shortcuts: Vec<katran::shortcuts::ShortcutInfo>,
    gallery_columns: u8,
    pods_tile_size: u16,
    stream_rows: u8,
    pending_density: Vec<String>,
    images: Images,
    text_preview: text_preview_ui::TextPreview,
    table_preview: table_preview_ui::TablePreview,
    events: Vec<Value>,
    next_cursor: Option<String>,
    sorting_input: sorting_input::State,
    pending_sort: Option<String>,
    glass_active: bool,
    transparent_viewport: bool,
    sort_notice: Option<(String, Instant)>,
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
// Keep labels and thumbnails usable in narrow windows without changing the preference.
fn effective_gallery_columns(width: f32, desired: u8) -> usize {
    let fit = ((width.max(0.0) + gallery_card::GAP) / (96.0 + gallery_card::GAP))
        .floor()
        .max(1.0) as usize;
    usize::from(desired.clamp(1, 12)).min(fit)
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
    /// Shared input ownership includes overlays before any early command handler.
    fn modal_owns_input(&self) -> bool {
        self.sorting_input.is_open()
            || self.shortcut_editor.is_some()
            || self.actions
            || self.open_apps.open
            || self.workspace.open
            || self.info.is_open()
            || self.pinned_ui.open
            || self.save
            || !self.pending_delete.is_empty()
            || self.state["settings_open"] == true
    }
    fn render(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        if self.api.is_some() {
            let settings = self.call(Command::GetSettings);
            if let Ok(theme) =
                serde_json::from_value::<katran::theme::ThemePreferences>(settings["theme"].clone())
            {
                if self.glass_active
                    && (!theme.glass || theme.preset != katran::theme::Preset::BlueExperimental)
                {
                    let _ = window_material::apply(self.native_window as isize, false);
                    self.glass_active = false;
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("native-glass-active"), false));
                    theme_ui::reset(&ctx);
                }
                if let Some(id) = &theme.custom_background
                    && !theme_ui::custom_cached(&ctx, id)
                {
                    let background = self.call(Command::GetThemeBackground { id: id.clone() });
                    if let Some(path) = background["path"].as_str()
                        && let Err(error) =
                            theme_ui::load_custom(&ctx, id, std::path::Path::new(path))
                    {
                        self.message = error;
                    }
                }
                theme_ui::apply(&ctx, &theme);
            }
        }
        theme_ui::background(&ctx, ctx.content_rect());
        self.resident_close_requested(&ctx);
        input_log::record(&ctx, self.logger.as_ref());
        if self.api.is_some() {
            self.state = self.call(Command::Interaction);
        }
        let sort_allowed = self.shortcut_editor.is_none()
            && !self.actions
            && !self.open_apps.open
            && !self.workspace.open
            && !self.info.is_open()
            && !self.pinned_ui.open
            && !self.save
            && self.pending_delete.is_empty()
            && self.state["settings_open"] != true
            && self.state["preview"].is_null()
            && !ctx.memory(|m| m.has_focus(egui::Id::new("shelf-search")))
            && (self.sorting_input.is_open() || !ctx.egui_wants_keyboard_input())
            && !(self.api.is_none() && self.busy_text_editing);
        if let Some(action) = self.sorting_input.take(&ctx, sort_allowed) {
            self.pending_sort = Some(action);
        }
        if !self.sorting_input.is_open() {
            self.overlay_escape(&ctx);
        }

        if self.api.is_some() && !self.info.is_open() {
            if !self.sorting_input.is_open() {
                self.pinned_input(&ctx);
            }
            self.pinned_tick(&ctx);
            if !self.pinned_ui.open && !self.sorting_input.is_open() {
                self.workspace_input(&ctx);
            }
            self.workspace_tick(&ctx);
        }
        if self.api.is_none()
            && self.job.is_some()
            && self.shortcut_editor.is_none()
            && !self.busy_text_editing
            && !self.actions
            && !self.sorting_input.is_open()
            && !self.open_apps.open
            && !self.workspace.open
            && !self.info.is_open()
            && !self.pinned_ui.open
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
        if self.api.is_some() {
            self.state = self.call(Command::Interaction);
            self.shortcuts =
                serde_json::from_value(self.call(Command::Shortcuts)).unwrap_or_default();
            self.direct_transfer_input(&ctx);
        }
        let tab_input_allowed = self.shortcut_editor.is_none()
            && !self.actions
            && !self.open_apps.open
            && !self.workspace.open
            && !self.info.is_open()
            && !self.pinned_ui.open
            && !self.save
            && self.pending_delete.is_empty()
            && self.state["settings_open"] != true
            && !ctx.memory(|m| m.has_focus(egui::Id::new("shelf-search")));
        if tab_input_allowed && !self.sorting_input.is_open() {
            if ctx.input_mut(|i| i.events.iter().any(|event| matches!(event, egui::Event::Key {key:egui::Key::Tab,pressed:true,modifiers,..} if *modifiers == egui::Modifiers::NONE)) && i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                self.pending_tab_action = Some("tab_toggle".into());
                if let Some(id) = ctx.memory(|m| m.focused()) {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
            } else {
                let tabs: Vec<_> = self
                    .shortcuts
                    .iter()
                    .filter(|s| {
                        s.id.starts_with("tab_position_")
                            || matches!(
                                s.id.as_str(),
                                "all" | "images" | "screenshots" | "downloads" | "text" | "links" | "tables" | "pods" | "tab_previous" | "tab_next"
                            )
                    })
                    .cloned()
                    .collect();
                if let Some(action) = shortcuts_ui::take_action_with_editing(&ctx, &tabs, false) {
                    self.pending_tab_action = Some(action);
                }
            }
        }
        if self.api.is_none()
            && self.job.is_some()
            && self.shortcut_editor.is_none()
            && !self.busy_text_editing
            && !self.actions
            && !self.sorting_input.is_open()
            && !self.open_apps.open
            && !self.workspace.open
            && !self.info.is_open()
            && !self.pinned_ui.open
            && !self.save
            && self.pending_delete.is_empty()
            && self.state["settings_open"] != true
            && self.state["preview"].is_null()
        {
            let density: Vec<_> = self
                .shortcuts
                .iter()
                .filter(|s| s.id == "density_more" || s.id == "density_less")
                .cloned()
                .collect();
            if let Some(action) = shortcuts_ui::take_action_with_editing(&ctx, &density, false) {
                self.pending_density.push(action);
            }
        }
        self.finish_job();
        self.preview_warmup_tick();
        if self.api.is_some() {
            self.open_apps_tick(&ctx);
        }
        if self.api.is_some() {
            for action in std::mem::take(&mut self.pending_density) {
                self.run(action, &ctx);
            }
        }
        if self.api.is_some()
            && let Some(action) = self.pending_tab_action.take()
        {
            self.run(action, &ctx);
        }
        if self.api.is_some()
            && let Some(action) = self.pending_sort.take()
        {
            self.run(action, &ctx);
        }
        if self.native_drag.is_some() {
            ctx.request_repaint();
        }
        if self.api.is_some()
            && self.job.is_none()
            && let Some(command) = self.take_queued_copy()
        {
            self.start_job(command, false, ctx.clone());
        }
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect::<Vec<_>>()
        });
        self.pending_drops.extend(dropped);
        if self.api.is_some() && self.job.is_none() && !self.pending_drops.is_empty() {
            let path = self.pending_drops.remove(0);
            self.start_job(
                Command::IngestPath {
                    path,
                    source: "drop".into(),
                },
                false,
                ctx.clone(),
            );
        }
        if self.api.is_some() {
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
                && !self.workspace.open
                && !self.info.is_open()
                && !self.pinned_ui.open
                && !self.save
                && self.pending_delete.is_empty()
            {
                self.shortcuts =
                    serde_json::from_value(self.call(Command::Shortcuts)).unwrap_or_default();
                let catalogue: Vec<_> = self
                    .shortcuts
                    .iter()
                    .filter(|s| {
                        (!self.actions || s.id == "settings")
                            && (!self.open_apps.open || s.id == "open_with")
                            && (self.state["settings_open"] != true || s.id == "copy")
                    })
                    .cloned()
                    .collect();
                if !self.sorting_input.is_open()
                    && let Some(id) = shortcuts_ui::take_action(&ctx, &catalogue)
                {
                    self.run(id, &ctx);
                }
            }
        }
        if self.api.is_some() {
            if !self.open_apps.open
                && !self.workspace.open
                && !self.info.is_open()
                && !self.pinned_ui.open
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K))
            {
                self.actions = !self.actions;
                self.focus_actions = self.actions;
                self.action_query.clear();
                self.action_selected = 0;
            }
            if self.shortcut_editor.is_none()
                && !self.open_apps.open
                && !self.workspace.open
                && !self.info.is_open()
                && !self.pinned_ui.open
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                if !self.pending_delete.is_empty() {
                    self.pending_delete.clear();
                } else if self.actions {
                    self.actions = false;
                } else if self.save {
                    self.save = false;
                } else if self.state["preview"].is_null()
                    && self.state["settings_open"] != true
                    && !ctx.egui_wants_keyboard_input()
                    && self.state["selected"]
                        .as_array()
                        .is_some_and(|selected| !selected.is_empty())
                {
                    self.state = self.call(Command::ClearSelection);
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
        let _header = egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(theme_ui::color(
                        &ctx,
                        theme_ui::Role::Background,
                        Color32::TRANSPARENT,
                    ))
                    .inner_margin(12),
            )
            .show(root, |ui| {
                theme_ui::decorate(ui, ui.max_rect());
                if theme_ui::compact_header(&ctx) {
                    let (_, drag) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 8.0),
                        egui::Sense::drag(),
                    );
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                } else {
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
                            self.close_or_hide(&ctx);
                        }
                    });
                }
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id(egui::Id::new("shelf-search"))
                        .hint_text("Поиск на полке…")
                        .font(egui::FontId::proportional(22.0))
                        .frame(egui::Frame::NONE)
                        .desired_width(ui.available_width()),
                );
                if search.changed() {
                    self.refresh();
                }
                if self.api.is_some() {
                    let tabs = self.call(Command::Tabs);
                    let text_enabled = tabs["tabs"].as_array().is_some_and(|tabs| {
                        tabs.iter()
                            .any(|tab| tab["tab"] == "text" && tab["enabled"] == true)
                    });
                    let link_enabled = tabs["tabs"].as_array().is_some_and(|tabs| {
                        tabs.iter()
                            .any(|tab| tab["tab"] == "links" && tab["enabled"] == true)
                    });
                    let markdown_preview = self.state["preview"].as_i64().is_some_and(|id| {
                        self.events.iter().any(|event| {
                            event["id"] == id
                                && event["name"].as_str().is_some_and(|name| {
                                    katran::file_types::is_markdown(std::path::Path::new(name))
                                })
                        })
                    });
                    self.text_preview.set_enabled(
                        text_enabled
                            || link_enabled
                            || markdown_preview
                            || tabs["tabs"].as_array().is_some_and(|tabs| {
                                tabs.iter()
                                    .any(|tab| tab["tab"] == "pods" && tab["enabled"] == true)
                            }),
                    );
                    let tables_enabled = tabs["tabs"].as_array().is_some_and(|tabs| {
                        tabs.iter()
                            .any(|tab| tab["tab"] == "tables" && tab["enabled"] == true)
                    });
                    self.table_preview.set_enabled(tables_enabled);
                    if let Ok(order) = serde_json::from_value(tabs["visible_tabs"].clone()) {
                        self.tab_order = order;
                    }
                }
                if let Some(command) =
                    tab_bar::show(ui, &self.tab_order, &self.state["tab"], self.api.is_some())
                {
                    self.call(command);
                    self.state = self.call(Command::Interaction);
                    self.refresh();
                }
                if self.next_cursor.is_some() && ui.button("Загрузить ещё").clicked() {
                    self.load_more();
                }
            });
        #[cfg(test)]
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("test-header-rect"), _header.response.rect)
        });
        let _footer = egui::Panel::bottom("footer")
            .exact_size(34.0)
            .frame(
                egui::Frame::new()
                    .fill(theme_ui::footer_fill(&ctx, Color32::from_rgb(54, 54, 58)))
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    self.info_bar(ui);
                    let selected = self.state["selected"].as_array().map_or(0, Vec::len);
                    if selected > 0 {
                        ui.weak(format!("{selected} выбрано"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let response = ui_style::actions_button(ui, self.actions);
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::new("actions-button-rect"), response.rect)
                        });
                        if response.clicked() {
                            self.actions = !self.actions;
                            self.focus_actions = self.actions;
                            self.action_query.clear();
                        }
                        let sort = ui_style::status_icon(
                            ui,
                            egui_lucide::Lucide::ArrowUpDown,
                            "Сортировка · запятая",
                        );
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("sorting-button-rect"), sort.rect)
                        });
                        if sort.clicked() {
                            self.actions = false;
                            self.sorting_input.open();
                        }
                    });
                });
            });
        #[cfg(test)]
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("test-footer-rect"), _footer.response.rect)
        });
        if self.state["tab"] == "folder" {
            if self.state["folder_path"].is_null() {
                self.events.clear();
                self.next_cursor = None;
            } else if self
                .workspace
                .last_refresh
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
            {
                self.workspace.last_refresh = Some(Instant::now());
                self.refresh();
            }
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        let visible = self.events.clone();
        self.state = self.call(Command::SetVisible {
            ids: visible.iter().filter_map(|e| e["id"].as_i64()).collect(),
        });
        let gallery = self.state["view"] == "gallery" || self.state["tab"] == "pods";
        if self.api.is_some() {
            let settings = self.call(Command::GetSettings);
            self.gallery_columns = settings["gallery_columns"]
                .as_u64()
                .unwrap_or(4)
                .clamp(1, 12) as u8;
            self.stream_rows = settings["stream_rows"].as_u64().unwrap_or(3).clamp(1, 12) as u8;
            self.pods_tile_size = settings["pods_tile_size"]
                .as_u64()
                .unwrap_or(96)
                .clamp(48, 192) as u16;
        }
        let columns = if self.state["tab"] == "pods" {
            pods_ui::columns(root.available_width() - 40.0, self.pods_tile_size)
        } else if gallery {
            effective_gallery_columns(root.available_width() - 40.0, self.gallery_columns)
        } else {
            1
        };
        if self.api.is_some()
            && !self.actions
            && !self.sorting_input.is_open()
            && !self.open_apps.open
            && !self.workspace.open
            && !self.info.is_open()
            && !self.pinned_ui.open
            && !self.save
            && self.pending_delete.is_empty()
        {
            let document_preview = self.state["preview"].as_i64().is_some_and(|id| {
                self.events.iter().any(|event| {
                    event["id"] == id
                        && (event["kind"] == "text_fragment"
                            || event["name"].as_str().is_some_and(|name| {
                                katran::file_types::is_markdown(std::path::Path::new(name))
                            }))
                })
            });
            self.text_preview
                .handle_view_shortcuts(&ctx, document_preview);
            let table_preview = self.state["preview"].as_i64().is_some_and(|id| {
                self.events
                    .iter()
                    .any(|event| event["id"] == id && event["kind"] == "table_fragment")
            });
            self.table_preview
                .handle_view_shortcuts(&ctx, table_preview);
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
        self.sort_notice(&ctx);
        if self.api.is_some() {
            self.info_panel(&ctx);
            self.workspace_panel(&ctx);
            self.pinned_panel(&ctx);
            self.sort_menu(&ctx);
            self.palette(&ctx);
            self.open_apps_panel(&ctx);
        }
        if let Some(api) = self.api.as_mut()
            && self.settings_ui.show(&ctx, api)
        {
            self.state = self.call(Command::Interaction);
            self.refresh();
            ctx.request_repaint();
        }
        if self.api.is_some()
            && self.job.is_none()
            && self.capture_job.is_none()
            && !self.overlay_ui.pending
            && !self.settings_ui.background_busy()
            && self.settings_ui.take_restart_requested()
        {
            let result = self.call(Command::RestartApplication);
            if result["restart_requested"] == true {
                match restart_ui::spawn_successor() {
                    Ok(()) => self.quit_application(&ctx),
                    Err(error) => self.message = error,
                }
            }
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.api.is_some() && !self.pending_delete.is_empty() {
            let result=egui::Modal::new(egui::Id::new("delete-confirmation")).show(&ctx,|ui| {
                if self.pending_delete_originals {
                    ui.heading(if self.pending_delete_recycle && !self.pending_delete_permanent { "Переместить файлы в корзину?" } else { "Удалить оригиналы с диска?" });
                    ui.label(format!("Выбрано объектов: {}.", self.pending_delete.len()));
                    if self.pending_delete_permanent { ui.label("Оригиналы снимков ShareX будут удалены безвозвратно, без корзины."); }
                    if self.pending_delete_recycle { ui.label("Выбранные оригиналы будут перемещены в корзину Windows."); }
                    ui.label("Остальные выбранные объекты будут убраны с полки.");
                } else {
                    ui.heading(format!("Убрать с полки {} объектов?",self.pending_delete.len()));
                    ui.label("Исходные файлы останутся на месте. Временные копии без других ссылок будут удалены.");
                }
                let enter=ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Enter));
                let label = if self.pending_delete_recycle && !self.pending_delete_permanent {"В корзину · Enter"} else if self.pending_delete_originals {"Удалить оригиналы · Enter"} else {"Убрать · Enter"};
                if ui.button(label).clicked()||enter {
                    let ids=self.pending_delete.clone();
                    if self.start_job(Command::DeleteSelection{ids,confirmed:true},false,ctx.clone()) {self.pending_delete.clear();}
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
                    let destination = PathBuf::from(&self.destination);
                    if self.start_job(Command::Import { ids, destination }, false, ctx.clone()) {
                        self.save = false;
                        self.message = "Сохранение копий…".into();
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
            && !self.sorting_input.is_open()
            && !self.open_apps.open
            && !self.workspace.open
            && !self.info.is_open()
            && !self.pinned_ui.open
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
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        if self.glass_active {
            [0.0, 0.0, 0.0, 0.0]
        } else if self.transparent_viewport {
            Color32::from_rgb(12, 12, 12).to_normalized_gamma_f32()
        } else {
            Color32::from_rgba_unmultiplied(12, 12, 12, 180).to_normalized_gamma_f32()
        }
    }
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.tray_tick(ctx);
        self.overlay_tick(ctx);
        self.finish_job();
        self.preview_warmup_tick();
        if self.api.is_some() {
            self.open_apps_tick(ctx);
        }
        if self.job.is_some() || self.capture_job.is_some() || self.preview_warmup.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if let Some(rx) = &self.activation_rx {
            self.activation_pending |= rx.try_iter().next().is_some();
        }
        if self.activation_pending {
            if self.api.is_some() {
                let reply = self.call(Command::ActivateWindow);
                if reply["activation_requested"] == true {
                    self.call(Command::SetWindowVisibility { visible: true });
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
        self.source_tick(ctx);
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        // Eframe invokes this before Context::run_ui, outside egui locks.
        self.finish_job();
        self.perform_native_drag(ctx, input);
        let starts_sort=!self.modal_owns_input() && !ctx.egui_wants_keyboard_input() && input.events.iter().any(|e|matches!(e,
            egui::Event::Key{key,physical_key,pressed:true,modifiers,..}
                if physical_key.unwrap_or(*key)==egui::Key::Comma && !modifiers.ctrl && !modifiers.alt && !modifiers.command));
        let modal = self.modal_owns_input() || starts_sort;
        let copying_bound = self.shortcuts.iter().any(|s| {
            s.id == "transfer_copy"
                && s.binding
                    .as_deref()
                    .is_some_and(|b| b.split(" / ").any(|b| b == "Ctrl Tab"))
        });
        let editing = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if copying_bound && input.focused && !modal && !editing {
            let mut copy = false;
            input.events.retain(|e| match e {
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } if *modifiers == egui::Modifiers::CTRL => {
                    copy |= !*repeat;
                    false
                }
                _ => true,
            });
            if copy {
                self.pending_tab_action = Some("transfer_copy".into());
            }
        }
        if self.navigation_input.intercept(ctx, input, modal) {
            self.pending_tab_action = Some("tab_toggle".into());
        }
    }
    fn ui(&mut self, root: &mut egui::Ui, _: &mut eframe::Frame) {
        self.render(root);
    }
}
fn main() -> eframe::Result {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--glass-lab")) {
        return glass_lab::run();
    }
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--glass-lab-render")) {
        return glass_lab::export()
            .map_err(|e| eframe::Error::AppCreation(Box::new(std::io::Error::other(e))));
    }

    restart_ui::wait_for_predecessor()
        .map_err(|error| eframe::Error::AppCreation(Box::new(std::io::Error::other(error))))?;
    if let Some(code) = katran::shell_visual_worker::run_if_requested() {
        std::process::exit(code);
    }
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
        navigation_input: navigation_input::State::default(),
        overlay_ui: overlay_ui::State::default(),
        pods_ui: pods_ui::State::default(),
        native_window: 0,
        logger,
        pending_delete: Vec::new(),
        pending_delete_originals: false,
        pending_delete_permanent: false,
        pending_delete_recycle: false,
        settings_ui: settings_ui::State::default(),
        open_apps: open_apps_ui::State::default(),
        workspace: workspace_ui::State::default(),
        info: info_bar::State::default(),
        pinned_ui: pinned_ui::State::default(),
        shortcut_editor: None,
        api: Some(api),
        job: None,
        capture_job: None,
        preview_warmup: None,
        pending_drops: Vec::new(),
        pending_copy: None,
        pending_copy_ids: None,
        sources: source_jobs::Sources::default(),
        busy_text_editing: false,
        pending_tab_action: None,
        tab_order: katran::tab_layout::default_order(),
        shortcuts: Vec::new(),
        gallery_columns: 4,
        pods_tile_size: 96,
        stream_rows: 3,
        pending_density: Vec::new(),
        images: Images::new(root.clone()),
        text_preview: text_preview_ui::TextPreview::new(root.clone()),
        table_preview: table_preview_ui::TablePreview::new(root),
        events: vec![],
        next_cursor: None,
        sorting_input: Default::default(),
        pending_sort: None,
        glass_active: false,
        transparent_viewport: false,
        sort_notice: None,
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
    let warmup = app.call(Command::BeginJob {
        command: Box::new(Command::EnsurePreviewBackend),
    });
    app.preview_warmup = warmup["job_id"].as_u64().map(katran::domain_jobs::JobId);
    let theme: katran::theme::ThemePreferences =
        serde_json::from_value(app.call(Command::GetSettings)["theme"].clone()).unwrap_or_default();
    let glass_start = theme.preset == katran::theme::Preset::BlueExperimental && theme.glass;
    app.transparent_viewport = glass_start;
    eframe::run_native(
        "Katran",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(app_identity::window_icon())
                .with_inner_size([940.0, 680.0])
                .with_min_inner_size([640.0, 440.0])
                .with_decorations(false)
                .with_transparent(glass_start),
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx
                .options_mut(|options| options.zoom_with_keyboard = false);
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
                    if glass_start {
                        match window_material::apply(app.native_window as isize, true) {
                            Ok(()) => {
                                app.glass_active = true;
                                cc.egui_ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("native-glass-active"), true)
                                });
                            }
                            Err(error) => app.message = error,
                        }
                    }
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
        struct NoExternalPreview;
        impl katran::quicklook::Launcher for NoExternalPreview {
            fn launch(&self, _: &std::path::Path) -> katran::api::Result<()> {
                Err(katran::api::ApiError {
                    code: "test_preview_disabled".into(),
                    message: "External preview disabled in tests".into(),
                })
            }
        }
        struct NoExternalFile;
        impl katran::open_file::Launcher for NoExternalFile {
            fn launch(
                &self,
                _: &std::path::Path,
                _: katran::open_file::Operation,
            ) -> katran::api::Result<()> {
                Err(katran::api::ApiError {
                    code: "test_file_disabled".into(),
                    message: "External file disabled in tests".into(),
                })
            }
        }
        struct FakeApps;
        impl katran::app_handlers::Provider for FakeApps {
            fn discover(&self, _: &str) -> katran::Result<Vec<katran::app_handlers::AppHandler>> {
                Ok(vec![])
            }
            fn invoke(&self, _: &str, _: &str, _: &std::path::Path) -> katran::Result<()> {
                Ok(())
            }
        }
        let mut api = Service::open(root, 10000).unwrap();
        api.set_app_provider(Box::new(FakeApps));
        api.set_file_launcher(Box::new(NoExternalFile));
        api.set_preview_launcher(Box::new(NoExternalPreview));
        api.execute(Request {
            api_version: 1,
            request_id: "test-all".into(),
            command: Command::SetTab {
                tab: katran::interaction::ShelfTab::All,
            },
        });
        let mut settings: katran::settings::Settings = serde_json::from_value(
            api.execute(Request {
                api_version: 1,
                request_id: "test-no-personal-folders".into(),
                command: Command::GetSettings,
            })["result"]
                .clone(),
        )
        .unwrap();
        settings.downloads_folder = Some(root.join("synthetic-downloads"));
        api.execute(Request {
            api_version: 1,
            request_id: "test-no-personal-folders".into(),
            command: Command::SetSettings { settings },
        });
        Shelf {
            #[cfg(feature = "clipboard-events")]
            clipboard_events: Default::default(),
            instance: None,
            activation_rx: None,
            activation_pending: false,
            native_drag: None,
            navigation_input: navigation_input::State::default(),
            overlay_ui: overlay_ui::State::default(),
            pods_ui: pods_ui::State::default(),
            native_window: 77,
            logger: None,
            pending_delete: Vec::new(),
            pending_delete_originals: false,
            pending_delete_permanent: false,
            pending_delete_recycle: false,
            settings_ui: settings_ui::State::default(),
            open_apps: open_apps_ui::State::default(),
            workspace: workspace_ui::State::default(),
            info: info_bar::State::default(),
            pinned_ui: pinned_ui::State::default(),
            shortcut_editor: None,
            api: Some(api),
            job: None,
            capture_job: None,
            preview_warmup: None,
            pending_drops: Vec::new(),
            pending_copy: None,
            pending_copy_ids: None,
            sources: source_jobs::Sources::default(),
            busy_text_editing: false,
            pending_tab_action: None,
            tab_order: katran::tab_layout::default_order(),
            shortcuts: Vec::new(),
            gallery_columns: 4,
            pods_tile_size: 96,
            stream_rows: 3,
            pending_density: Vec::new(),
            images: Images::new(root.into()),
            text_preview: text_preview_ui::TextPreview::new(root.into()),
            table_preview: table_preview_ui::TablePreview::new(root.into()),
            events: vec![],
            next_cursor: None,
            sorting_input: Default::default(),
            pending_sort: None,
            glass_active: false,
            transparent_viewport: false,
            sort_notice: None,
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

    #[test]
    fn preview_updates_interaction_without_reloading_shelf() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let id = app.call(Command::IngestBytes {
            name: "synthetic.png".into(),
            bytes: vec![1, 2, 3],
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let logger = katran::diagnostics::Logger::start(root.path()).unwrap();
        app.api.as_mut().unwrap().attach_logger(logger.clone());
        let ctx = egui::Context::default();
        app.run("preview".into(), &ctx);
        assert_eq!(app.state["preview"], id);
        app.run("preview".into(), &ctx);
        assert!(app.state["preview"].is_null());
        app.run("quicklook".into(), &ctx);
        drain_domain_job(&mut app);
        assert_eq!(app.message, "External preview disabled in tests");
        app.run("refresh".into(), &ctx);
        assert!(logger.flush(std::time::Duration::from_secs(2)));
        let log = std::fs::read_to_string(root.path().join("logs/events.jsonl")).unwrap();
        let commands: Vec<String> = log
            .lines()
            .filter_map(|line| {
                let event: Value = serde_json::from_str(line).unwrap();
                (event["kind"] == "command").then(|| {
                    event["data"]["command"]["type"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
            })
            .collect();
        assert_eq!(
            commands
                .iter()
                .filter(|c| c.as_str() == "browse_shelf_sorted")
                .count(),
            1,
            "Preview must not reload catalogue; only explicit Refresh browses"
        );
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c.as_str(), "interaction" | "status")),
            "hot successful reads should not flood diagnostics"
        );
    }

    #[test]
    fn gallery_respects_preference_and_minimum_width() {
        assert_eq!(effective_gallery_columns(900.0, 4), 4);
        assert_eq!(effective_gallery_columns(900.0, 5), 5);
        assert!(effective_gallery_columns(200.0, 12) < 12);
        assert_eq!(effective_gallery_columns(0.0, 12), 1);
    }

    #[test]
    fn density_shortcuts_keep_interface_scale_and_update_settings() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let scale = ctx.pixels_per_point();
        frame(&mut app, &ctx, egui::Key::Minus, egui::Modifiers::CTRL);
        assert_eq!(ctx.pixels_per_point(), scale);
        assert_eq!(app.call(Command::GetSettings)["gallery_columns"], 5);
        frame(&mut app, &ctx, egui::Key::Plus, egui::Modifiers::CTRL);
        assert_eq!(ctx.pixels_per_point(), scale);
        assert_eq!(app.call(Command::GetSettings)["gallery_columns"], 4);
        let mut release = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Plus,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        release.textures_delta.clear();
        frame(
            &mut app,
            &ctx,
            egui::Key::Plus,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert_eq!(ctx.pixels_per_point(), scale);
        assert_eq!(app.call(Command::GetSettings)["gallery_columns"], 3);
    }

    #[test]
    fn reveal_works_from_ctrl_enter_and_quicklook_from_palette() {
        for palette in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let mut app = test_shelf(root.path());
            app.call(Command::SetTab {
                tab: katran::interaction::ShelfTab::Images,
            });
            let event = app.call(Command::IngestBytes {
                name: "selected.png".into(),
                bytes: vec![1],
                source: "test".into(),
            });
            let id = event["id"].as_i64().unwrap();
            app.refresh();
            app.state = app.call(Command::SetVisible { ids: vec![id] });
            app.actions = palette;
            app.action_query = "Открыть через QuickLook".into();
            let ctx = egui::Context::default();
            ui_style::install(&ctx);
            frame(
                &mut app,
                &ctx,
                egui::Key::Enter,
                if palette {
                    egui::Modifiers::NONE
                } else {
                    egui::Modifiers::CTRL
                },
            );
            assert_eq!(
                app.message,
                if palette {
                    "External preview disabled in tests"
                } else {
                    "External file disabled in tests"
                }
            );
            assert!(app.call(Command::Interaction)["preview"].is_null());
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
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let mut output = ctx.run_ui(input, |ui| {
            // Exercise the real UI without any native window or system clipboard.
            app.render(ui);
        });
        output.textures_delta.clear();
        drain_domain_job(app);
    }

    #[test]
    fn escape_closes_overlays_and_preview_before_clearing_selection() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let first = app.call(Command::IngestBytes {
            name: "first.png".into(),
            bytes: vec![1],
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        let second = app.call(Command::IngestBytes {
            name: "second.png".into(),
            bytes: vec![2],
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.refresh();
        app.state = app.call(Command::SetVisible {
            ids: vec![first, second],
        });
        app.call(Command::Select {
            id: first,
            mode: SelectionMode::Replace,
        });
        app.state = app.call(Command::Select {
            id: second,
            mode: SelectionMode::Toggle,
        });
        app.state = app.call(Command::TogglePreview);
        app.actions = true;
        let selected = serde_json::json!([first, second]);
        let cursor = app.state["cursor"].clone();
        let ctx = egui::Context::default();
        ui_style::install(&ctx);

        frame(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(!app.actions);
        assert_eq!(app.state["preview"], second);
        assert_eq!(app.state["selected"], selected);

        frame(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(app.state["preview"].is_null());
        assert_eq!(app.state["selected"], selected);

        frame(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        assert_eq!(app.state["selected"], serde_json::json!([]));
        assert_eq!(app.state["cursor"], cursor);
    }

    #[test]
    fn document_ctrl_arrows_keep_preview_and_selection() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: true,
        });
        for text in ["# Первый", "# Второй"] {
            app.call(Command::IngestText {
                text: text.into(),
                source: "test".into(),
            });
        }
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Text,
        });
        app.refresh();
        let id = app.events[0]["id"].as_i64().unwrap();
        app.call(Command::SetVisible {
            ids: app.events.iter().filter_map(|e| e["id"].as_i64()).collect(),
        });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Space, egui::Modifiers::NONE);
        let before = app.call(Command::Interaction);
        frame(&mut app, &ctx, egui::Key::ArrowRight, egui::Modifiers::CTRL);
        let right = app.call(Command::Interaction);
        assert_eq!(right["preview"], before["preview"]);
        assert_eq!(right["selected"], before["selected"]);
        frame(&mut app, &ctx, egui::Key::ArrowLeft, egui::Modifiers::CTRL);
        let left = app.call(Command::Interaction);
        assert_eq!(left["preview"], before["preview"]);
        assert_eq!(left["selected"], before["selected"]);
    }
    #[test]
    fn tables_space_opens_internal_grid_and_mode_keys_keep_current_item() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Tables,
            enabled: true,
        });
        for s in ["first", "second"] {
            app.call(Command::IngestTable {
                rows: vec![vec![s.into()]],
                source: "test".into(),
            });
        }
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Tables,
        });
        app.refresh();
        let ids: Vec<_> = app
            .events
            .iter()
            .filter_map(|event| event["id"].as_i64())
            .collect();
        let id = ids[0];
        app.call(Command::SetVisible { ids });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Space, egui::Modifiers::NONE);
        assert_eq!(app.call(Command::Interaction)["preview"], id);
        assert!(!app.message.contains("External preview"));
        frame(&mut app, &ctx, egui::Key::ArrowRight, egui::Modifiers::CTRL);
        assert_eq!(app.call(Command::Interaction)["preview"], id);
        frame(&mut app, &ctx, egui::Key::ArrowLeft, egui::Modifiers::CTRL);
        assert_eq!(app.call(Command::Interaction)["preview"], id);
        assert_eq!(
            app.call(Command::Interaction)["selected"],
            serde_json::json!([id])
        );
    }
    fn drain_domain_job(app: &mut Shelf) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while matches!(app.job, Some(UiJob::Domain { .. })) {
            app.finish_job();
            assert!(Instant::now() < deadline, "domain UI job did not complete");
            std::thread::yield_now();
        }
    }
    #[test]
    fn comma_sort_routes_domain_action_and_preserves_text_and_settings() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Comma, egui::Modifiers::NONE);
        frame(&mut app, &ctx, egui::Key::O, egui::Modifiers::SHIFT);
        assert_eq!(app.call(Command::Interaction)["sort"]["mode"], "origin");
        assert_eq!(app.call(Command::Interaction)["sort"]["reverse"], true);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("shelf-search")));
        frame(&mut app, &ctx, egui::Key::Comma, egui::Modifiers::NONE);
        frame(&mut app, &ctx, egui::Key::A, egui::Modifiers::NONE);
        assert_eq!(app.call(Command::Interaction)["sort"]["mode"], "origin");
        let mut released = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Comma,
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
        frame(&mut app, &ctx, egui::Key::Comma, egui::Modifiers::CTRL);
        assert_eq!(app.call(Command::Interaction)["settings_open"], true);
    }
    #[test]
    fn sort_menu_takes_precedence_over_accidental_comma_binding() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        app.call(Command::SetView {
            view: katran::interaction::View::List,
        });
        app.call(Command::SetShortcut {
            id: "gallery".into(),
            binding: Some("Comma".into()),
            replace: false,
            reset: false,
        });
        frame(&mut app, &ctx, egui::Key::Comma, egui::Modifiers::NONE);
        assert_eq!(app.call(Command::Interaction)["view"], "list");
        assert!(app.sorting_input.is_open());
        frame(&mut app, &ctx, egui::Key::O, egui::Modifiers::NONE);
        assert_eq!(app.call(Command::Interaction)["sort"]["mode"], "origin");
        assert!(!app.sorting_input.is_open());
    }
    #[test]
    fn tabs_shortcuts_and_hidden_selection_use_domain_api() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let id = app.call(Command::IngestBytes {
            name: "hidden.png".into(),
            bytes: vec![1],
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        // A worker may own the API when the selected item is temporarily unavailable.
        let api = app.api.take().unwrap();
        let (tx, rx) = mpsc::channel();
        app.job = Some(UiJob::UnavailableTransport(rx));
        app.events.clear();
        app.copy_request(&ctx);
        assert_eq!(app.pending_copy_ids, Some(vec![id]));
        tx.send((
            api,
            serde_json::json!({"ok":true,"result":{"captured":0}}),
            true,
        ))
        .unwrap();
        app.finish_job();
        assert!(
            matches!(app.take_queued_copy(), Some(Command::CopySmart { items }) if items.len()==1 && items[0].id==id)
        );
        frame(&mut app, &ctx, egui::Key::Num2, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "screenshots");
        assert!(app.events.is_empty());
        assert!(app.state["selected"].as_array().unwrap().is_empty());
        assert!(app.state["cursor"].is_null());
        frame(&mut app, &ctx, egui::Key::Num3, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "downloads");
        assert!(app.state["selected"].as_array().unwrap().is_empty());
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "folder");
        app.call(Command::ShowSettings);
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "folder");
        app.call(Command::CloseSettings);
        app.actions = true;
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "folder");
    }
    #[test]
    fn copy_event_and_key_produce_one_shortcut_action() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let shortcuts: Vec<katran::shortcuts::ShortcutInfo> =
            serde_json::from_value(app.call(Command::Shortcuts)).unwrap();
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![
                egui::Event::Copy,
                egui::Event::Key {
                    key: egui::Key::C,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                },
            ],
            ..Default::default()
        };
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(input, |_ui| {
            actions.push(shortcuts_ui::take_action_with_editing(
                &ctx, &shortcuts, false,
            ));
            actions.push(shortcuts_ui::take_action_with_editing(
                &ctx, &shortcuts, false,
            ));
        });
        output.textures_delta.clear();
        assert_eq!(actions, vec![Some("copy".into()), None]);
    }
    #[test]
    fn pinned_tag_picker_filters_and_does_not_leak_navigation() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let id = app.call(Command::IngestBytes {
            name: "pin.png".into(),
            bytes: vec![1],
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::TogglePins { ids: vec![id] });
        app.call(Command::SetPinnedTags {
            id,
            tags: vec!["Work".into()],
        });
        app.state = app.call(Command::SelectTabPosition { position: 1 });
        app.refresh();
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::T, egui::Modifiers::CTRL);
        assert!(app.pinned_ui.open);
        frame(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(!app.pinned_ui.open);
        assert_eq!(app.call(Command::Interaction)["tag_filter"], "work");
        assert!(app.call(Command::Interaction)["preview"].is_null());
    }
    #[test]
    fn folder_modal_history_and_keyboard_do_not_leak() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(
            &mut app,
            &ctx,
            egui::Key::K,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert!(app.workspace.open);
        frame(&mut app, &ctx, egui::Key::Num2, egui::Modifiers::NONE);
        assert_eq!(app.call(Command::Interaction)["tab"], "images");
        frame(&mut app, &ctx, egui::Key::H, egui::Modifiers::CTRL);
        frame(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(!app.workspace.open);
        let state = app.call(Command::Interaction);
        assert_eq!(state["tab"], "folder");
        assert_eq!(
            state["folder_path"].as_str().map(PathBuf::from),
            Some(folder.path().canonicalize().unwrap())
        );
    }
    #[test]
    fn repeated_digit_switches_pinned_without_retaining_selection() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let id = app.call(Command::IngestBytes {
            name: "pinned.png".into(),
            bytes: vec![1],
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::TogglePins { ids: vec![id] });
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Num1, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "images");
        assert_eq!(app.state["pinned_only"], true);
        assert!(app.state["selected"].as_array().unwrap().is_empty());
        assert!(app.events.iter().any(|item| item["id"] == id));
        // A second physical press follows key release; holding a key must not toggle repeatedly.
        let mut released = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Num1,
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
        frame(&mut app, &ctx, egui::Key::Num1, egui::Modifiers::NONE);
        assert_eq!(app.state["pinned_only"], false);
    }
    #[cfg(windows)]
    #[test]
    fn cut_then_tab_and_paste_moves_synthetic_file() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let id = app.call(Command::IngestBytes {
            name: "synthetic.png".into(),
            bytes: b"synthetic bytes".to_vec(),
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::X, egui::Modifiers::CTRL);
        assert!(
            app.message.contains("Подготовлено к перемещению"),
            "cut was not staged: {} state: {}",
            app.message,
            app.state
        );
        assert_eq!(app.call(Command::Get { id })["id"], id);
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "folder");
        frame(&mut app, &ctx, egui::Key::V, egui::Modifiers::CTRL);
        let outputs: Vec<_> = std::fs::read_dir(folder.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(
            outputs.len(),
            1,
            "paste receipt: {}; state: {}; outputs: {:?}",
            app.message,
            app.state,
            outputs
        );
        assert_eq!(
            outputs[0].file_name().unwrap(),
            "synthetic.png",
            "paste receipt: {}",
            app.message
        );
        assert_eq!(std::fs::read(&outputs[0]).unwrap(), b"synthetic bytes");
        assert!(app.message.contains("перемещено"));
        assert!(app.call(Command::Get { id }).is_null());
    }
    #[test]
    fn tab_after_window_refocus_ignores_stale_widget_focus() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        fn dispatch(app: &mut Shelf, ctx: &egui::Context, mut input: egui::RawInput) {
            eframe::App::raw_input_hook(app, ctx, &mut input);
            let mut output = ctx.run_ui(input, |ui| app.render(ui));
            output.textures_delta.clear();
        }
        dispatch(&mut app, &ctx, egui::RawInput::default());
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("some-button")));
        dispatch(
            &mut app,
            &ctx,
            egui::RawInput {
                focused: false,
                events: vec![egui::Event::WindowFocused(false)],
                ..Default::default()
            },
        );
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("shelf-search")));
        dispatch(
            &mut app,
            &ctx,
            egui::RawInput {
                focused: true,
                events: vec![egui::Event::WindowFocused(true)],
                ..Default::default()
            },
        );
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        dispatch(&mut app, &ctx, input);
        assert_eq!(app.call(Command::Interaction)["tab"], "folder");
    }
    #[test]
    fn refocus_preserves_deliberate_search_and_modal_tab_input() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let mut first = egui::RawInput::default();
        eframe::App::raw_input_hook(&mut app, &ctx, &mut first);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("shelf-search")));
        let mut loss = egui::RawInput {
            focused: false,
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut loss);
        ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("shelf-search")));
        let mut gain = egui::RawInput {
            focused: true,
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut gain);
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new("shelf-search"))));
        let tab = || egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut local = tab();
        eframe::App::raw_input_hook(&mut app, &ctx, &mut local);
        assert_eq!(local.events.len(), 1);
        assert!(app.pending_tab_action.is_none());
        ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("shelf-search")));
        app.actions = true;
        let mut modal = tab();
        eframe::App::raw_input_hook(&mut app, &ctx, &mut modal);
        assert_eq!(modal.events.len(), 1);
        assert!(app.pending_tab_action.is_none());
    }
    #[test]
    fn category_arrows_leave_pinned_even_at_boundary() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        app.state = app.call(Command::SelectTabPosition { position: 1 });
        assert_eq!(app.state["pinned_only"], true);
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(
            &mut app,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert_eq!(app.state["tab"], "screenshots");
        assert_eq!(app.state["pinned_only"], false);
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        app.state = app.call(Command::SelectTabPosition { position: 1 });
        frame(
            &mut app,
            &ctx,
            egui::Key::ArrowLeft,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        );
        assert_eq!(app.state["tab"], "images");
        assert_eq!(app.state["pinned_only"], false);
    }
    #[test]
    fn repeated_tab_and_positional_shortcuts_follow_api_order() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::MoveTab {
            tab: katran::interaction::ShelfTab::Downloads,
            index: 0,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Num1, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "downloads");
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::Num2),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
        assert_eq!(app.state["tab"], "images");
        app.call(Command::SelectTabPosition { position: 1 });
        for expected in ["folder", "downloads", "folder", "downloads"] {
            // A focusable button must never trap repeated tab toggling.
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("button-focus")));
            frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
            assert_eq!(app.state["tab"], expected);
        }
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::SHIFT);
        assert_eq!(app.state["tab"], "downloads");
        frame(&mut app, &ctx, egui::Key::F12, egui::Modifiers::NONE);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("shelf-search")));
        frame(&mut app, &ctx, egui::Key::Num2, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "downloads");
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "downloads");
    }
    #[test]
    fn tab_shortcuts_queue_while_dispatcher_is_busy() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Num1, egui::Modifiers::NONE);
        let api = app.api.take().unwrap();
        let (tx, rx) = mpsc::channel();
        app.job = Some(UiJob::UnavailableTransport(rx));
        frame(&mut app, &ctx, egui::Key::Num3, egui::Modifiers::NONE);
        assert_eq!(app.pending_tab_action.as_deref(), Some("tab_position_3"));
        tx.send((api, serde_json::json!({"ok":true,"result":{}}), false))
            .unwrap();
        frame(&mut app, &ctx, egui::Key::F12, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "downloads");
    }
    #[test]
    fn settings_shortcut_uses_rebound_catalogue_in_palette() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetShortcut {
            id: "settings".into(),
            binding: Some("F8".into()),
            replace: false,
            reset: false,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Comma, egui::Modifiers::CTRL);
        assert_eq!(app.call(Command::Interaction)["settings_open"], false);
        app.actions = true;
        frame(&mut app, &ctx, egui::Key::F8, egui::Modifiers::NONE);
        assert_eq!(app.call(Command::Interaction)["settings_open"], true);
    }
    #[test]
    fn background_sharex_notifies_and_commits_without_repeated_scans() {
        let root = tempfile::tempdir().unwrap();
        let shots = tempfile::tempdir().unwrap();
        let folder = shots.path().to_path_buf();
        image::RgbaImage::new(2, 2)
            .save(folder.join("synthetic.png"))
            .unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Screenshots,
        });
        let mut settings: katran::settings::Settings =
            serde_json::from_value(app.call(Command::GetSettings)).unwrap();
        settings.sharex_folder = Some(folder);
        app.call(Command::SetSettings { settings });
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        while (app.events.len() != 1 || app.job.is_some()) && Instant::now() < deadline {
            app.finish_job();
            app.source_tick(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.events.len(), 1, "{}", app.message);
        assert!(app.job.is_none());
        // Idle queue ticks do not rescan or refresh the list.
        app.events.clear();
        for _ in 0..5 {
            app.source_tick(&ctx);
            app.finish_job();
        }
        assert!(app.events.is_empty());
    }
    #[test]
    fn deleting_screenshot_group_requires_confirmation_before_removing_originals() {
        let root = tempfile::tempdir().unwrap();
        let shots = tempfile::tempdir().unwrap();
        let original = shots.path().join("synthetic.png");
        image::RgbaImage::new(2, 2).save(&original).unwrap();
        let mut app = test_shelf(root.path());
        struct SyntheticTrash {
            root: PathBuf,
            count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }
        impl katran::downloads::Trash for SyntheticTrash {
            fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
                let resolved = path.canonicalize()?;
                assert!(resolved.starts_with(&self.root));
                assert!(resolved.components().any(|c| {
                    c.as_os_str()
                        .to_string_lossy()
                        .starts_with(".katran-recovery-")
                }));
                self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::fs::remove_file(resolved)?;
                Ok(())
            }
        }
        let recycled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        app.api
            .as_mut()
            .unwrap()
            .set_trash(Box::new(SyntheticTrash {
                root: shots.path().canonicalize().unwrap(),
                count: recycled.clone(),
            }));
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Screenshots,
        });
        let mut settings: katran::settings::Settings =
            serde_json::from_value(app.call(Command::GetSettings)).unwrap();
        settings.sharex_folder = Some(shots.path().to_path_buf());
        app.call(Command::SetSettings { settings });
        let second = shots.path().join("second.png");
        image::RgbaImage::new(3, 2).save(&second).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut source_ids = Vec::new();
        while source_ids.len() < 2 {
            let result = app.call(Command::PollSources { limit: 8 });
            for item in result["items"].as_array().unwrap() {
                if let Some(id) = item["event"]["id"].as_i64() {
                    source_ids.push(id);
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let id = source_ids[0];
        let second_id = source_ids[1];
        app.refresh();
        app.call(Command::SetVisible {
            ids: vec![id, second_id],
        });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        app.state = app.call(Command::Select {
            id: second_id,
            mode: SelectionMode::Toggle,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(app.pending_delete, vec![id, second_id]);
        assert!(app.pending_delete_originals);
        assert!(original.is_file(), "Delete must only request confirmation");
        assert!(second.is_file());
        assert_eq!(app.events.len(), 2);
        // An ordinary frame must retain the original while displaying the warning.
        fn contains(shape: &egui::Shape, text: &str) -> bool {
            match shape {
                egui::Shape::Text(shape) => shape.galley.text().contains(text),
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains(shape, text)),
                _ => false,
            }
        }
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
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| contains(&shape.shape, "перемещены в корзину Windows"))
        );
        output.textures_delta.clear();
        assert!(original.is_file());
        assert!(second.is_file());
        frame(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(app.pending_delete.is_empty());
        assert!(!original.exists());
        assert!(!second.exists());
        assert_eq!(recycled.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(app.events.is_empty());
        // Partial and complete per-item failures must remain visible to the owner.
        app.action_result(serde_json::json!({"deleted":1,"original_files":true,"items":[{"ok":true,"id":1},{"ok":false,"id":2,"error":{"message":"source changed"}}]}));
        assert!(app.message.contains("Удалено объектов: 1"));
        assert!(app.message.contains("#2: source changed"));
        app.action_result(serde_json::json!({"deleted":0,"original_files":true,"items":[{"ok":false,"id":2,"error":{"message":"source changed"}}]}));
        assert!(app.message.contains("#2: source changed"));
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
    #[test]
    fn settings_window_does_not_block_main_window_copy() {
        struct Sink(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
        impl katran::text_fragments::ClipboardSink for Sink {
            fn write(&self, text: &str) -> katran::Result<u32> {
                self.0.lock().unwrap().push(text.into());
                Ok(42)
            }
        }
        struct Source;
        impl katran::clipboard_files::Source for Source {
            fn sequence(&self) -> u32 {
                41
            }
            fn files(&self) -> katran::Result<Vec<PathBuf>> {
                Ok(vec![])
            }
        }
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let writes = std::sync::Arc::new(std::sync::Mutex::new(vec![
            "synthetic ShareX screenshot".to_owned(),
        ]));
        app.api
            .as_mut()
            .unwrap()
            .set_text_clipboard_sink(Box::new(Sink(writes.clone())));
        app.api
            .as_mut()
            .unwrap()
            .set_clipboard_files_source(Box::new(Source));
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: true,
        });
        let id = app.call(Command::IngestText {
            text: "selected text".into(),
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Text,
        });
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        app.call(Command::ShowSettings);
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::C, egui::Modifiers::CTRL);
        assert_eq!(
            writes.lock().unwrap().last().unwrap(),
            "selected text",
            "message={} state={}",
            app.message,
            app.state
        );
        assert_eq!(app.call(Command::Interaction)["settings_open"], true);
        let count = writes.lock().unwrap().len();
        let mut released = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::C,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        released.textures_delta.clear();
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("shelf-search")));
        frame(&mut app, &ctx, egui::Key::C, egui::Modifiers::CTRL);
        assert_eq!(
            writes.lock().unwrap().len(),
            count,
            "text editing must own copy"
        );
    }
    #[test]
    fn quicklook_ack_does_not_replace_status_with_a_notification() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.message = "existing result".into();
        app.action_result(serde_json::json!({"backend":"quicklook","requested":true}));
        assert_eq!(app.message, "existing result");
    }
    #[test]
    fn experimental_theme_keeps_shell_geometry_and_standard_returns() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        for n in 0..4 {
            app.call(Command::IngestBytes {
                name: format!("image{n}.png"),
                bytes: vec![n],
                source: "test".into(),
            });
        }
        app.refresh();
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let standard = visible_landmarks(&mut app, &ctx);
        app.call(Command::SetTheme {
            theme: katran::theme::ThemePreferences {
                preset: katran::theme::Preset::BlueExperimental,
                accent: None,
                decoration: true,
                lily_background: None,
                custom_background: None,
                glass: false,
                compact_header: false,
            },
        });
        let blue = visible_landmarks(&mut app, &ctx);
        assert_eq!(blue, standard, "theme must not move the existing controls");
        app.call(Command::SetTheme {
            theme: Default::default(),
        });
        assert_eq!(visible_landmarks(&mut app, &ctx), standard);
    }
    #[test]
    fn table_copy_status_reports_partial_formats_and_item_failure() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.action_result(serde_json::json!({"table_copied":false,"merged":true,"html_written":false,"text_written":true,"warnings":["synthetic HTML failure"]}));
        assert!(app.message.contains("значения с табуляциями"));
        assert!(app.message.contains("synthetic HTML failure"));
        app.action_result(serde_json::json!({"table_copied":false,"merged":false,"items":[{"id":5,"ok":false,"error":{"message":"stale table"}}]}));
        assert!(app.message.contains("#5: stale table"));
        app.action_result(
            serde_json::json!({"table_copied":true,"table_count":2,"row_count":5,"column_count":3}),
        );
        assert!(app.message.contains("2 · 5 × 3"));
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
        output.textures_delta.clear();
        let positions = ["Katran ·", "Actions  Ctrl K", "image3.png"]
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
    struct NoTransferClipboard;
    impl katran::clipboard_files::Source for NoTransferClipboard {
        fn sequence(&self) -> u32 {
            panic!("Shift+Tab must not read clipboard");
        }
        fn files(&self) -> katran::Result<Vec<PathBuf>> {
            panic!("Shift+Tab must not read clipboard");
        }
    }
    #[test]
    fn shift_tab_transfers_and_imports_without_clipboard() {
        fn release_tab(app: &mut Shelf, ctx: &egui::Context, modifiers: egui::Modifiers) {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ui| app.render(ui),
            );
            output.textures_delta.clear();
        }
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let png = tempfile::tempdir().unwrap();
        let png_path = png.path().join("sample.png");
        image::RgbaImage::new(2, 2).save(&png_path).unwrap();
        let mut app = test_shelf(root.path());
        struct SyntheticTrash(PathBuf);
        impl katran::downloads::Trash for SyntheticTrash {
            fn recycle(&self, path: &std::path::Path) -> katran::Result<()> {
                let resolved = path.canonicalize()?;
                assert!(resolved.starts_with(&self.0));
                std::fs::remove_file(resolved)?;
                Ok(())
            }
        }
        app.api.as_mut().unwrap().set_trash(Box::new(SyntheticTrash(
            folder.path().canonicalize().unwrap(),
        )));
        app.api
            .as_mut()
            .unwrap()
            .set_clipboard_files_source(Box::new(NoTransferClipboard));
        app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let id = app.call(Command::IngestBytes {
            name: "sample.png".into(),
            bytes: std::fs::read(png_path).unwrap(),
            source: "synthetic".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::TogglePins { ids: vec![id] });
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "folder");
        assert!(
            !folder.path().join("sample.png").exists(),
            "plain Tab must not transfer"
        );
        release_tab(&mut app, &ctx, egui::Modifiers::NONE);
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(app.state["tab"], "images");
        release_tab(&mut app, &ctx, egui::Modifiers::NONE);
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::SHIFT);
        assert_eq!(app.state["tab"], "folder", "{}", app.message);
        assert!(folder.path().join("sample.png").exists(), "{}", app.message);
        assert_eq!(
            app.call(Command::Get { id })["id"],
            id,
            "pinned source stays"
        );
        let page = app.call(Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        });
        let folder_id = page["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "sample.png")
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        app.call(Command::SetVisible {
            ids: vec![folder_id],
        });
        app.state = app.call(Command::Select {
            id: folder_id,
            mode: SelectionMode::Replace,
        });
        release_tab(&mut app, &ctx, egui::Modifiers::SHIFT);
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::SHIFT);
        assert_eq!(app.state["tab"], "images", "{}", app.message);
        assert!(
            !folder.path().join("sample.png").exists(),
            "Shift+Tab moves the Folder original after import: {}",
            app.message
        );
        assert!(
            app.events
                .iter()
                .any(|v| v["id"].as_i64().is_some_and(|n| n > 0))
        );
    }
    #[test]
    fn shift_tab_without_folder_preserves_selection_and_text_input() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.api
            .as_mut()
            .unwrap()
            .set_clipboard_files_source(Box::new(NoTransferClipboard));
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let id = app.call(Command::IngestBytes {
            name: "test.png".into(),
            bytes: vec![1],
            source: "synthetic".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::SHIFT);
        assert_eq!(app.state["tab"], "images");
        assert_eq!(app.state["selected"], serde_json::json!([id]));
        assert!(!app.message.is_empty(), "missing folder must be reported");
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("shelf-search")));
        let mut released = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::SHIFT,
                }],
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        released.textures_delta.clear();
        app.message.clear();
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::SHIFT);
        assert!(app.message.is_empty(), "text editing must retain Shift+Tab");
        assert_eq!(
            app.call(Command::Interaction)["selected"],
            serde_json::json!([id])
        );
    }
    #[test]
    fn compact_blue_header_removes_title_row_but_keeps_search_and_tabs() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
        out.textures_delta.clear();
        let before = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("test-header-rect")))
            .unwrap();
        app.call(Command::SetTheme {
            theme: katran::theme::ThemePreferences {
                preset: katran::theme::Preset::BlueExperimental,
                ..Default::default()
            },
        });
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
        out.textures_delta.clear();
        let after = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("test-header-rect")))
            .unwrap();
        assert!(after.height() < before.height());
        assert!(ctx.memory(|m| m.focused()).is_none());
    }
    #[test]
    fn sort_modal_owns_tab_transfer_and_paste_keys_before_early_handlers() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.api
            .as_mut()
            .unwrap()
            .set_clipboard_files_source(Box::new(NoTransferClipboard));
        app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        let e = app.call(Command::IngestBytes {
            name: "stay.png".into(),
            bytes: vec![1, 2, 3],
            source: "synthetic".into(),
        });
        let id = e["id"].as_i64().unwrap();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        app.sorting_input.open();
        let tab = app.state["tab"].clone();
        let mut input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: Some(egui::Key::Tab),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        assert!(app.pending_tab_action.is_none());
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::SHIFT);
        frame(&mut app, &ctx, egui::Key::V, egui::Modifiers::CTRL);
        frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::CTRL);
        assert_eq!(app.state["tab"], tab);
        assert!(app.sorting_input.is_open());
        assert!(std::path::Path::new(e["path"].as_str().unwrap()).exists());
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    }
    #[test]
    fn comma_and_tab_in_one_frame_open_sort_without_switching_context() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let before = app.call(Command::Interaction)["tab"].clone();
        let mut input = egui::RawInput {
            events: [egui::Key::Comma, egui::Key::Tab]
                .into_iter()
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect(),
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
        assert_eq!(app.state["tab"], before);
        assert!(app.sorting_input.is_open());
        assert!(app.pending_tab_action.is_none());
    }
    #[test]
    fn ctrl_tab_copies_repeatedly_without_losing_selection_or_reading_clipboard() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        app.api
            .as_mut()
            .unwrap()
            .set_clipboard_files_source(Box::new(NoTransferClipboard));
        app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let e = app.call(Command::IngestBytes {
            name: "copy.png".into(),
            bytes: vec![1, 2, 3],
            source: "synthetic".into(),
        });
        let id = e["id"].as_i64().unwrap();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
        output.textures_delta.clear();
        for _ in 0..3 {
            let key = |pressed| egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: Some(egui::Key::Tab),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            };
            let mut input = egui::RawInput {
                events: vec![key(true)],
                ..Default::default()
            };
            eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
            let mut output = ctx.run_ui(input, |ui| app.render(ui));
            output.textures_delta.clear();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![key(false)],
                    ..Default::default()
                },
                |ui| app.render(ui),
            );
            output.textures_delta.clear();
            assert_eq!(app.state["tab"], "images");
            assert_eq!(app.state["selected"][0], id);
        }
        assert_eq!(
            std::fs::read_dir(folder.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_file())
                .count(),
            3
        );
        assert!(std::path::Path::new(e["path"].as_str().unwrap()).exists());
    }
    #[test]
    fn sort_notice_obeys_preference_and_expiry_without_focus() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        app.run("sort_arrival_reverse".into(), &ctx);
        assert!(app.sort_notice.is_some());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.sort_notice(ui.ctx()));
        output.textures_delta.clear();
        assert!(ctx.memory(|m| m.focused()).is_none());
        app.sort_notice = Some(("expired".into(), Instant::now() - Duration::from_secs(3)));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.sort_notice(ui.ctx()));
        output.textures_delta.clear();
        assert!(app.sort_notice.is_none());
        let mut settings: katran::settings::Settings =
            serde_json::from_value(app.call(Command::GetSettings)).unwrap();
        settings.sort_notifications = false;
        app.call(Command::SetSettings { settings });
        app.run("sort_arrival".into(), &ctx);
        assert!(app.sort_notice.is_none());
    }
    #[test]
    fn actions_popup_tracks_actual_status_bar_trigger() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        app.actions = true;
        for size in [egui::vec2(940.0, 680.0), egui::vec2(620.0, 480.0)] {
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| app.render(ui),
                );
                output.textures_delta.clear();
            }
            let trigger = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("actions-button-rect")))
                .unwrap();
            let popup = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("test-actions-popup-rect")))
                .unwrap();
            assert!(
                popup.bottom() <= trigger.top() - 4.0,
                "{popup:?} must stay above {trigger:?}"
            );
            assert!(
                (popup.right() - trigger.right()).abs() <= 2.0,
                "right edges: {popup:?} / {trigger:?}"
            );
        }
    }
    #[test]
    fn folder_header_keeps_category_geometry_with_long_status_and_recovery() {
        fn panel_sizes(app: &mut Shelf, ctx: &egui::Context, size: egui::Vec2) -> (f32, f32) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| app.render(ui));
            output.textures_delta.clear();
            let header =
                ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new("test-header-rect")));
            let footer =
                ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new("test-footer-rect")));
            (
                header.expect("header layout recorded").height(),
                footer.expect("footer layout recorded").height(),
            )
        }
        let root = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let folder = parent.path().join("long-folder-name-".repeat(5));
        std::fs::create_dir(&folder).unwrap();
        let mut app = test_shelf(root.path());
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let normal = panel_sizes(&mut app, &ctx, egui::vec2(940.0, 680.0));
        app.call(Command::ChooseFolder { path: folder });
        app.message = "ошибка с длинным путём ".repeat(50);
        app.workspace.recovery = Some((0..30).map(|i| serde_json::json!({"recovery_path":format!("long-recovery-path-{i}"),"error":{"message":"synthetic"}})).collect());
        let folder_sizes = panel_sizes(&mut app, &ctx, egui::vec2(940.0, 680.0));
        assert_eq!(
            normal, folder_sizes,
            "Folder details must not change panel heights"
        );
        assert!(
            (30.0..=40.0).contains(&folder_sizes.1),
            "footer must stay compact: {:?}",
            folder_sizes
        );
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        app.call(Command::SelectTabPosition { position: 1 });
        assert_eq!(
            normal,
            panel_sizes(&mut app, &ctx, egui::vec2(940.0, 680.0)),
            "Pinned state must not change panel heights"
        );
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
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        // A stale visual selection must never start a native clipboard write.
        app.state = serde_json::json!({"selected": [999], "cursor": 999});
        app.run("copy".into(), &egui::Context::default());
        assert!(app.api.is_some());
        assert!(app.message.contains("устарел"));
        app.message.clear();
        for n in 0..4 {
            app.call(Command::IngestBytes {
                name: format!("image{n}.png"),
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
        app.job = Some(UiJob::UnavailableTransport(pending_rx));
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
            Command::CopySmart { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, selected_id);
                assert_eq!(items[0].revision, selected_revision);
            }
            _ => panic!("queued copy must use the checked command"),
        }
        app.state = app.call(Command::Interaction);
        // Domain jobs retain the interactive dispatcher while work is pending.
        assert!(app.start_job(Command::Status, false, egui::Context::default()));
        assert!(app.api.is_some());
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.job.is_some() && Instant::now() < deadline {
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
        frame(&mut app, &ctx, egui::Key::Space, egui::Modifiers::NONE);
        assert_eq!(app.state["preview"], 2);
        app.run("preview".into(), &ctx);
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
            name: "external.png".into(),
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
            id: "preview".into(),
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
        let mut released = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Comma,
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

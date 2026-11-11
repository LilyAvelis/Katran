//! Lazy, bounded read-only table preview. Rendering touches only visible cells.
use super::*;
use crate::theme_ui::{Role, color as theme_color};
use katran::table_fragments::Table;
use std::{
    ops::Range,
    sync::{Arc, Condvar, Mutex},
};

const ROW_HEIGHT: f32 = 28.0;
const COL_WIDTH: f32 = 180.0;
const ROW_HEADER: f32 = 48.0;
const DISPLAY_CELL_CHARS: usize = 256;
const TOOLTIP_CHARS: usize = 4096;
const SOURCE_CHARS: usize = 65_536;
type Key = (i64, String);
struct Queue {
    next: Option<(Key, egui::Context)>,
    closed: bool,
}
struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: mpsc::Receiver<(Key, Result<Content, String>)>,
}
struct Content {
    table: Table,
    csv: String,
    truncated: bool,
}
pub(super) struct TablePreview {
    root: PathBuf,
    enabled: bool,
    worker: Option<Worker>,
    wanted: Option<Key>,
    content: Option<Result<Content, String>>,
    grid: bool,
}
impl TablePreview {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            enabled: false,
            worker: None,
            wanted: None,
            content: None,
            grid: true,
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
        let worker = queue.clone();
        let (tx, rx) = mpsc::channel();
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
                let reply = match &mut api {
                    Ok(api) => api.execute(Request {
                        api_version: 1,
                        request_id: "table-preview".into(),
                        command: Command::GetTable { id: key.0 },
                    }),
                    Err(error) => serde_json::json!({"ok":false,"error":error}),
                };
                let content = if reply["ok"] == true && reply["result"]["revision"] == key.1 {
                    (|| -> Result<Content, String> {
                        let table: Table = serde_json::from_value(reply["result"]["table"].clone())
                            .map_err(|e| e.to_string())?;
                        table.validate()?;
                        let raw = reply["result"]["csv_text"]
                            .as_str()
                            .ok_or("CSV preview missing")?;
                        let full = raw.strip_prefix('\u{feff}').unwrap_or(raw);
                        let mut chars = full.chars();
                        let csv: String = chars.by_ref().take(SOURCE_CHARS).collect();
                        Ok(Content {
                            table,
                            csv,
                            truncated: chars.next().is_some(),
                        })
                    })()
                } else {
                    Err(reply["error"]["message"]
                        .as_str()
                        .unwrap_or("Таблица изменилась или недоступна")
                        .into())
                };
                if tx.send((key, content)).is_err() {
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
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.stop();
        }
    }
    pub fn handle_view_shortcuts(&mut self, ctx: &egui::Context, active: bool) -> bool {
        if !self.enabled || !active || ctx.egui_wants_keyboard_input() {
            return false;
        }
        let key = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: key @ (egui::Key::ArrowLeft | egui::Key::ArrowRight),
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd => {
                    Some(*key)
                }
                _ => None,
            })
        });
        let Some(key) = key else {
            return false;
        };
        self.grid = key == egui::Key::ArrowLeft;
        ctx.input_mut(|input| { input.events.retain(|event| !matches!(event,egui::Event::Key {key:event_key,pressed:true,modifiers,..} if *event_key==key && modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd)); input.consume_key(egui::Modifiers::CTRL,key); });
        true
    }
    pub fn show(&mut self, ui: &mut egui::Ui, event: &Value, api: &mut Service) {
        if !self.enabled {
            return;
        }
        let (Some(id), Some(revision)) = (event["id"].as_i64(), event["revision"].as_str()) else {
            return;
        };
        let key = (id, revision.to_owned());
        self.start();
        if self.wanted.as_ref() != Some(&key) {
            self.wanted = Some(key.clone());
            self.content = None;
            self.grid = true;
            let (lock, wake) = &*self.worker.as_ref().unwrap().queue;
            lock.lock().unwrap().next = Some((key, ui.ctx().clone()));
            wake.notify_one();
        }
        while let Ok((key, content)) = self.worker.as_ref().unwrap().rx.try_recv() {
            if self.wanted.as_ref() == Some(&key) {
                self.content = Some(content);
            }
        }
        ui.heading("Таблица");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.grid, true, "Таблица");
            ui.selectable_value(&mut self.grid, false, "CSV");
        });
        ui.separator();
        match &self.content {
            None => {}
            Some(Err(message)) => {
                ui.label(message);
            }
            Some(Ok(content)) if self.grid => {
                show_grid(ui, &content.table, api);
            }
            Some(Ok(content)) => {
                egui::ScrollArea::both()
                    .max_height(ui.available_height().max(40.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&content.csv).monospace().size(14.0));
                        if content.truncated {
                            ui.weak("Показана только часть CSV; таблица сохранена полностью.");
                        }
                    });
            }
        }
    }
}
impl Drop for TablePreview {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Visible half-open index interval for a uniform axis, clipped to its size.
fn visible_range(min: f32, max: f32, offset: f32, step: f32, count: usize) -> Range<usize> {
    let first = ((min - offset) / step).floor().max(0.0) as usize;
    let end = ((max - offset) / step).ceil().max(0.0) as usize;
    first.min(count)..end.min(count).max(first.min(count))
}
fn column_label(mut index: usize) -> String {
    let mut out = String::new();
    index += 1;
    while index > 0 {
        index -= 1;
        out.push((b'A' + (index % 26) as u8) as char);
        index /= 26;
    }
    out.chars().rev().collect()
}
fn compact(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let mut text: String = chars
        .by_ref()
        .take(limit)
        .map(|c| match c {
            '\n' => '↵',
            '\r' => '␍',
            '\t' => '⇥',
            _ => c,
        })
        .collect();
    if chars.next().is_some() {
        text.push('…');
    }
    text
}
fn clipped_text(ui: &egui::Ui, rect: egui::Rect, value: &str, color: Color32) {
    clipped_text_scaled(ui, rect, value, color, 1.0);
}
fn clipped_text_scaled(ui: &egui::Ui, rect: egui::Rect, value: &str, color: Color32, factor: f32) {
    let mut job = egui::text::LayoutJob::simple_singleline(
        value.to_owned(),
        egui::FontId::proportional(13.0 * factor),
        color,
    );
    let padding = (6.0 * factor).max(1.0);
    job.wrap.max_width = (rect.width() - 2.0 * padding).max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    ui.painter()
        .with_clip_rect(ui.clip_rect().intersect(rect))
        .galley(
            egui::pos2(
                rect.left() + padding,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
}
fn table_call(api: &mut Service, command: Command) -> Result<Value, String> {
    let response = api.execute(Request {
        api_version: 1,
        request_id: "table-view".into(),
        command,
    });
    if response["ok"] == true {
        Ok(response["result"].clone())
    } else {
        Err(response["error"]["message"]
            .as_str()
            .unwrap_or("Ошибка просмотра таблицы")
            .into())
    }
}
fn show_grid(ui: &mut egui::Ui, table: &Table, api: &mut Service) {
    use katran::preview_view::{Point, Size};
    let content = Size {
        width: ROW_HEADER + table.rows[0].len() as f32 * COL_WIDTH,
        height: (table.rows.len() + 1) as f32 * ROW_HEIGHT,
    };
    let factor = table_call(api, Command::Interaction)
        .ok()
        .and_then(|value| value["table_view"]["factor"].as_f64())
        .unwrap_or(1.0) as f32;
    let mut intent = 0i8;
    ui.horizontal(|ui| {
        if ui.button("−").on_hover_text("Уменьшить").clicked() {
            intent = -1;
        }
        let percent = factor * 100.0;
        let label = if percent < 10.0 {
            format!("{percent:.1}%")
        } else {
            format!("{percent:.0}%")
        };
        if ui.button(label).on_hover_text("Сбросить масштаб").clicked() {
            intent = 2;
        }
        if ui.button("+").on_hover_text("Увеличить").clicked() {
            intent = 1;
        }
        if ui.button("Вписать").clicked() {
            intent = 3;
        }
    });
    let (canvas, response) = ui.allocate_exact_size(
        egui::vec2(
            ui.available_width().max(1.0),
            ui.available_height().max(40.0),
        ),
        egui::Sense::click_and_drag(),
    );
    let viewport = Size {
        width: canvas.width(),
        height: canvas.height(),
    };
    let center = Point {
        x: viewport.width / 2.0,
        y: viewport.height / 2.0,
    };
    let command = match intent {
        -1 => Some(Command::ZoomTableView {
            multiplier: 1.0 / 1.25,
            anchor: center,
            viewport,
            content,
        }),
        1 => Some(Command::ZoomTableView {
            multiplier: 1.25,
            anchor: center,
            viewport,
            content,
        }),
        2 => Some(Command::ResetTableView),
        3 => Some(Command::FitTableView { viewport, content }),
        _ => None,
    };
    if let Some(command) = command
        && let Err(message) = table_call(api, command)
    {
        ui.label(message);
        return;
    }
    if response.hovered() {
        let (wheel, zoom, cursor) = ui.input(|input| {
            (
                input.smooth_scroll_delta().y,
                input.zoom_delta(),
                input.pointer.hover_pos(),
            )
        });
        let action = if (zoom - 1.0).abs() > 0.001 {
            Some((zoom, center))
        } else if wheel.abs() > 0.01 {
            cursor.map(|point| {
                (
                    (wheel * 0.003).exp(),
                    Point {
                        x: point.x - canvas.left(),
                        y: point.y - canvas.top(),
                    },
                )
            })
        } else {
            None
        };
        if let Some((multiplier, anchor)) = action
            && let Err(message) = table_call(
                api,
                Command::ZoomTableView {
                    multiplier,
                    anchor,
                    viewport,
                    content,
                },
            )
        {
            ui.label(message);
            return;
        }
    }
    if response.dragged_by(egui::PointerButton::Primary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let delta = ui.input(|input| input.pointer.delta());
        if let Err(message) = table_call(
            api,
            Command::PanTableView {
                delta: Point {
                    x: delta.x,
                    y: delta.y,
                },
                viewport,
                content,
            },
        ) {
            ui.label(message);
            return;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let geometry = match table_call(api, Command::TableViewGeometry { viewport, content }) {
        Ok(value) => value,
        Err(message) => {
            ui.label(message);
            return;
        }
    };
    let factor = geometry["factor"].as_f64().unwrap_or(1.0) as f32;
    let scroll = egui::vec2(
        geometry["scroll"]["x"].as_f64().unwrap_or(0.0) as f32,
        geometry["scroll"]["y"].as_f64().unwrap_or(0.0) as f32,
    );
    paint_grid(ui, canvas, table, factor, scroll);
}
fn paint_grid(ui: &mut egui::Ui, canvas: egui::Rect, table: &Table, factor: f32, scroll: Vec2) {
    let rows = table.rows.len();
    let cols = table.rows.first().map_or(0, Vec::len);
    let row = ROW_HEIGHT * factor;
    let col = COL_WIDTH * factor;
    let row_header = ROW_HEADER * factor;
    let origin = canvas.min - scroll;
    let range_y = visible_range(scroll.y, scroll.y + canvas.height(), row, row, rows);
    let range_x = visible_range(scroll.x, scroll.x + canvas.width(), row_header, col, cols);
    let painter = ui
        .painter()
        .with_clip_rect(ui.clip_rect().intersect(canvas));
    painter.rect_filled(
        canvas,
        0.0,
        theme_color(ui.ctx(), Role::Surface, Color32::from_gray(50)),
    );
    if row < 6.0 || 13.0 * factor < 5.0 {
        let ystep = (6.0 / row).ceil().max(1.0) as usize;
        let xstep = (6.0 / col).ceil().max(1.0) as usize;
        let color = egui::Stroke::new(
            0.5,
            theme_color(ui.ctx(), Role::Border, Color32::from_gray(85)),
        );
        let first_y = range_y.start.div_ceil(ystep) * ystep;
        let first_x = range_x.start.div_ceil(xstep) * xstep;
        for y in (first_y..range_y.end).step_by(ystep) {
            let py = origin.y + row + y as f32 * row;
            painter.line_segment(
                [
                    egui::pos2(canvas.left(), py),
                    egui::pos2(canvas.right(), py),
                ],
                color,
            );
        }
        for x in (first_x..range_x.end).step_by(xstep) {
            let px = origin.x + row_header + x as f32 * col;
            painter.line_segment(
                [
                    egui::pos2(px, canvas.top()),
                    egui::pos2(px, canvas.bottom()),
                ],
                color,
            );
        }
        return;
    }
    let mut cell_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(canvas)
            .sense(egui::Sense::hover()),
    );
    cell_ui.set_clip_rect(ui.clip_rect().intersect(canvas));
    for y in range_y.clone() {
        for x in range_x.clone() {
            let cell = egui::Rect::from_min_size(
                origin + egui::vec2(row_header + x as f32 * col, row + y as f32 * row),
                egui::vec2(col, row),
            );
            painter.rect_filled(
                cell,
                0.0,
                theme_color(
                    ui.ctx(),
                    if y % 2 == 0 {
                        Role::Surface
                    } else {
                        Role::Raised
                    },
                    Color32::from_gray(if y % 2 == 0 { 50 } else { 54 }),
                ),
            );
            painter.rect_stroke(
                cell,
                0.0,
                egui::Stroke::new(
                    0.5,
                    theme_color(ui.ctx(), Role::Border, Color32::from_gray(85)),
                ),
                egui::StrokeKind::Inside,
            );
            let value = &table.rows[y][x];
            clipped_text_scaled(
                &cell_ui,
                cell,
                &compact(value, DISPLAY_CELL_CHARS),
                theme_color(ui.ctx(), Role::Text, Color32::from_gray(230)),
                factor,
            );
            cell_ui
                .interact(
                    cell,
                    ui.id().with(("table-cell", y, x)),
                    egui::Sense::hover(),
                )
                .on_hover_text(compact(value, TOOLTIP_CHARS));
        }
    }
    let header_y = canvas.top();
    let header_x = canvas.left();
    let mut header_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(canvas)
            .sense(egui::Sense::hover()),
    );
    header_ui.set_clip_rect(ui.clip_rect().intersect(canvas));
    for x in range_x {
        let head = egui::Rect::from_min_size(
            egui::pos2(origin.x + row_header + x as f32 * col, header_y),
            egui::vec2(col, row),
        );
        painter.rect_filled(
            head,
            0.0,
            theme_color(ui.ctx(), Role::Raised, Color32::from_gray(76)),
        );
        clipped_text_scaled(
            &header_ui,
            head,
            &column_label(x),
            theme_color(ui.ctx(), Role::Text, Color32::WHITE),
            factor,
        );
    }
    for y in range_y {
        let head = egui::Rect::from_min_size(
            egui::pos2(header_x, origin.y + row + y as f32 * row),
            egui::vec2(row_header, row),
        );
        painter.rect_filled(
            head,
            0.0,
            theme_color(ui.ctx(), Role::Raised, Color32::from_gray(76)),
        );
        clipped_text_scaled(
            &header_ui,
            head,
            &(y + 1).to_string(),
            theme_color(ui.ctx(), Role::Text, Color32::WHITE),
            factor,
        );
    }
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(header_x, header_y), egui::vec2(row_header, row)),
        0.0,
        theme_color(ui.ctx(), Role::Raised, Color32::from_gray(76)),
    );
}
pub(super) fn paint_card(ui: &egui::Ui, event: &Value, rect: egui::Rect) {
    ui.painter().rect_filled(
        rect,
        4.0,
        theme_color(ui.ctx(), Role::Surface, Color32::from_gray(58)),
    );
    let preview = event["text_preview"].as_str().unwrap_or("");
    // Preview is bounded metadata; trim incomplete final line before parsing.
    let candidate = preview
        .rsplit_once('\n')
        .map_or(preview, |(head, _)| head.trim_end_matches('\r'));
    if let Ok(table) = katran::table_fragments::parse_tsv(candidate, true) {
        let rows = table.rows.len().min(3);
        let cols = table.rows.first().map_or(0, Vec::len).min(4);
        if rows > 0 && cols > 0 {
            let width = rect.width() / cols as f32;
            let height = (rect.height() / rows as f32).min(32.0);
            for y in 0..rows {
                for x in 0..cols {
                    let cell = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x as f32 * width, y as f32 * height),
                        egui::vec2(width, height),
                    );
                    ui.painter().rect_stroke(
                        cell,
                        0.0,
                        egui::Stroke::new(
                            0.5,
                            theme_color(ui.ctx(), Role::Border, Color32::from_gray(95)),
                        ),
                        egui::StrokeKind::Inside,
                    );
                    clipped_text(
                        ui,
                        cell,
                        &compact(&table.rows[y][x], 64),
                        theme_color(ui.ctx(), Role::Text, Color32::from_gray(220)),
                    );
                }
            }
            return;
        }
    }
    clipped_text(
        ui,
        rect,
        &compact(preview, 120),
        theme_color(ui.ctx(), Role::Text, Color32::from_gray(220)),
    );
}
pub(super) fn card_title(_event: &Value) -> String {
    "Таблица".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_and_table_cards_use_theme_surface_without_changing_standard() {
        let ctx = egui::Context::default();
        crate::ui_style::install(&ctx);
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(180.0, 80.0));
        let event = serde_json::json!({"text_preview":"one\ttwo\nthree\tfour"});
        let paint = |ctx: &egui::Context, table: bool| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                if table {
                    paint_card(ui, &event, rect);
                } else {
                    crate::text_preview_ui::paint_card(ui, &event, rect);
                }
            });
            let fills: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(r) if r.rect == rect => Some(r.fill),
                    _ => None,
                })
                .collect();
            output.textures_delta.clear();
            fills
        };
        for table in [false, true] {
            assert!(paint(&ctx, table).contains(&Color32::from_gray(58)));
        }
        crate::theme_ui::apply(
            &ctx,
            &katran::theme::ThemePreferences {
                preset: katran::theme::Preset::BlueExperimental,
                accent: None,
                decoration: false,
                ..Default::default()
            },
        );
        let expected = crate::theme_ui::color(&ctx, Role::Surface, Color32::from_gray(58));
        assert_ne!(expected, Color32::from_gray(58));
        for table in [false, true] {
            assert!(paint(&ctx, table).contains(&expected));
        }
    }
    #[test]
    fn viewport_limits_both_axes_and_labels_are_correct() {
        assert_eq!(
            visible_range(0.0, 280.0, ROW_HEIGHT, ROW_HEIGHT, 10_000),
            0..9
        );
        assert_eq!(
            visible_range(
                ROW_HEIGHT * 5000.0,
                ROW_HEIGHT * 5010.0,
                ROW_HEIGHT,
                ROW_HEIGHT,
                10_000
            ),
            4999..5009
        );
        assert_eq!(
            visible_range(
                ROW_HEADER + COL_WIDTH * 200.0,
                ROW_HEADER + COL_WIDTH * 202.0,
                ROW_HEADER,
                COL_WIDTH,
                256
            ),
            200..202
        );
        assert_eq!(column_label(0), "A");
        assert_eq!(column_label(25), "Z");
        assert_eq!(column_label(26), "AA");
        assert_eq!(column_label(255), "IV");
        assert_eq!(compact("a\nb\tc", 256), "a↵b⇥c");
    }
    #[test]
    fn lazy_table_preview_loads_with_text_disabled_and_stops() {
        let root = tempfile::tempdir().unwrap();
        let mut service = Service::open(root.path(), 100_000).unwrap();
        let mut call = |command| {
            service.execute(Request {
                api_version: 1,
                request_id: "table-preview-test".into(),
                command,
            })
        };
        let result = call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Tables,
            enabled: true,
        });
        assert_eq!(result["ok"], true);
        let event = call(Command::IngestTable {
            rows: vec![
                vec!["🐄".into(), "".into()],
                vec!["two".into(), "three".into()],
            ],
            source: "test".into(),
        })["result"]
            .clone();
        assert_eq!(call(Command::GetSettings)["result"]["text_enabled"], false);
        let mut preview = TablePreview::new(root.path().into());
        assert!(preview.worker.is_none());
        preview.set_enabled(true);
        assert!(preview.worker.is_none());
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        while preview.content.is_none() && Instant::now() < deadline {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                preview.show(ui, &event, &mut service)
            });
            out.textures_delta.clear();
            std::thread::yield_now();
        }
        let content = preview.content.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(content.table.rows[0][0], "🐄");
        assert!(!content.csv.starts_with('\u{feff}'));
        let queue = preview.worker.as_ref().unwrap().queue.clone();
        preview.set_enabled(false);
        assert!(preview.worker.is_none());
        assert!(preview.content.is_none());
        assert!(queue.0.lock().unwrap().closed);
    }
    #[test]
    fn ctrl_arrows_switch_and_are_removed_before_global_navigation() {
        let root = tempfile::tempdir().unwrap();
        let mut preview = TablePreview::new(root.path().into());
        preview.set_enabled(true);
        let ctx = egui::Context::default();
        let raw = |key| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        let mut handled = false;
        let mut remaining = true;
        let mut out = ctx.run_ui(raw(egui::Key::ArrowRight), |ui| {
            handled = preview.handle_view_shortcuts(ui.ctx(), true);
            remaining = ui.ctx().input(|i| {
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::ArrowRight,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
        });
        out.textures_delta.clear();
        assert!(handled);
        assert!(!remaining);
        assert!(!preview.grid);
        let mut out = ctx.run_ui(raw(egui::Key::ArrowLeft), |ui| {
            assert!(preview.handle_view_shortcuts(ui.ctx(), true));
        });
        out.textures_delta.clear();
        assert!(preview.grid);
        let mut out = ctx.run_ui(raw(egui::Key::ArrowRight), |ui| {
            assert!(!preview.handle_view_shortcuts(ui.ctx(), false));
        });
        out.textures_delta.clear();
        assert!(preview.grid);
    }
    #[test]
    fn large_grid_paints_only_visible_cells() {
        let table = Table {
            rows: vec![vec!["x".into(); 100]; 1000],
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                ..Default::default()
            },
            |ui| {
                let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
                paint_grid(ui, canvas, &table, 1.0, Vec2::ZERO);
            },
        );
        assert!(
            output.shapes.len() < 1000,
            "unexpected full-grid paint: {}",
            output.shapes.len()
        );
        output.textures_delta.clear();
    }
    #[test]
    fn ctrl_shortcuts_respect_text_edit_focus() {
        let root = tempfile::tempdir().unwrap();
        let mut preview = TablePreview::new(root.path().into());
        preview.set_enabled(true);
        let ctx = egui::Context::default();
        let mut input = String::new();
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
            |ui| {
                ui.add(egui::TextEdit::singleline(&mut input))
                    .request_focus();
                assert!(!preview.handle_view_shortcuts(ui.ctx(), true));
            },
        );
        output.textures_delta.clear();
        assert!(preview.grid);
    }
    #[test]
    fn overview_paints_bounded_shapes_for_full_100000_cell_table() {
        let table = Table {
            rows: vec![vec!["x".into(); 10]; 10_000],
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(940.0, 680.0),
                )),
                ..Default::default()
            },
            |ui| {
                let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(940.0, 600.0));
                paint_grid(ui, canvas, &table, 0.002, Vec2::ZERO);
            },
        );
        assert!(
            output.shapes.len() < 1000,
            "overview drew {} shapes",
            output.shapes.len()
        );
        output.textures_delta.clear();
    }
    #[test]
    fn wheel_and_drag_send_domain_zoom_and_pan() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100_000).unwrap();
        let mut call = |command| {
            api.execute(Request {
                api_version: 1,
                request_id: "zoom-ui-test".into(),
                command,
            })
        };
        assert_eq!(
            call(Command::SetTabEnabled {
                tab: katran::interaction::ShelfTab::Tables,
                enabled: true
            })["ok"],
            true
        );
        let event = call(Command::IngestTable {
            rows: vec![vec!["x".into(); 20]; 100],
            source: "test".into(),
        })["result"]
            .clone();
        let id = event["id"].as_i64().unwrap();
        call(Command::SetVisible { ids: vec![id] });
        call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        call(Command::OpenCurrent);
        let ctx = egui::Context::default();
        let raw = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            events,
            ..Default::default()
        };
        let table = Table {
            rows: vec![vec!["x".into(); 20]; 100],
        };
        let mut out = ctx.run_ui(
            raw(vec![egui::Event::PointerMoved(egui::pos2(200.0, 120.0))]),
            |ui| show_grid(ui, &table, &mut api),
        );
        out.textures_delta.clear();
        let mut out = ctx.run_ui(
            raw(vec![
                egui::Event::PointerMoved(egui::pos2(200.0, 120.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 100.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]),
            |ui| show_grid(ui, &table, &mut api),
        );
        out.textures_delta.clear();
        let state = table_call(&mut api, Command::Interaction).unwrap();
        assert!(state["table_view"]["factor"].as_f64().unwrap() > 1.0);
        let before = state["table_view"]["scroll"]["x"].as_f64().unwrap();
        let press = egui::Event::PointerButton {
            pos: egui::pos2(200.0, 120.0),
            button: egui::PointerButton::Middle,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        let mut out = ctx.run_ui(raw(vec![press]), |ui| show_grid(ui, &table, &mut api));
        out.textures_delta.clear();
        let mut out = ctx.run_ui(
            raw(vec![egui::Event::PointerMoved(egui::pos2(150.0, 120.0))]),
            |ui| show_grid(ui, &table, &mut api),
        );
        out.textures_delta.clear();
        let state = table_call(&mut api, Command::Interaction).unwrap();
        assert!(state["table_view"]["scroll"]["x"].as_f64().unwrap() > before);
    }
}

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum PreviewDrag {
    Pan,
    Export,
}

fn preview_drag(factor: f64, ctrl: bool) -> PreviewDrag {
    if factor > 1.0 && !ctrl {
        PreviewDrag::Pan
    } else {
        PreviewDrag::Export
    }
}

fn latched_preview_drag(
    previous: Option<PreviewDrag>,
    started: bool,
    factor: f64,
    ctrl: bool,
) -> Option<PreviewDrag> {
    if started {
        Some(preview_drag(factor, ctrl))
    } else {
        previous
    }
}

fn preview_pan_command(
    primary_pan: bool,
    middle_pan: bool,
    delta: Vec2,
    viewport: katran::preview_view::Size,
    fitted: katran::preview_view::Size,
) -> Option<Command> {
    (primary_pan || middle_pan).then_some(Command::PanPreview {
        delta: katran::preview_view::Point {
            x: delta.x,
            y: delta.y,
        },
        viewport,
        fitted,
    })
}

impl Shelf {
    pub(super) fn shelf_view(
        &mut self,
        root: &mut egui::Ui,
        ctx: &egui::Context,
        visible: &[Value],
        gallery: bool,
        columns: usize,
    ) {
        self.images.begin_frame();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(12))
            .show(root, |ui| {
                if let Some(id) = self.state["preview"].as_i64() {
                    if let Some(event) = visible.iter().find(|e| e["id"] == id) {
                        let width = ui.available_width();
                        let height = ui.available_height();
                        let (title, _) = ui.allocate_exact_size(Vec2::new(width, 32.0), egui::Sense::hover());
                        let name = event["name"].as_str().unwrap_or("Изображение");
                        let mut job = egui::text::LayoutJob::simple_singleline(
                            name.into(), egui::FontId::proportional(16.0), Color32::from_gray(235));
                        job.wrap.max_width = title.width();
                        job.wrap.max_rows = 1;
                        job.wrap.overflow_character = Some('…');
                        let galley = ui.fonts_mut(|f| f.layout_job(job));
                        let position = title.center() - galley.size() / 2.0;
                        ui.painter_at(title).galley(position, galley, Color32::from_gray(235));
                        let (canvas, response) = ui.allocate_exact_size(
                            Vec2::new(width, (height - 76.0).max(32.0)), egui::Sense::click_and_drag());
                        let mut image_rect = None;
                        let drag_id = response.id.with(("preview-drag", id));
                        let mut export_started = response.drag_started_by(egui::PointerButton::Primary);
                        if self.api.is_some() && let Some(size) = self.images.fitted_preview_size(event, canvas) {
                            use katran::preview_view::{Point, Size};
                            let viewport = Size { width: canvas.width(), height: canvas.height() };
                            let fitted = Size { width: size.x, height: size.y };
                            if response.hovered() {
                                let (wheel, center_zoom, cursor) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta(), i.pointer.hover_pos()));
                                if wheel.abs() > 0.01 && let Some(cursor) = cursor {
                                    self.state = self.call(Command::ZoomPreview {
                                        multiplier: (wheel * 0.003).exp(),
                                        anchor: Point { x: cursor.x - canvas.min.x, y: cursor.y - canvas.min.y }, viewport, fitted,
                                    });
                                }
                                if (center_zoom - 1.0).abs() > 0.001 {
                                    self.state = self.call(Command::ZoomPreview {
                                        multiplier: center_zoom,
                                        anchor: Point { x: viewport.width / 2.0, y: viewport.height / 2.0 }, viewport, fitted,
                                    });
                                }
                            }
                            let ctrl = ctx.input(|i| i.modifiers.ctrl);
                            let factor = self.state["preview_view"]["factor"].as_f64().unwrap_or(1.0);
                            // Latch at drag start: changing Ctrl or zoom during a pan
                            // must never turn it into a native OLE export.
                            let started = response.drag_started_by(egui::PointerButton::Primary);
                            let intent = ctx.data_mut(|data| {
                                let intent = latched_preview_drag(data.get_temp::<PreviewDrag>(drag_id), started, factor, ctrl);
                                if let Some(intent) = intent { data.insert_temp(drag_id, intent); }
                                intent
                            });
                            export_started = started && intent == Some(PreviewDrag::Export);
                            let primary_pan = response.dragged_by(egui::PointerButton::Primary)
                                && intent == Some(PreviewDrag::Pan);
                            let middle_pan = response.dragged_by(egui::PointerButton::Middle);
                            if let Some(command) = preview_pan_command(primary_pan, middle_pan, ctx.input(|i| i.pointer.delta()), viewport, fitted) {
                                self.state = self.call(command);
                            }
                            if primary_pan || middle_pan {
                                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            } else if response.hovered() && preview_drag(factor, ctrl) == PreviewDrag::Pan {
                                ctx.set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            let geometry = self.call(Command::PreviewGeometry { viewport, fitted });
                            if let (Some(left), Some(top), Some(width), Some(height)) = (
                                geometry["left"].as_f64(), geometry["top"].as_f64(), geometry["width"].as_f64(), geometry["height"].as_f64()) {
                                image_rect = Some(egui::Rect::from_min_size(canvas.min + Vec2::new(left as f32, top as f32), Vec2::new(width as f32, height as f32)));
                            }
                        }
                        if !ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                            ctx.data_mut(|data| data.remove::<PreviewDrag>(drag_id));
                        }
                        self.images.paint_preview(ui, event, canvas, image_rect);
                        if export_started {
                            self.start_drag(vec![id], ctx);
                        }
                        ui.vertical_centered(|ui| {
                            let zoomed = self.state["preview_view"]["factor"].as_f64().is_some_and(|factor| factor > 1.0);
                            let hint = if zoomed {
                                "ЛКМ — сдвиг · Ctrl+тащить — экспорт · Space — назад · ← → — листать"
                            } else {
                                "Тащить — экспорт · колесо — увеличить · Space — назад · ← → — листать"
                            };
                            ui.weak(hint).on_hover_text("Колесо приближает к курсору, Ctrl+колесо — к центру. Увеличенное изображение можно двигать левой или средней кнопкой мыши.");
                        });
                        if let Some(index) = visible.iter().position(|e| e["id"] == id) {
                            let neighbors = [index.checked_sub(1), index.checked_add(1).filter(|n| *n < visible.len())];
                            for neighbor in neighbors.into_iter().flatten() {
                                self.images.prefetch(ctx, &visible[neighbor], 1400);
                            }
                        }
                    }
                } else {
                    self.images.clear_preview_geometry();
                    if visible.is_empty() {
                        ui.label(
                            "Полка пуста. Ctrl K → Включить захват, затем скопируйте изображение.",
                        );
                    }
                    let gap = gallery_card::GAP;
                    ui.spacing_mut().item_spacing = Vec2::splat(gap);
                    let cell_width = if gallery {
                        ((ui.available_width() - 16.0 - gap * (columns - 1) as f32)
                            / columns as f32)
                            .max(80.0)
                    } else {
                        (ui.available_width() - 16.0).max(80.0)
                    };
                    let height = if gallery { cell_width + 54.0 } else { 150.0 };
                    let scroll_id = ui.make_persistent_id("shelf-scroll");
                    let mut area = egui::ScrollArea::vertical().id_salt("shelf-scroll");
                    if self.scroll
                        && let Some(index) =
                            visible.iter().position(|e| e["id"] == self.state["cursor"])
                    {
                        let offset = egui::scroll_area::State::load(ctx, scroll_id)
                            .map_or(0.0, |state| state.offset.y);
                        let pitch = height + ui.spacing().item_spacing.y;
                        area = area.vertical_scroll_offset(ensure_visible_offset(
                            offset,
                            ui.available_height(),
                            (index / columns) as f32 * pitch,
                            height,
                        ));
                    }
                    self.scroll = false;
                    area.show_rows(ui, height, visible.len().div_ceil(columns), |ui, range| {
                        for row in range {
                            ui.horizontal(|ui| {
                                for event in visible
                                    .iter()
                                    .take(((row + 1) * columns).min(visible.len()))
                                    .skip(row * columns)
                                {
                                    let id = event["id"].as_i64().unwrap();
                                    let selected = self.state["selected"]
                                        .as_array()
                                        .is_some_and(|s| s.contains(&Value::from(id)));
                                    let cursor = self.state["cursor"] == id;
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(cell_width, height),
                                        egui::Sense::click_and_drag(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        8.0,
                                        Color32::from_gray(if selected { 74 } else { 46 }),
                                    );
                                    if cursor {
                                        ui.painter().rect_stroke(
                                            rect,
                                            8.0,
                                            egui::Stroke::new(1.5, Color32::from_gray(180)),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    if gallery {
                                        let zones = gallery_card::geometry(rect);
                                        self.images.paint_at(ui, event, zones.image, 512);
                                        gallery_card::text(
                                            ui,
                                            zones.name,
                                            event["name"].as_str().unwrap_or("Файл"),
                                            14.0,
                                            Color32::from_gray(235),
                                        );
                                        let origin: katran::origin::Origin =
                                            serde_json::from_value(event["origin"].clone())
                                                .unwrap_or_default();
                                        let label = origin
                                            .label()
                                            .unwrap_or_else(|| "Источник не определён".into());
                                        ui.painter_at(zones.origin).rect_filled(
                                            zones.origin,
                                            4.0,
                                            Color32::from_gray(55),
                                        );
                                        gallery_card::text(
                                            ui,
                                            zones.origin.shrink2(Vec2::new(5.0, 0.0)),
                                            &label,
                                            12.0,
                                            Color32::from_rgb(183, 200, 203),
                                        );
                                    } else {
                                        let image = egui::Rect::from_min_size(
                                            rect.min + Vec2::splat(10.0),
                                            Vec2::new(176.0, 120.0),
                                        );
                                        self.images.paint_at(ui, event, image, 320);
                                        let left = image.right() + 16.0;
                                        let width = (rect.right() - left - 10.0).max(1.0);
                                        let origin: katran::origin::Origin =
                                            serde_json::from_value(event["origin"].clone())
                                                .unwrap_or_default();
                                        let lines = [
                                            event["name"].as_str().unwrap_or("Файл").to_owned(),
                                            event["source"].as_str().unwrap_or("").to_owned(),
                                            origin
                                                .label()
                                                .unwrap_or_else(|| "Источник не определён".into()),
                                            if event["available"] == false {
                                                "Исходник недоступен"
                                            } else if event["storage"] == "cached" {
                                                "Временная копия"
                                            } else {
                                                "Исходный файл"
                                            }
                                            .to_owned(),
                                        ];
                                        for (index, line) in lines.iter().enumerate() {
                                            let r = egui::Rect::from_min_size(
                                                egui::pos2(
                                                    left,
                                                    rect.top() + 12.0 + index as f32 * 28.0,
                                                ),
                                                Vec2::new(width, 24.0),
                                            );
                                            gallery_card::text(
                                                ui,
                                                r,
                                                line,
                                                if index == 0 { 14.0 } else { 12.0 },
                                                Color32::from_gray(if index == 0 {
                                                    235
                                                } else {
                                                    180
                                                }),
                                            );
                                        }
                                    }
                                    let origin: katran::origin::Origin =
                                        serde_json::from_value(event["origin"].clone())
                                            .unwrap_or_default();
                                    let response = response.on_hover_text(format!(
                                        "{}\n{}",
                                        event["name"].as_str().unwrap_or("Файл"),
                                        origin
                                            .label()
                                            .unwrap_or_else(|| "Источник не определён".into())
                                    ));
                                    if response.drag_started() {
                                        if !selected {
                                            self.state = self.call(Command::Select {
                                                id, mode: katran::interaction::SelectionMode::Replace,
                                            });
                                        }
                                        let ids = self.state["selected"].as_array().into_iter()
                                            .flatten().filter_map(Value::as_i64).collect();
                                        self.start_drag(ids, ctx);
                                    }
                                    if response.clicked() || response.double_clicked() {
                                        self.state = self.call(Command::Select {
                                            id,
                                            mode: mode(ctx.input(|i| i.modifiers)),
                                        });
                                        if response.double_clicked() {
                                            self.state = self.call(Command::OpenCurrent);
                                        }
                                    }
                                }
                            });
                        }
                    });
                }
                if self.job.is_some() {
                    ui.label("Выполняется операция…");
                }
                if !self.message.is_empty() {
                    ui.label(&self.message);
                }
            });
        self.images.end_frame(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katran::preview_view::{Point, Size};

    #[test]
    fn preview_primary_drag_reserves_ctrl_for_export() {
        assert_eq!(preview_drag(1.0, false), PreviewDrag::Export);
        assert_eq!(preview_drag(1.0, true), PreviewDrag::Export);
        assert_eq!(preview_drag(1.01, false), PreviewDrag::Pan);
        assert_eq!(preview_drag(16.0, false), PreviewDrag::Pan);
        assert_eq!(preview_drag(16.0, true), PreviewDrag::Export);
    }

    #[test]
    fn drag_intent_stays_latched_when_modifiers_or_zoom_change() {
        let pan = latched_preview_drag(None, true, 2.0, false);
        assert_eq!(pan, Some(PreviewDrag::Pan));
        assert_eq!(latched_preview_drag(pan, false, 1.0, true), pan);
        let export = latched_preview_drag(None, true, 2.0, true);
        assert_eq!(export, Some(PreviewDrag::Export));
        assert_eq!(latched_preview_drag(export, false, 2.0, false), export);
        assert_eq!(latched_preview_drag(export, true, 2.0, false), pan);
    }

    #[test]
    fn grab_and_middle_drag_route_to_the_domain_pan_api() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 1024).unwrap();
        let mut call = |command| {
            let result = api.execute(Request {
                api_version: 1,
                request_id: "grab-test".into(),
                command,
            });
            assert_eq!(result["ok"], true, "{result}");
            result["result"].clone()
        };
        let event = call(Command::IngestBytes {
            name: "synthetic.png".into(),
            bytes: vec![1],
            source: "test".into(),
        });
        call(Command::SetVisible {
            ids: vec![event["id"].as_i64().unwrap()],
        });
        call(Command::TogglePreview);
        let viewport = Size {
            width: 200.0,
            height: 200.0,
        };
        let fitted = viewport;
        call(Command::ZoomPreview {
            multiplier: 2.0,
            anchor: Point { x: 100.0, y: 100.0 },
            viewport,
            fitted,
        });
        for (primary, middle, expected) in [(true, false, 30.0), (false, true, 60.0)] {
            let command =
                preview_pan_command(primary, middle, Vec2::new(30.0, -20.0), viewport, fitted)
                    .unwrap();
            let state = call(command);
            assert_eq!(state["preview_view"]["offset"]["x"], expected);
        }
        // Export and idle input produce no pan command (and no native drag is invoked).
        assert!(preview_pan_command(false, false, Vec2::splat(50.0), viewport, fitted).is_none());
        let state =
            call(preview_pan_command(true, false, Vec2::splat(1000.0), viewport, fitted).unwrap());
        assert_eq!(state["preview_view"]["offset"]["x"], 100.0);
        assert_eq!(state["preview_view"]["offset"]["y"], 100.0);
        assert!(!root.path().join("drag-cache").exists());
    }
}

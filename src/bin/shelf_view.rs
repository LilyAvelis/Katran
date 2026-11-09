use super::*;

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
                            if response.dragged_by(egui::PointerButton::Middle) {
                                let delta = ctx.input(|i| i.pointer.delta());
                                self.state = self.call(Command::PanPreview { delta: Point { x: delta.x, y: delta.y }, viewport, fitted });
                            }
                            let geometry = self.call(Command::PreviewGeometry { viewport, fitted });
                            if let (Some(left), Some(top), Some(width), Some(height)) = (
                                geometry["left"].as_f64(), geometry["top"].as_f64(), geometry["width"].as_f64(), geometry["height"].as_f64()) {
                                image_rect = Some(egui::Rect::from_min_size(canvas.min + Vec2::new(left as f32, top as f32), Vec2::new(width as f32, height as f32)));
                            }
                        }
                        self.images.paint_preview(ui, event, canvas, image_rect);
                        if response.drag_started_by(egui::PointerButton::Primary) {
                            self.start_drag(vec![id], ctx);
                        }
                        ui.vertical_centered(|ui| {
                            ui.weak("Space / Esc — полка · ← → — листать · колесо — зум · Ctrl — центр · средняя — сдвиг");
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

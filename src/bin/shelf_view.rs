use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum PreviewDrag {
    Pan,
    Export,
}

fn preview_drag(factor: f64) -> PreviewDrag {
    if factor > 1.0 {
        PreviewDrag::Pan
    } else {
        PreviewDrag::Export
    }
}

fn latched_preview_drag(
    previous: Option<PreviewDrag>,
    started: bool,
    factor: f64,
) -> Option<PreviewDrag> {
    if started {
        Some(preview_drag(factor))
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct PodsPreviewLayout {
    canvas: Vec2,
    metadata_height: f32,
}

fn pods_preview_layout(width: f32, height: f32) -> PodsPreviewLayout {
    let canvas_height = (height * 0.42).clamp(0.0, 300.0);
    PodsPreviewLayout {
        canvas: Vec2::new(width.max(0.0), canvas_height),
        metadata_height: (height - canvas_height).max(0.0),
    }
}

fn stream_row_height(viewport: f32, rows: u8, gap: f32) -> f32 {
    ((viewport - gap * f32::from(rows.saturating_sub(1))) / f32::from(rows.max(1))).max(72.0)
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
            .frame(
                egui::Frame::new()
                    .fill(theme_ui::color(
                        ctx,
                        theme_ui::Role::Background,
                        Color32::TRANSPARENT,
                    ))
                    .inner_margin(12),
            )
            .show(root, |ui| {
                if let Some(id) = self.state["preview"].as_i64() {
                    if let Some(event) = visible.iter().find(|e| e["id"] == id) {
                        if self.state["tab"] == "pods" && event["kind"] == "text_fragment" {
                            let revision = event["revision"].as_str().unwrap_or("");
                            if self.api.is_some() && self.pods_ui.description_needed(id, revision) {
                                let description = self.call(Command::DescribePod { id });
                                self.pods_ui.set_description(id, revision, description);
                            }
                            let description =
                                self.pods_ui.description().cloned().unwrap_or(Value::Null);
                            let mut full_event = event.clone();
                            if let Some(text) = description["text"].as_str() {
                                full_event["text_preview"] = Value::String(text.to_owned());
                            }
                            let width = ui.available_width();
                            let height = ui.available_height().max(32.0);
                            let layout = pods_preview_layout(width, height);
                            ui.vertical_centered(|ui| {
                                let (canvas, _) =
                                    ui.allocate_exact_size(layout.canvas, egui::Sense::hover());
                                #[cfg(test)]
                                ctx.data_mut(|data| {
                                    data.insert_temp(egui::Id::new("pod-preview-canvas"), canvas)
                                });
                                self.pods_ui.paint(ui, &full_event, canvas);
                                egui::ScrollArea::vertical()
                                    .id_salt("pods-metadata")
                                    .max_height(layout.metadata_height)
                                    .show(ui, |ui| {
                                        ui.set_width(width);
                                        ui.vertical_centered(|ui| {
                                            ui.heading(
                                                description["name"].as_str().unwrap_or("Pods"),
                                            );
                                            if description.is_null() {
                                                ui.label("Метаданные недоступны");
                                            } else {
                                                ui.label(format!(
                                                    "UTF-8: {} байт · UTF-16: {}",
                                                    description["utf8_bytes"],
                                                    description["utf16_units"]
                                                ));
                                                if let Some(points) =
                                                    description["codepoints"].as_array()
                                                {
                                                    for point in points {
                                                        ui.label(format!(
                                                            "{} · {}",
                                                            point["codepoint"]
                                                                .as_str()
                                                                .unwrap_or(""),
                                                            point["name"].as_str().unwrap_or("")
                                                        ));
                                                        ui.weak(
                                                            point["utf8"].as_str().unwrap_or(""),
                                                        );
                                                    }
                                                }
                                                if description["metadata_truncated"] == true {
                                                    ui.weak("Показана часть метаданных");
                                                }
                                            }
                                        });
                                    });
                            });
                        } else if event["kind"] == "table_fragment" {
                            if let Some(api) = self.api.as_mut() {
                                self.table_preview.show(ui, event, api);
                            }
                        } else if matches!(
                            event["kind"].as_str(),
                            Some("text_fragment" | "link_fragment")
                        ) || katran::file_types::is_markdown(std::path::Path::new(
                            event["name"].as_str().unwrap_or(""),
                        )) {
                            self.text_preview.show(ui, event);
                        } else {
                            let width = ui.available_width();
                            let height = ui.available_height();
                            let (title, _) = ui
                                .allocate_exact_size(Vec2::new(width, 32.0), egui::Sense::hover());
                            let name = event["name"].as_str().unwrap_or("Изображение");
                            let mut job = egui::text::LayoutJob::simple_singleline(
                                name.into(),
                                egui::FontId::proportional(16.0),
                                Color32::from_gray(235),
                            );
                            job.wrap.max_width = title.width();
                            job.wrap.max_rows = 1;
                            job.wrap.overflow_character = Some('…');
                            let galley = ui.fonts_mut(|f| f.layout_job(job));
                            let position = title.center() - galley.size() / 2.0;
                            ui.painter_at(title)
                                .galley(position, galley, Color32::from_gray(235));
                            let (canvas, response) = ui.allocate_exact_size(
                                Vec2::new(width, (height - 40.0).max(32.0)),
                                egui::Sense::click_and_drag(),
                            );
                            let mut image_rect = None;
                            let drag_id = response.id.with(("preview-drag", id));
                            // Resolve the gesture even while a background job owns the API
                            // or the preview texture is loading. Neither may enable export.
                            let factor =
                                self.state["preview_view"]["factor"].as_f64().unwrap_or(1.0);
                            let started = response.drag_started_by(egui::PointerButton::Primary);
                            let intent = ctx.data_mut(|data| {
                                let intent = latched_preview_drag(
                                    data.get_temp::<PreviewDrag>(drag_id),
                                    started,
                                    factor,
                                );
                                if let Some(intent) = intent {
                                    data.insert_temp(drag_id, intent);
                                }
                                intent
                            });
                            let mut export_started =
                                started && intent == Some(PreviewDrag::Export) && factor <= 1.0;
                            if self.api.is_some()
                                && let Some(size) = self.images.fitted_preview_size(event, canvas)
                            {
                                use katran::preview_view::{Point, Size};
                                let viewport = Size {
                                    width: canvas.width(),
                                    height: canvas.height(),
                                };
                                let fitted = Size {
                                    width: size.x,
                                    height: size.y,
                                };
                                if response.hovered() {
                                    let (wheel, center_zoom, cursor) = ui.input(|i| {
                                        (
                                            i.smooth_scroll_delta().y,
                                            i.zoom_delta(),
                                            i.pointer.hover_pos(),
                                        )
                                    });
                                    if wheel.abs() > 0.01
                                        && let Some(cursor) = cursor
                                    {
                                        self.state = self.call(Command::ZoomPreview {
                                            multiplier: (wheel * 0.003).exp(),
                                            anchor: Point {
                                                x: cursor.x - canvas.min.x,
                                                y: cursor.y - canvas.min.y,
                                            },
                                            viewport,
                                            fitted,
                                        });
                                    }
                                    if (center_zoom - 1.0).abs() > 0.001 {
                                        self.state = self.call(Command::ZoomPreview {
                                            multiplier: center_zoom,
                                            anchor: Point {
                                                x: viewport.width / 2.0,
                                                y: viewport.height / 2.0,
                                            },
                                            viewport,
                                            fitted,
                                        });
                                    }
                                }
                                let factor =
                                    self.state["preview_view"]["factor"].as_f64().unwrap_or(1.0);
                                export_started &= factor <= 1.0;
                                let primary_pan = response.dragged_by(egui::PointerButton::Primary)
                                    && intent == Some(PreviewDrag::Pan);
                                let middle_pan = response.dragged_by(egui::PointerButton::Middle);
                                if let Some(command) = preview_pan_command(
                                    primary_pan,
                                    middle_pan,
                                    ctx.input(|i| i.pointer.delta()),
                                    viewport,
                                    fitted,
                                ) {
                                    self.state = self.call(command);
                                }
                                if primary_pan || middle_pan {
                                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                } else if response.hovered()
                                    && preview_drag(factor) == PreviewDrag::Pan
                                {
                                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                                }
                                let geometry =
                                    self.call(Command::PreviewGeometry { viewport, fitted });
                                if let (Some(left), Some(top), Some(width), Some(height)) = (
                                    geometry["left"].as_f64(),
                                    geometry["top"].as_f64(),
                                    geometry["width"].as_f64(),
                                    geometry["height"].as_f64(),
                                ) {
                                    image_rect = Some(egui::Rect::from_min_size(
                                        canvas.min + Vec2::new(left as f32, top as f32),
                                        Vec2::new(width as f32, height as f32),
                                    ));
                                }
                            }
                            if !ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                                ctx.data_mut(|data| data.remove::<PreviewDrag>(drag_id));
                            }
                            self.images.paint_preview(ui, event, canvas, image_rect);
                            if export_started {
                                self.start_drag(vec![id], ctx);
                            }
                            if let Some(index) = visible.iter().position(|e| e["id"] == id) {
                                let neighbors = [
                                    index.checked_sub(1),
                                    index.checked_add(1).filter(|n| *n < visible.len()),
                                ];
                                for neighbor in neighbors.into_iter().flatten() {
                                    self.images.prefetch(ctx, &visible[neighbor], 1400);
                                }
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
                    let pods = self.state["tab"] == "pods";
                    let cell_width = if pods {
                        f32::from(self.pods_tile_size).min(
                            ((ui.available_width() - 16.0 - gap * (columns - 1) as f32)
                                / columns as f32)
                                .max(1.0),
                        )
                    } else if gallery {
                        ((ui.available_width() - 16.0 - gap * (columns - 1) as f32)
                            / columns as f32)
                            .max(1.0)
                    } else {
                        (ui.available_width() - 16.0).max(80.0)
                    };
                    let height = if pods {
                        cell_width
                    } else if gallery {
                        cell_width + 54.0
                    } else {
                        stream_row_height(ui.available_height(), self.stream_rows, gap)
                    };
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
                                    let criterion =
                                        sorting_ui::criterion(event, &self.state["sort"]);
                                    let id = event["id"].as_i64().unwrap();
                                    let selected = self.state["selected"]
                                        .as_array()
                                        .is_some_and(|s| s.contains(&Value::from(id)));
                                    let cursor = self.state["cursor"] == id;
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(cell_width, height),
                                        egui::Sense::click_and_drag(),
                                    );
                                    #[cfg(test)]
                                    if pods {
                                        ctx.data_mut(|data| {
                                            data.insert_temp(
                                                egui::Id::new("test-pod-tile-rect"),
                                                rect,
                                            )
                                        });
                                    }
                                    ui.painter().rect_filled(
                                        rect,
                                        8.0,
                                        theme_ui::color(
                                            ctx,
                                            if selected {
                                                theme_ui::Role::Selection
                                            } else {
                                                theme_ui::Role::Surface
                                            },
                                            Color32::from_gray(if selected { 74 } else { 46 }),
                                        ),
                                    );
                                    theme_ui::paint_optical(ui, rect, 8.0);
                                    if cursor {
                                        ui.painter().rect_stroke(
                                            rect,
                                            8.0,
                                            egui::Stroke::new(
                                                1.5,
                                                theme_ui::color(
                                                    ctx,
                                                    theme_ui::Role::Accent,
                                                    Color32::from_gray(180),
                                                ),
                                            ),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    if pods {
                                        self.pods_ui.paint(ui, event, rect.shrink(6.0));
                                    } else if gallery {
                                        let zones = gallery_card::geometry(rect);
                                        if self.state["tab"] == "pods" {
                                            self.pods_ui.paint(ui, event, zones.image);
                                        } else if event["kind"] == "table_fragment" {
                                            table_preview_ui::paint_card(ui, event, zones.image);
                                        } else if matches!(
                                            event["kind"].as_str(),
                                            Some("text_fragment" | "link_fragment")
                                        ) {
                                            text_preview_ui::paint_card(ui, event, zones.image);
                                        } else {
                                            self.images.paint_at(ui, event, zones.image, 512);
                                        }
                                        let title = if let Some(name) =
                                            event["properties"]["display_name"].as_str()
                                        {
                                            name.to_owned()
                                        } else if event["kind"] == "table_fragment" {
                                            table_preview_ui::card_title(event)
                                        } else if matches!(
                                            event["kind"].as_str(),
                                            Some("text_fragment" | "link_fragment")
                                        ) {
                                            text_preview_ui::card_title(event)
                                        } else {
                                            event["name"].as_str().unwrap_or("Файл").to_owned()
                                        };
                                        gallery_card::text(
                                            ui,
                                            zones.name,
                                            &title,
                                            14.0,
                                            Color32::from_gray(235),
                                        );
                                        let origin: katran::origin::Origin =
                                            serde_json::from_value(event["origin"].clone())
                                                .unwrap_or_default();
                                        let origin_label = origin
                                            .label()
                                            .unwrap_or_else(|| "Источник не определён".into());
                                        let label = criterion.as_ref().map_or_else(
                                            || origin_label.clone(),
                                            |criterion| {
                                                if self.state["sort"]["mode"] == "origin" {
                                                    criterion.clone()
                                                } else {
                                                    format!("{criterion} · {origin_label}")
                                                }
                                            },
                                        );
                                        ui.painter_at(zones.origin).rect_filled(
                                            zones.origin,
                                            4.0,
                                            theme_ui::color(
                                                ctx,
                                                theme_ui::Role::Raised,
                                                Color32::from_gray(55),
                                            ),
                                        );
                                        gallery_card::text(
                                            ui,
                                            zones.origin.shrink2(Vec2::new(5.0, 0.0)),
                                            &label,
                                            12.0,
                                            Color32::from_rgb(183, 200, 203),
                                        );
                                    } else {
                                        let image_height = (rect.height() - 20.0).max(1.0);
                                        let image = egui::Rect::from_min_size(
                                            rect.min + Vec2::splat(10.0),
                                            Vec2::new(
                                                (image_height * 1.4667).min(176.0),
                                                image_height,
                                            ),
                                        );
                                        if self.state["tab"] == "pods" {
                                            self.pods_ui.paint(ui, event, image);
                                        } else if event["kind"] == "table_fragment" {
                                            table_preview_ui::paint_card(ui, event, image);
                                        } else if matches!(
                                            event["kind"].as_str(),
                                            Some("text_fragment" | "link_fragment")
                                        ) {
                                            text_preview_ui::paint_card(ui, event, image);
                                        } else {
                                            self.images.paint_at(ui, event, image, 320);
                                        }
                                        let left = image.right() + 16.0;
                                        let width = (rect.right() - left - 10.0).max(1.0);
                                        let origin: katran::origin::Origin =
                                            serde_json::from_value(event["origin"].clone())
                                                .unwrap_or_default();
                                        let lines = [
                                            if let Some(name) =
                                                event["properties"]["display_name"].as_str()
                                            {
                                                name.to_owned()
                                            } else if event["kind"] == "table_fragment" {
                                                table_preview_ui::card_title(event)
                                            } else if matches!(
                                                event["kind"].as_str(),
                                                Some("text_fragment" | "link_fragment")
                                            ) {
                                                text_preview_ui::card_title(event)
                                            } else {
                                                event["name"].as_str().unwrap_or("Файл").to_owned()
                                            },
                                            criterion.clone().unwrap_or_else(|| {
                                                event["source"].as_str().unwrap_or("").to_owned()
                                            }),
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
                                            if rect.top() + 12.0 + index as f32 * 28.0 + 24.0
                                                > rect.bottom()
                                            {
                                                break;
                                            }
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
                                    if event["properties"]["pinned"] == true {
                                        ui.painter().text(
                                            rect.right_top() + egui::vec2(-8.0, 8.0),
                                            egui::Align2::RIGHT_TOP,
                                            "•",
                                            egui::FontId::proportional(22.0),
                                            Color32::from_rgb(224, 188, 127),
                                        );
                                    }
                                    let origin: katran::origin::Origin =
                                        serde_json::from_value(event["origin"].clone())
                                            .unwrap_or_default();
                                    let response = response.on_hover_text(format!(
                                        "{}\n{}\n{}",
                                        event["name"].as_str().unwrap_or("Файл"),
                                        origin
                                            .label()
                                            .unwrap_or_else(|| "Источник не определён".into()),
                                        criterion.as_deref().unwrap_or("")
                                    ));
                                    if response.drag_started() {
                                        if !selected {
                                            self.state = self.call(Command::Select {
                                                id,
                                                mode: katran::interaction::SelectionMode::Replace,
                                            });
                                        }
                                        let ids = self.state["selected"]
                                            .as_array()
                                            .into_iter()
                                            .flatten()
                                            .filter_map(Value::as_i64)
                                            .collect();
                                        self.start_drag(ids, ctx);
                                    }
                                    if response.clicked() || response.double_clicked() {
                                        self.state = self.call(Command::Select {
                                            id,
                                            mode: mode(ctx.input(|i| i.modifiers)),
                                        });
                                        if response.double_clicked() {
                                            self.start_job(
                                                Command::RunAction { id: "open".into() },
                                                false,
                                                ctx.clone(),
                                            );
                                        }
                                    }
                                }
                            });
                        }
                    });
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
    fn pods_preview_metadata_layout_keeps_canvas_full_width_and_scroll_bounded() {
        for (width, height) in [(320.0, 360.0), (960.0, 680.0)] {
            let root = tempfile::tempdir().unwrap();
            let mut app = super::super::tests::test_shelf(root.path());
            app.call(Command::SetTabEnabled {
                tab: katran::interaction::ShelfTab::Pods,
                enabled: true,
            });
            let event = app.call(Command::IngestText {
                text: "🐄".into(),
                source: "test".into(),
            });
            let id = event["id"].as_i64().unwrap();
            app.call(Command::SetTab {
                tab: katran::interaction::ShelfTab::Pods,
            });
            app.call(Command::SetVisible { ids: vec![id] });
            app.call(Command::Select {
                id,
                mode: SelectionMode::Replace,
            });
            app.state = app.call(Command::TogglePreview);
            let title = "UNICODE NAME ".repeat(4);
            app.pods_ui.set_description(
                id,
                event["revision"].as_str().unwrap(),
                serde_json::json!({
                    "text": "🐄", "name": title, "utf8_bytes": 4, "utf16_units": 2,
                    "codepoints": [], "metadata_truncated": false
                }),
            );
            let ctx = egui::Context::default();
            let mut layout = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, height),
                    )),
                    ..Default::default()
                },
                |ui| {
                    layout = Some(pods_preview_layout(
                        (ui.available_width() - 24.0).max(0.0),
                        (ui.available_height() - 24.0 - 30.0).max(32.0),
                    ));
                    app.shelf_view(ui, &ctx, std::slice::from_ref(&event), true, 9);
                },
            );
            output.textures_delta.clear();
            let layout = layout.expect("preview layout measured");
            let canvas = ctx
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("pod-preview-canvas")))
                .expect("actual canvas");
            assert!(
                (canvas.center().x - width / 2.0).abs() <= 2.0,
                "canvas must use the centered content width"
            );
            assert!(layout.canvas.y <= 300.0);
            assert!(layout.metadata_height >= 0.0);
            fn find_text(shape: &egui::Shape, needle: &str) -> Option<egui::Rect> {
                match shape {
                    egui::Shape::Text(text) if text.galley.text().contains(needle) => {
                        Some(text.visual_bounding_rect())
                    }
                    egui::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| find_text(shape, needle))
                    }
                    _ => None,
                }
            }
            let title_rect = output
                .shapes
                .iter()
                .find_map(|shape| find_text(&shape.shape, "UNICODE NAME"))
                .expect("long metadata title should be painted");
            let summary_rect = output
                .shapes
                .iter()
                .find_map(|shape| find_text(&shape.shape, "UTF-8: 4 байт · UTF-16: 2"))
                .expect("UTF summary should be painted");
            let center_x = width / 2.0;
            assert!(
                (title_rect.center().x - center_x).abs() <= 2.0,
                "title should be centered at width {width}: {title_rect:?}"
            );
            assert!(
                (summary_rect.center().x - center_x).abs() <= 2.0,
                "summary should be centered at width {width}: {summary_rect:?}"
            );
            assert!(
                title_rect.top() >= canvas.bottom(),
                "title must follow canvas at width {width}"
            );
            assert!(
                summary_rect.top() >= title_rect.bottom(),
                "summary must follow title at width {width}"
            );
            output.textures_delta.clear();
        }
    }

    #[test]
    fn pods_tiles_are_square_without_text_plaques() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.state = serde_json::json!({"tab":"pods","selected":[],"cursor":1,"preview":null,"sort":{"mode":"arrival"}});
        let event = serde_json::json!({"id":1,"kind":"text_fragment","name":"SENTINEL TITLE","text_preview":"🐄","origin":{"source_app":"SENTINEL ORIGIN"},"source":"SENTINEL SOURCE","properties":{"pinned":false},"revision":"r1"});
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 680.0),
                )),
                ..Default::default()
            },
            |ui| app.shelf_view(ui, &ctx, std::slice::from_ref(&event), true, 9),
        );
        fn has_sentinel(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Text(t) => t.galley.text().contains("SENTINEL"),
                egui::Shape::Vec(shapes) => shapes.iter().any(has_sentinel),
                _ => false,
            }
        }
        let labelled = output.shapes.iter().any(|s| has_sentinel(&s.shape));
        output.textures_delta.clear();
        let tile = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("test-pod-tile-rect")))
            .expect("Pods tile allocated");
        assert!(!labelled, "tile must not paint title or origin");
        assert_eq!(tile.width(), tile.height());
        assert!(tile.width() <= 96.0);
    }
    #[test]
    fn pods_preview_loads_full_description_through_dispatcher() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Pods,
            enabled: true,
        });
        let event = app.call(Command::IngestText {
            text: "🐄".into(),
            source: "test".into(),
        });
        let id = event["id"].as_i64().unwrap();
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Pods,
        });
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        app.state = app.call(Command::TogglePreview);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 680.0),
                )),
                ..Default::default()
            },
            |ui| app.shelf_view(ui, &ctx, std::slice::from_ref(&event), true, 9),
        );
        output.textures_delta.clear();
        let value = app.pods_ui.description().expect("description requested");
        assert_eq!(value["text"], "🐄");
        assert_eq!(value["codepoints"][0]["codepoint"], "U+1F404");
        assert!(value["name"].as_str().is_some_and(|name| !name.is_empty()));
    }
    #[test]
    fn preview_primary_drag_only_exports_at_fitted_scale() {
        assert_eq!(preview_drag(1.0), PreviewDrag::Export);
        assert_eq!(preview_drag(1.000001), PreviewDrag::Pan);
        assert_eq!(preview_drag(1.01), PreviewDrag::Pan);
        assert_eq!(preview_drag(16.0), PreviewDrag::Pan);
    }

    #[test]
    fn drag_intent_stays_latched_when_zoom_changes() {
        let pan = latched_preview_drag(None, true, 2.0);
        assert_eq!(pan, Some(PreviewDrag::Pan));
        assert_eq!(latched_preview_drag(pan, false, 1.0), pan);
        let export = latched_preview_drag(None, true, 1.0);
        assert_eq!(export, Some(PreviewDrag::Export));
        assert_eq!(latched_preview_drag(export, true, 2.0), pan);
    }

    #[test]
    fn zoomed_preview_without_texture_never_starts_export() {
        // Reproduce dragging before the texture is available, with and without
        // Ctrl and while the dispatcher is temporarily owned by a worker.
        for ctrl in [false, true] {
            for busy in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let mut app = super::super::tests::test_shelf(root.path());
                app.call(Command::SetTab {
                    tab: katran::interaction::ShelfTab::Images,
                });
                app.call(Command::IngestBytes {
                    name: "synthetic.png".into(),
                    bytes: vec![1],
                    source: "test".into(),
                });
                app.refresh();
                app.state = app.call(Command::SetVisible {
                    ids: app
                        .events
                        .iter()
                        .map(|event| event["id"].as_i64().unwrap())
                        .collect(),
                });
                app.state = app.call(Command::TogglePreview);
                let size = Size {
                    width: 200.0,
                    height: 200.0,
                };
                app.state = app.call(Command::ZoomPreview {
                    multiplier: 1.01,
                    anchor: Point { x: 100.0, y: 100.0 },
                    viewport: size,
                    fitted: size,
                });
                assert!(app.state["preview_view"]["factor"].as_f64().unwrap() > 1.0);
                let held_api = if busy { app.api.take() } else { None };
                let ctx = egui::Context::default();
                let modifiers = egui::Modifiers {
                    ctrl,
                    ..Default::default()
                };
                for events in [
                    vec![egui::Event::PointerMoved(egui::pos2(200.0, 200.0))],
                    vec![egui::Event::PointerButton {
                        pos: egui::pos2(200.0, 200.0),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    }],
                    vec![egui::Event::PointerMoved(egui::pos2(260.0, 240.0))],
                ] {
                    let visible = app.events.clone();
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(900.0, 680.0),
                        )),
                        events,
                        ..Default::default()
                    };
                    let mut output =
                        ctx.run_ui(input, |ui| app.shelf_view(ui, &ctx, &visible, true, 4));
                    output.textures_delta.clear();
                    assert!(
                        app.job.is_none(),
                        "zoomed drag must not prepare a native export"
                    );
                    assert!(app.native_drag.is_none());
                }
                drop(held_api);
            }
        }
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

#[cfg(test)]
mod density_geometry_tests {
    use super::stream_row_height;
    #[test]
    fn stream_rows_fill_viewport_at_each_density() {
        for rows in 1..=7 {
            let height = stream_row_height(600.0, rows, 8.0);
            assert!((height * f32::from(rows) + 8.0 * f32::from(rows - 1) - 600.0).abs() < 0.01);
        }
        assert_eq!(stream_row_height(300.0, 12, 8.0), 72.0);
    }
}

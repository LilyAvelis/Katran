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
                    ui.label("Просмотр · Esc — назад");
                    if let Some(event) = visible.iter().find(|event| event["id"] == id) { self.images.draw(
                        ui,
                        event,
                        (ui.available_size() - egui::vec2(0.0, 24.0)).max(Vec2::splat(32.0)),
                        1400,
                    ); }
                } else {
                    if visible.is_empty() {
                        ui.label(
                            "Полка пуста. Ctrl K → Включить захват, затем скопируйте изображение.",
                        );
                    }
                    let height = if gallery { 220.0 } else { 150.0 };
                    let cell_width = if gallery {
                        (ui.available_width() - 8.0 * (columns - 1) as f32) / columns as f32
                    } else {
                        ui.available_width()
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
                            height - 8.0,
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
                                        egui::vec2(cell_width, height - 8.0),
                                        egui::Sense::click(),
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
                                    ui.scope_builder(
                                        egui::UiBuilder::new()
                                            .max_rect(rect.shrink(8.0))
                                            .layout(egui::Layout::top_down(egui::Align::Min)),
                                        |ui| {
                                            if gallery {
                                                self.images.draw(
                                                    ui,
                                                    event,
                                                    egui::vec2(cell_width - 16.0, 164.0),
                                                    320,
                                                );
                                                ui.add(
                                                    egui::Label::new(
                                                        event["name"].as_str().unwrap_or("Файл"),
                                                    )
                                                    .truncate(),
                                                );
                                            } else {
                                                ui.horizontal(|ui| {
                                                    self.images.draw(
                                                        ui,
                                                        event,
                                                        egui::vec2(176.0, 120.0),
                                                        320,
                                                    );
                                                    ui.vertical(|ui| {
                                                        ui.label(
                                                            event["name"]
                                                                .as_str()
                                                                .unwrap_or("Файл"),
                                                        );
                                                        ui.weak(
                                                            event["source"].as_str().unwrap_or(""),
                                                        );
                                                        let origin:katran::origin::Origin=serde_json::from_value(event["origin"].clone()).unwrap_or_default();
                                                        ui.label(RichText::new(origin.label().map(|s|format!("Из: {s}")).unwrap_or_else(||"Источник не определён".into())).small().color(Color32::from_rgb(183,200,203)));
                                                        ui.weak(if event["available"] == false {
                                                            "Исходник недоступен"
                                                        } else if event["storage"] == "cached" {
                                                            "Временная копия"
                                                        } else {
                                                            "Исходный файл"
                                                        });
                                                    });
                                                });
                                            }
                                        },
                                    );
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
                if self.job.is_some() { ui.label("Выполняется операция…"); }
                if !self.message.is_empty() {
                    ui.label(&self.message);
                }
            });
        self.images.end_frame(ctx);
    }
}

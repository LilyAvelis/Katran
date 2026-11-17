//! Cursor-driven split shelf: presentation only; all actions use the dispatcher.
use super::i18n::tr;
use super::*;

const ROW_HEIGHT: f32 = 40.0;

impl Shelf {
    pub(super) fn details_view(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        visible: &[Value],
    ) {
        self.pods_ui.begin_frame();
        if visible.is_empty() {
            self.details_metadata.reset();
            ui.label(if self.query.trim().is_empty() {
                tr("Полка пуста", "Shelf is empty")
            } else {
                tr("Ничего не найдено", "No results found")
            });
            self.pods_ui.end_frame();
            return;
        }
        self.pods_ui.end_frame();
        let width = ui.available_width().max(1.0);
        let height = ui.available_height().max(1.0);
        if width >= 480.0 {
            let left = (width * 0.38).clamp(180.0, width - 200.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.details_list(ui, ctx, visible),
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width().max(1.0), height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.details_panel(ui, visible),
                );
            });
        } else {
            let list_height = (height * 0.4).max(1.0);
            ui.allocate_ui_with_layout(
                Vec2::new(width, list_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.details_list(ui, ctx, visible),
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                Vec2::new(width, ui.available_height().max(1.0)),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.details_panel(ui, visible),
            );
        }
    }

    fn details_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, visible: &[Value]) {
        let height = ui.available_height().max(1.0);
        let scroll = self.scroll;
        self.scroll = false;
        let grouped = self.state["tab"] == "pods" && self.state["emoji_catalogue"] == true;
        let layout = category_divider::layout(visible, 1, ROW_HEIGHT, 2.0, grouped);
        let scroll_id = ui.make_persistent_id("details-list");
        let mut area = egui::ScrollArea::vertical()
            .id_salt("details-list")
            .max_height(height)
            .auto_shrink([false, false]);
        if scroll && let Some(index) = visible.iter().position(|event| event["id"] == self.state["cursor"])
            && let Some(row) = layout.rows.iter().find(|row| matches!(&row.kind, category_divider::Kind::Cards(range) if range.contains(&index)))
        {
            let offset = egui::scroll_area::State::load(ctx, scroll_id).map_or(0.0, |state| state.offset.y);
            area = area.vertical_scroll_offset(ensure_visible_offset(offset, height, row.top, row.height));
        }
        area.show_viewport(ui, |ui, viewport| {
            let width = ui.available_width().max(1.0);
            let (content, _) =
                ui.allocate_exact_size(Vec2::new(width, layout.height), egui::Sense::hover());
            for (index, row) in layout.rows.iter().enumerate() {
                if row.top + row.height < viewport.top() || row.top > viewport.bottom() {
                    continue;
                }
                let rect = egui::Rect::from_min_size(
                    content.min + egui::vec2(0.0, row.top),
                    Vec2::new(width, row.height),
                );
                match &row.kind {
                    category_divider::Kind::Divider { label, .. } => {
                        ui.painter().text(
                            rect.left_center() + egui::vec2(5.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(12.0),
                            Color32::from_gray(180),
                        );
                    }
                    category_divider::Kind::Cards(range) => {
                        for event in &visible[range.clone()] {
                            let Some(id) = event["id"].as_i64() else {
                                continue;
                            };
                            let selected = self.state["selected"]
                                .as_array()
                                .is_some_and(|ids| ids.contains(&Value::from(id)));
                            let cursor = self.state["cursor"] == id;
                            let response = ui.interact(
                                rect,
                                ui.id().with(("details-row", index, id)),
                                egui::Sense::click_and_drag(),
                            );
                            if cursor || response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    5.0,
                                    ui.visuals().widgets.hovered.weak_bg_fill,
                                );
                            }
                            if let Some(slot) =
                                self.tag_material_cache.assignments.get(&id).and_then(|n| {
                                    self.tag_material_cache
                                        .slots
                                        .iter()
                                        .find(|slot| slot.number == *n)
                                })
                            {
                                super::tag_material_ui::paint(ui, rect, slot);
                            }
                            let label = event["properties"]["display_name"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .or_else(|| event["name"].as_str())
                                .or_else(|| event["text_preview"].as_str())
                                .unwrap_or(tr("Объект", "Item"));
                            let icon = match event["kind"].as_str() {
                                Some("managed_image") => egui_lucide::Lucide::Image,
                                Some("text_fragment") => egui_lucide::Lucide::FileText,
                                Some("link_fragment") => egui_lucide::Lucide::Link,
                                Some("table_fragment") => egui_lucide::Lucide::Table,
                                _ => egui_lucide::Lucide::File,
                            };
                            let icon_rect = egui::Rect::from_center_size(
                                rect.left_center() + egui::vec2(16.0, 0.0),
                                egui::vec2(18.0, 18.0),
                            );
                            icon.size(18.0)
                                .image()
                                .tint(Color32::from_gray(200))
                                .paint_at(ui, icon_rect);
                            let line = egui::Rect::from_min_max(
                                rect.min + egui::vec2(32.0, 0.0),
                                rect.max - egui::vec2(8.0, 0.0),
                            );
                            super::gallery_card::text(
                                ui,
                                line,
                                label,
                                14.0,
                                Color32::from_gray(235),
                            );
                            if let Some(stroke) = super::shelf_view::card_outline(ctx, selected) {
                                ui.painter().rect_stroke(
                                    rect,
                                    5.0,
                                    stroke,
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if cursor {
                                super::shelf_view::paint_cursor(ui, rect);
                            }
                            let response = response.on_hover_text(label);
                            if response.drag_started() {
                                if !selected {
                                    self.state = self.call(Command::Select {
                                        id,
                                        mode: SelectionMode::Replace,
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
                    }
                }
            }
        });
    }

    fn details_panel(&mut self, ui: &mut egui::Ui, visible: &[Value]) {
        let Some(event) = visible
            .iter()
            .find(|event| event["id"] == self.state["cursor"])
        else {
            self.details_metadata.reset();
            ui.weak(tr("Выберите объект", "Choose an item"));
            return;
        };
        let preview_height = (ui.available_height() * 0.55).max(1.0);
        let width = ui.available_width().max(1.0);
        ui.allocate_ui_with_layout(
            Vec2::new(width, preview_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_max_height(preview_height);
                ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                if event["available"] == false {
                    ui.weak(tr("Объект недоступен", "Item unavailable"));
                } else if self.state["tab"] == "pods" && event["kind"] == "text_fragment" {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(width, preview_height),
                        egui::Sense::hover(),
                    );
                    self.pods_ui.paint(ui, event, rect);
                } else if event["kind"] == "table_fragment" {
                    if let Some(api) = self.api.as_mut() {
                        self.table_preview.show(ui, event, api);
                    } else {
                        ui.weak(tr("Загрузка…", "Loading…"));
                    }
                } else if matches!(
                    event["kind"].as_str(),
                    Some("text_fragment" | "link_fragment")
                ) || katran::file_types::is_markdown(std::path::Path::new(
                    event["name"].as_str().unwrap_or(""),
                )) {
                    self.text_preview.show(ui, event);
                } else {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(width, (preview_height - 22.0).max(1.0)),
                        egui::Sense::hover(),
                    );
                    self.images.paint_at(ui, event, rect, 512);
                    ui.weak(match self.images.visual_status(event, 512) {
                        "image" => tr("Изображение", "Image"),
                        "shell_thumbnail" => tr("Миниатюра", "Thumbnail"),
                        "shell_icon" | "generic_icon" => tr("Значок файла", "File icon"),
                        "unavailable" => tr("Предпросмотр недоступен", "Preview unavailable"),
                        _ => tr("Загрузка…", "Loading…"),
                    });
                }
            },
        );
        ui.separator();
        let state = self.state.clone();
        let tag = event["id"]
            .as_i64()
            .and_then(|id| self.tag_material_cache.assignments.get(&id))
            .and_then(|number| {
                self.tag_material_cache
                    .slots
                    .iter()
                    .find(|slot| slot.number == *number)
            })
            .map(|slot| slot.name.clone());
        egui::ScrollArea::vertical()
            .id_salt("details-metadata")
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                self.details_metadata.show(ui, event, &state);
                if let Some(tag) = tag {
                    ui.weak(tr("Тег", "Tag"));
                    ui.label(tag);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_details_action_and_arrow_use_cursor_without_fullscreen_preview() {
        use katran::interaction::{ShelfTab, View};
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        });
        for text in ["first", "second"] {
            app.call(Command::IngestText {
                text: text.into(),
                source: "test".into(),
            });
        }
        app.state = app.call(Command::SetTab {
            tab: ShelfTab::Text,
        });
        let ctx = egui::Context::default();
        app.run("details".into(), &ctx);
        assert_eq!(app.call(Command::Interaction)["view"], "details");
        app.refresh();
        assert!(app.events.len() >= 2);
        let ids: Vec<i64> = app
            .events
            .iter()
            .filter_map(|event| event["id"].as_i64())
            .collect();
        app.call(Command::SetVisible { ids: ids.clone() });
        app.state = app.call(Command::Select {
            id: ids[0],
            mode: SelectionMode::Replace,
        });
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
        let after = app.call(Command::Interaction);
        assert_eq!(after["cursor"], ids[1]);
        assert!(after["preview"].is_null());
        assert_eq!(after["view"], "details");
        assert_eq!(serde_json::to_value(View::Details).unwrap(), "details");
    }

    #[test]
    fn details_show_cursor_metadata_across_providers_without_fullscreen_preview() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        for (tab, kind, id) in [
            ("images", "managed_image", 1_i64),
            ("text", "text_fragment", 2),
            ("links", "link_fragment", 3),
            ("tables", "table_fragment", 4),
            ("pods", "text_fragment", 5),
            ("folder", "file_reference", -2),
            ("desktop", "managed_image", -(1_i64 << 62) - 1),
            ("pods", "text_fragment", -(1_i64 << 61)),
        ] {
            app.state = serde_json::json!({"tab":tab,"view":"details","cursor":id,"selected":[],"preview":null,"sort":{"mode":"arrival"},"emoji_catalogue":tab=="pods" && id<0,"view_generation":1});
            let event = serde_json::json!({"id":id,"revision":"r1","kind":kind,"name":"CURSOR-ITEM","text_preview":"sample","source":"test","available":true,"path":"C:\\synthetic","properties":{"pinned":false}});
            let other = serde_json::json!({"id":99,"revision":"r2","kind":"file_reference","name":"OTHER-ITEM","properties":{}});
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    ..Default::default()
                },
                |ui| app.shelf_view(ui, &ctx, &[event.clone(), other.clone()], false, 1),
            );
            fn collect(shape: &egui::Shape, text: &mut String) {
                match shape {
                    egui::Shape::Text(value) => {
                        text.push_str(&value.galley.job.text);
                        text.push('\n');
                    }
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            collect(shape, text);
                        }
                    }
                    _ => {}
                }
            }
            let mut text = String::new();
            for shape in &output.shapes {
                collect(&shape.shape, &mut text);
            }
            output.textures_delta.clear();
            assert!(text.contains("CURSOR-ITEM"), "{tab}: {text:?}");
            assert!(app.state["preview"].is_null());
            assert_eq!(app.state["cursor"], id);
            assert_eq!(app.state["selected"], serde_json::json!([]));
        }
    }
}

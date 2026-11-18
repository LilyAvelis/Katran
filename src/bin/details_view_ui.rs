//! Cursor-driven split shelf: presentation only; all actions use the dispatcher.
use super::i18n::tr;
use super::*;

const ROW_HEIGHT: f32 = 40.0;

pub(super) fn display_title(event: &Value, tab: &str) -> String {
    if let Some(name) = event["properties"]["display_name"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
    {
        return name.to_owned();
    }
    if tab == "text" && event["kind"] == "text_fragment" {
        let title = katran::text_fragments::title(event["text_preview"].as_str().unwrap_or(""));
        return if title.is_empty() {
            tr("Текст", "Text").into()
        } else {
            title
        };
    }
    event["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            event["text_preview"]
                .as_str()
                .filter(|text| !text.is_empty())
        })
        .unwrap_or(tr("Объект", "Item"))
        .to_owned()
}

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
                            let label =
                                display_title(event, self.state["tab"].as_str().unwrap_or(""));
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
                            if super::images::is_bs(event) {
                                super::images::paint_bs_badge(ui, icon_rect);
                            } else if super::images::is_bull(event) {
                                super::images::paint_bull_badge(ui, icon_rect);
                            } else {
                                icon.size(18.0)
                                    .image()
                                    .tint(Color32::from_gray(200))
                                    .paint_at(ui, icon_rect);
                            }
                            let line = egui::Rect::from_min_max(
                                rect.min + egui::vec2(32.0, 0.0),
                                rect.max - egui::vec2(24.0, 0.0),
                            );
                            super::gallery_card::text(
                                ui,
                                line,
                                &label,
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
                            if event["properties"]["pinned"] == true {
                                let color = self
                                    .tag_material_cache
                                    .assignments
                                    .get(&id)
                                    .and_then(|number| {
                                        self.tag_material_cache
                                            .slots
                                            .iter()
                                            .find(|slot| slot.number == *number)
                                    })
                                    .map(super::tag_material_ui::color)
                                    .unwrap_or(Color32::from_rgb(236, 159, 72));
                                let dot = rect.right_center() - egui::vec2(10.0, 0.0);
                                ui.painter().circle_filled(dot, 4.0, color);
                                ui.painter().circle_stroke(
                                    dot,
                                    4.0,
                                    egui::Stroke::new(1.0, Color32::from_black_alpha(115)),
                                );
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
        // Reserve the slot before drawing: asynchronous preview content must not
        // change the location or size of the metadata viewport.
        let (preview_rect, _) =
            ui.allocate_exact_size(Vec2::new(width, preview_height), egui::Sense::hover());
        let mut preview_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("details-preview")
                .max_rect(preview_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        preview_ui.set_clip_rect(ui.clip_rect().intersect(preview_rect));
        {
            let ui = &mut preview_ui;
            if event["available"] == false {
                ui.weak(tr("Объект недоступен", "Item unavailable"));
            } else if let Some(target) = super::media_preview_ui::target(event, &self.state, false)
            {
                if let Some(api) = self.api.as_mut() {
                    self.media_preview.show_with_poster(
                        ui,
                        &target,
                        event,
                        &mut self.images,
                        &mut |command| {
                            api.execute(Request {
                                api_version: 1,
                                request_id: "media-ui".into(),
                                command,
                            })
                        },
                    );
                } else {
                    ui.weak(tr(
                        "Видео временно недоступно",
                        "Video temporarily unavailable",
                    ));
                }
            } else if self.state["tab"] == "pods" && event["kind"] == "text_fragment" {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, preview_height), egui::Sense::hover());
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
                ui.weak(
                    if event["source"] == "text_collection" && super::images::is_bull(event) {
                        tr("Коллекция текстов", "Text collection")
                    } else if event["source"] == "link_collection" && super::images::is_bull(event)
                    {
                        tr("Коллекция ссылок", "Link collection")
                    } else if matches!(
                        event["source"].as_str(),
                        Some("image_translation_collection" | "image_collection")
                    ) && super::images::is_bull(event)
                    {
                        tr("Коллекция изображений", "Image collection")
                    } else {
                        match self.images.visual_status(event, 512) {
                            "image" => tr("Изображение", "Image"),
                            "shell_thumbnail" => tr("Миниатюра", "Thumbnail"),
                            "shell_icon" | "generic_icon" => tr("Значок файла", "File icon"),
                            "unavailable" => tr("Предпросмотр недоступен", "Preview unavailable"),
                            _ => tr("Загрузка…", "Loading…"),
                        }
                    },
                );
            }
        }
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
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                #[cfg(test)]
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::new("test-details-geometry"),
                        (preview_rect, ui.id(), ui.clip_rect()),
                    )
                });
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

    fn painted_details_list(
        app: &mut Shelf,
        ctx: &egui::Context,
        events: &[Value],
    ) -> (String, Vec<(egui::Pos2, Color32)>) {
        fn collect(shape: &egui::Shape, text: &mut String, dots: &mut Vec<(egui::Pos2, Color32)>) {
            match shape {
                egui::Shape::Text(value) => {
                    text.push_str(&value.galley.job.text);
                    text.push('\n');
                }
                egui::Shape::Circle(value)
                    if value.radius == 4.0 && value.fill != Color32::TRANSPARENT =>
                {
                    dots.push((value.center, value.fill))
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, text, dots);
                    }
                }
                _ => {}
            }
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                ..Default::default()
            },
            |ui| app.details_list(ui, ctx, events),
        );
        let mut text = String::new();
        let mut dots = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, &mut text, &mut dots);
        }
        output.textures_delta.clear();
        (text, dots)
    }

    #[test]
    fn details_metadata_geometry_stays_fixed_while_preview_loads_and_items_change() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Links,
            enabled: true,
        });
        app.text_preview.set_enabled(true);
        let first_url = "https://example.test/first";
        let second_url = format!("https://example.test/{}", "long-segment/".repeat(30));
        let first = app.call(Command::IngestLink {
            url: first_url.into(),
            source: "synthetic".into(),
        });
        let second = app.call(Command::IngestLink {
            url: second_url.clone(),
            source: "synthetic".into(),
        });
        let first_id = first["id"].as_i64().unwrap();
        let second_id = second["id"].as_i64().unwrap();
        app.state = serde_json::json!({"tab":"links","cursor":first_id,"selected":[],"emoji_catalogue":false,"view_generation":1});
        let mut events = vec![
            first,
            second,
            serde_json::json!({"id":3,"revision":"synthetic","kind":"managed_image","name":"synthetic-image","available":false,"properties":{}}),
        ];
        let ctx = egui::Context::default();
        for width in [310.0, 500.0] {
            let input = || egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 600.0),
                )),
                ..Default::default()
            };
            app.text_preview.stop();
            app.text_preview.set_enabled(true);
            app.state["cursor"] = first_id.into();
            let frame = |app: &mut Shelf, events: &[Value]| {
                let mut out = ctx.run_ui(input(), |ui| app.details_panel(ui, events));
                out.textures_delta.clear();
                ctx.data(|data| {
                    data.get_temp::<(egui::Rect, egui::Id, egui::Rect)>(egui::Id::new(
                        "test-details-geometry",
                    ))
                    .unwrap()
                })
            };
            let loading = frame(&mut app, &events);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !app.text_preview.test_has_text(first_url) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "synthetic link preview did not load"
                );
                assert_eq!(frame(&mut app, &events), loading);
                std::thread::yield_now();
            }
            assert_eq!(frame(&mut app, &events), loading);
            app.state["cursor"] = second_id.into();
            assert_eq!(frame(&mut app, &events), loading);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !app.text_preview.test_has_text(&second_url) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "second synthetic link preview did not load"
                );
                assert_eq!(frame(&mut app, &events), loading);
                std::thread::yield_now();
            }
            assert_eq!(frame(&mut app, &events), loading);
            events[1]["properties"]["pinned"] = true.into();
            assert_eq!(frame(&mut app, &events), loading);
            events[1]["properties"]["pinned"] = false.into();
            assert_eq!(frame(&mut app, &events), loading);
            app.state["cursor"] = 3.into();
            assert_eq!(frame(&mut app, &events), loading);
            let (preview, _, metadata_clip) = loading;
            let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 600.0));
            assert!(bounds.contains_rect(preview) && bounds.contains_rect(metadata_clip));
            assert!(metadata_clip.top() >= preview.bottom());
        }
    }

    #[test]
    fn text_rows_paint_first_meaningful_line_or_explicit_rename() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.state = serde_json::json!({"tab":"text", "cursor":null, "selected":[], "emoji_catalogue":false});
        let ctx = egui::Context::default();
        let events = vec![
            serde_json::json!({"id":1,"kind":"text_fragment","name":"technical-1.txt","text_preview":"\n  First meaningful line  \nsecond", "properties":{}}),
            serde_json::json!({"id":2,"kind":"text_fragment","name":"technical-2.txt","text_preview":"original text", "properties":{"display_name":"My renamed item"}}),
        ];
        let (text, _) = painted_details_list(&mut app, &ctx, &events);
        assert!(text.contains("First meaningful line"), "{text:?}");
        assert!(text.contains("My renamed item"), "{text:?}");
        assert!(
            !text.contains("technical-1.txt") && !text.contains("technical-2.txt"),
            "{text:?}"
        );
        let pods = serde_json::json!({"kind":"text_fragment", "name":"Pods name", "text_preview":"🐄", "properties":{}});
        assert_eq!(display_title(&pods, "pods"), "Pods name");
        assert!(!display_title(&serde_json::json!({"kind":"text_fragment", "name":"technical.txt", "text_preview":" \n", "properties":{}}), "text").contains("technical"));
    }

    #[test]
    fn pinned_markers_are_painted_for_each_provider_and_use_tag_color() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        app.tag_material_cache.slots = vec![katran::tag_slots::Slot {
            number: 1,
            name: "Tag".into(),
            color: "#12A0FF".into(),
            opacity: 0,
            material: katran::tag_slots::Material::Solid,
            scale: 50,
            intensity: 0,
            angle: 0,
            seed: 0,
        }];
        for (tab, kind, id) in [
            ("images", "managed_image", 1_i64),
            ("text", "text_fragment", 2),
            ("links", "link_fragment", 3),
            ("tables", "table_fragment", 4),
            ("pods", "text_fragment", 5),
            ("folder", "file_reference", -2),
            ("desktop", "managed_image", -(1_i64 << 62) - 1),
        ] {
            app.state =
                serde_json::json!({"tab":tab,"cursor":null,"selected":[],"emoji_catalogue":false});
            app.tag_material_cache.assignments.clear();
            let event = serde_json::json!({"id":id,"kind":kind,"name":"Item","text_preview":"Example","properties":{"pinned":true}});
            let (_, dots) = painted_details_list(&mut app, &ctx, std::slice::from_ref(&event));
            assert_eq!(dots.len(), 1, "{tab}: {dots:?}");
            assert_eq!(dots[0].1, Color32::from_rgb(236, 159, 72));
            app.tag_material_cache.assignments.insert(id, 1);
            let (_, dots) = painted_details_list(&mut app, &ctx, std::slice::from_ref(&event));
            assert_eq!(dots.len(), 1, "{tab}: {dots:?}");
            assert_eq!(dots[0].1, Color32::from_rgb(18, 160, 255));
            let mut unpinned = event;
            unpinned["properties"]["pinned"] = false.into();
            let (_, dots) = painted_details_list(&mut app, &ctx, &[unpinned]);
            assert!(dots.is_empty(), "{tab}: {dots:?}");
        }
    }

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
            let expected = if tab == "text" {
                "sample"
            } else {
                "CURSOR-ITEM"
            };
            assert!(text.contains(expected), "{tab}: {text:?}");
            if tab == "text" {
                assert!(
                    !text.contains("CURSOR-ITEM"),
                    "technical text filename must not be its title"
                );
            }
            assert!(app.state["preview"].is_null());
            assert_eq!(app.state["cursor"], id);
            assert_eq!(app.state["selected"], serde_json::json!([]));
        }
    }
}

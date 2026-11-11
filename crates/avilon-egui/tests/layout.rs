use avilon_egui::{
    ActionPresentation, ActionRow, ActionStyle, KeyCaps, KeyCapsStyle, primary_shortcut,
};
use egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use egui_lucide::Lucide;

fn style() -> ActionStyle {
    ActionStyle {
        label_font: egui::FontId::proportional(14.0),
        label_color: Color32::from_rgb(201, 202, 203),
        selection_fill: Color32::from_rgb(31, 42, 53),
        keycaps: KeyCapsStyle {
            font: egui::FontId::proportional(12.0),
            text_color: Color32::from_rgb(220, 221, 222),
            fill: Color32::from_rgb(25, 35, 45),
            stroke: Stroke::new(1.0, Color32::from_rgb(71, 72, 73)),
            corner_radius: 4,
        },
    }
}
fn presentation<'a>(label: &'a str, shortcut: &'a str) -> ActionPresentation<'a> {
    ActionPresentation {
        label,
        icon: Lucide::Copy,
        primary_shortcut: primary_shortcut(shortcut),
        full_shortcut_tooltip: shortcut,
    }
}
fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 300.0))),
        ..Default::default()
    }
}

#[test]
fn rows_and_keycaps_fit_at_multiple_widths_and_scales() {
    let visual = style();
    for scale in [1.0, 1.5, 2.0] {
        for width in [180.0, 240.0, 420.0] {
            for binding in [
                "Ctrl S / Alt S",
                "Ctrl Shift Alt Super ExtremelyLongShortcut",
            ] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                let mut row = Rect::NOTHING;
                let mut caps_size = Vec2::ZERO;
                let mut measured_full = Vec2::ZERO;
                let mut output = ctx.run_ui(input(), |ui| {
                    ui.set_width(width);
                    let hint = primary_shortcut(binding);
                    measured_full = KeyCaps::measure(ui, hint, &visual.keycaps).size;
                    caps_size = KeyCaps::measure_fitting(
                        ui,
                        hint,
                        ActionRow::shortcut_budget(width),
                        &visual.keycaps,
                    )
                    .size;
                    row = ActionRow::new(
                        presentation("A long label that is deliberately clipped", binding),
                        visual.clone(),
                    )
                    .with_texture(Some(egui::TextureId::User(42)))
                    .show(ui, true)
                    .rect;
                });
                output.textures_delta.clear();
                // Images snap to physical pixels, up to half a pixel at each DPI.
                let regions = ActionRow::regions(row, caps_size.x);
                assert!(row.height() >= 38.0);
                assert!(!regions.label.intersects(regions.keys));
                let mut frames = 0;
                let mut icon = 0;
                for clipped in output.shapes {
                    match clipped.shape {
                        egui::Shape::Rect(rect) if rect.fill == visual.keycaps.fill => {
                            frames += 1;
                            assert!(regions.keys.expand(0.1).contains_rect(rect.rect));
                            assert!((rect.rect.center().y - row.center().y).abs() < 0.1);
                        }
                        egui::Shape::Rect(rect)
                            if rect.brush.as_ref().is_some_and(|brush| {
                                brush.fill_texture_id == egui::TextureId::User(42)
                            }) =>
                        {
                            icon += 1;
                            assert!(
                                regions
                                    .icon
                                    .expand(0.5 / scale + 0.05)
                                    .contains_rect(rect.rect)
                            );
                        }
                        egui::Shape::Mesh(mesh) if mesh.texture_id == egui::TextureId::User(42) => {
                            icon += 1;
                            assert!(
                                regions
                                    .icon
                                    .expand(0.5 / scale + 0.05)
                                    .contains_rect(mesh.calc_bounds())
                            );
                        }
                        egui::Shape::Text(text)
                            if text.galley.text().starts_with("A long label") =>
                        {
                            let visible = text.visual_bounding_rect().intersect(clipped.clip_rect);
                            assert!(!visible.intersects(regions.keys));
                            assert!(regions.label.expand(0.1).contains_rect(clipped.clip_rect));
                        }
                        _ => {}
                    }
                }
                assert_eq!(icon, 1, "custom texture icon");
                let expected = if measured_full.x <= ActionRow::shortcut_budget(width) {
                    primary_shortcut(binding).split_whitespace().count()
                } else {
                    1
                };
                assert_eq!(frames, expected, "{binding} width={width} scale={scale}");
            }
        }
    }
}

#[test]
fn scroll_ancestor_clips_partially_visible_rows_and_keycaps() {
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let mut viewport = Rect::NOTHING;
        let mut row_rects = Vec::new();
        let mut output = ctx.run_ui(input(), |ui| {
            egui::ScrollArea::vertical()
                .max_height(110.0)
                .vertical_scroll_offset(61.0)
                .show(ui, |ui| {
                    viewport = ui.clip_rect();
                    for i in 0..10 {
                        let label = format!("Action{i}");
                        row_rects.push(
                            ActionRow::new(presentation(&label, "Ctrl Minus / Alt M"), style())
                                .with_texture(Some(egui::TextureId::User(42)))
                                .show(ui, true)
                                .rect,
                        );
                    }
                });
        });
        output.textures_delta.clear();
        assert!(row_rects[0].bottom() < viewport.top());
        assert!(row_rects[9].top() > viewport.bottom());
        assert!(
            row_rects
                .iter()
                .any(|r| r.top() < viewport.top() && r.bottom() > viewport.top())
        );
        for shape in output.shapes {
            if let egui::Shape::Text(text) = shape.shape {
                let value = text.galley.text();
                if (value.starts_with("Action") || value == "Ctrl" || value == "Minus")
                    && shape.clip_rect.is_positive()
                {
                    assert!(viewport.contains_rect(shape.clip_rect));
                }
                if let Some(i) = value
                    .strip_prefix("Action")
                    .and_then(|s| s.parse::<usize>().ok())
                    && !row_rects[i].intersects(viewport)
                {
                    assert!(
                        !shape
                            .clip_rect
                            .intersect(text.visual_bounding_rect())
                            .is_positive()
                    );
                }
            }
        }
    }
}

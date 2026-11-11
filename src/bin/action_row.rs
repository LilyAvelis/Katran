//! Readonly action presentation. Owns layout and paint, never action execution.
use crate::ui_style::KeyCaps;
use eframe::egui::{self, Color32};

pub(super) struct ActionPresentation<'a> {
    pub label: &'a str,
    pub icon: egui_lucide::Lucide,
    pub primary_shortcut: &'a str,
    pub full_shortcut_tooltip: &'a str,
}
pub(super) struct ActionRow<'a>(ActionPresentation<'a>, Option<egui::TextureId>);
pub(super) fn popup_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
        .fill(crate::theme_ui::color(
            ctx,
            crate::theme_ui::Role::Surface,
            Color32::from_rgb(69, 69, 74),
        ))
        .stroke(egui::Stroke::new(
            1.0,
            crate::theme_ui::color(ctx, crate::theme_ui::Role::Border, Color32::from_gray(118)),
        ))
        .corner_radius(12)
        .inner_margin(12)
}
pub(super) fn primary_shortcut(shortcut: &str) -> &str {
    shortcut.split('/').next().unwrap_or("").trim()
}

struct Regions {
    icon: egui::Rect,
    label: egui::Rect,
    keys: egui::Rect,
}
impl<'a> ActionRow<'a> {
    pub fn new(presentation: ActionPresentation<'a>) -> Self {
        Self(presentation, None)
    }
    pub fn with_texture(mut self, texture: Option<egui::TextureId>) -> Self {
        self.1 = texture;
        self
    }
    fn shortcut_budget(row_width: f32) -> f32 {
        ((row_width - 42.0).max(0.0) * 0.45).max(10.0)
    }
    fn regions(row: egui::Rect, key_width: f32) -> Regions {
        let left = (row.left() + 33.0).min(row.right());
        let right = (row.right() - 9.0).max(left);
        let available = right - left;
        // Ordinary bindings retain their intrinsic width. Oversized user bindings
        // are ellipsized inside one bounded keycap; full text is a tooltip.
        let width = key_width.min(available);
        Regions {
            icon: egui::Rect::from_center_size(
                egui::pos2(row.left() + 17.0, row.center().y),
                egui::vec2(16.0, 16.0),
            ),
            label: egui::Rect::from_min_max(
                egui::pos2(left, row.top()),
                egui::pos2((right - width - 6.0).max(left), row.bottom()),
            ),
            keys: egui::Rect::from_min_max(
                egui::pos2(right - width, row.top()),
                egui::pos2(right, row.bottom()),
            ),
        }
    }
    pub fn show(self, ui: &mut egui::Ui, selected: bool) -> egui::Response {
        let caps = KeyCaps::measure_fitting(
            ui,
            self.0.primary_shortcut,
            Self::shortcut_budget(ui.available_width()),
        );
        let font = egui::TextStyle::Body.resolve(ui.style());
        let height = 38.0_f32
            .max(caps.size.y + 6.0)
            .max(ui.fonts_mut(|f| f.row_height(&font)) + 6.0);
        let (row, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click(),
        );
        let regions = Self::regions(row, caps.size.x);
        if selected || response.hovered() {
            ui.painter().rect_filled(
                row,
                7.0,
                crate::theme_ui::color(
                    ui.ctx(),
                    crate::theme_ui::Role::Selection,
                    Color32::from_rgb(100, 100, 108),
                ),
            );
        }
        if let Some(texture) = self.1 {
            egui::Image::new((texture, egui::vec2(16.0, 16.0))).paint_at(ui, regions.icon);
        } else {
            self.0.icon.size(16.0).image().paint_at(ui, regions.icon);
        }
        // Single-line truncation uses egui's actual font layout and same vertical anchor.
        let mut job = egui::text::LayoutJob::simple(
            self.0.label.to_owned(),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
            regions.label.width(),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter()
            .with_clip_rect(ui.clip_rect().intersect(regions.label))
            .galley(
                egui::pos2(regions.label.left(), row.center().y - galley.size().y / 2.0),
                galley,
                ui.visuals().text_color(),
            );
        caps.paint(ui, regions.keys);
        response.on_hover_text(self.0.full_shortcut_tooltip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(scale: f32) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(scale);
        crate::ui_style::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        ctx
    }
    #[test]
    fn production_catalogue_frames_and_text_fit_measured_regions() {
        assert_eq!(primary_shortcut("Ctrl Plus / Ctrl Equals"), "Ctrl Plus");
        for scale in [1.0, 1.5, 2.0] {
            for width in [180.0, 240.0, 420.0] {
                let ctx = context(scale);
                let mut cases: Vec<_> = katran::actions::DEFS
                    .iter()
                    .map(|d| {
                        (
                            d.id,
                            d.label,
                            crate::sorting_ui::sequence_hint(d.id).unwrap_or(d.default_binding),
                        )
                    })
                    .collect();
                cases.extend([
                    (
                        "copy",
                        "Длинная пользовательская подпись с дополнительным текстом",
                        "Ctrl Shift Alt Super VeryLongUserBinding / Ctrl C",
                    ),
                    ("quicklook", "Override", "Ctrl Shift Enter"),
                ]);
                for (id, label, shortcut) in cases {
                    let mut row = egui::Rect::NOTHING;
                    let mut regions = None;
                    let mut complete_keys_fit = false;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(600.0, 200.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            ui.set_width(width);
                            complete_keys_fit =
                                KeyCaps::measure(ui, primary_shortcut(shortcut)).size.x
                                    <= ActionRow::shortcut_budget(ui.available_width());
                            let caps = KeyCaps::measure_fitting(
                                ui,
                                primary_shortcut(shortcut),
                                ActionRow::shortcut_budget(width),
                            );
                            row = ActionRow::new(ActionPresentation {
                                label,
                                icon: crate::action_icons::icon(id),
                                primary_shortcut: primary_shortcut(shortcut),
                                full_shortcut_tooltip: shortcut,
                            })
                            .show(ui, false)
                            .rect;
                            regions = Some(ActionRow::regions(row, caps.size.x));
                        },
                    );
                    output.textures_delta.clear();
                    let regions = regions.unwrap();
                    let tolerance = 0.05;
                    let mut frames = 0;
                    let mut icons = 0;
                    for clipped in &output.shapes {
                        match &clipped.shape {
                            egui::Shape::Rect(rect) if rect.brush.is_some() => {
                                icons += 1;
                                assert!(
                                    regions
                                        .icon
                                        .expand(0.5 / scale + tolerance)
                                        .contains_rect(rect.rect)
                                );
                                assert!(
                                    (rect.rect.center().y - row.center().y).abs()
                                        < 0.5 / scale + tolerance
                                );
                            }
                            egui::Shape::Mesh(mesh) => {
                                icons += 1;
                                let bounds = mesh.calc_bounds();
                                assert!(regions.icon.expand(tolerance).contains_rect(bounds));
                                assert!((bounds.center().y - row.center().y).abs() < tolerance);
                            }
                            egui::Shape::Rect(rect)
                                if rect.fill == Color32::from_white_alpha(12) =>
                            {
                                frames += 1;
                                let stroke_bounds = match rect.stroke_kind {
                                    egui::StrokeKind::Inside => rect.rect,
                                    egui::StrokeKind::Middle => {
                                        rect.rect.expand(rect.stroke.width / 2.0)
                                    }
                                    egui::StrokeKind::Outside => {
                                        rect.rect.expand(rect.stroke.width)
                                    }
                                };
                                assert!(
                                    regions.keys.expand(tolerance).contains_rect(stroke_bounds),
                                    "{id} {shortcut} at {width}/{scale}: {stroke_bounds:?} outside {:?}",
                                    regions.keys
                                );
                                assert!((rect.rect.center().y - row.center().y).abs() < tolerance);
                                assert!(!rect.rect.intersects(regions.label));
                            }
                            egui::Shape::Text(text) => {
                                let bounds =
                                    egui::Rect::from_min_size(text.pos, text.galley.size());
                                assert!(
                                    (bounds.center().y - row.center().y).abs() < tolerance,
                                    "text anchor {id}: {bounds:?} / {row:?}"
                                );
                                let visible =
                                    text.visual_bounding_rect().intersect(clipped.clip_rect);
                                if clipped.clip_rect == regions.keys {
                                    assert!(regions.keys.expand(tolerance).contains_rect(bounds));
                                    assert!(!visible.intersects(regions.label));
                                } else {
                                    assert!(!visible.intersects(regions.keys));
                                }
                            }
                            _ => {}
                        }
                    }
                    assert_eq!(icons, 1, "real SVG icon must paint: {id}");
                    assert_eq!(frames > 0, !shortcut.is_empty(), "{id}");
                    if complete_keys_fit {
                        assert_eq!(
                            frames,
                            primary_shortcut(shortcut).split_whitespace().count(),
                            "fitting hint/binding must retain each complete keycap: {id}"
                        );
                    } else {
                        assert_eq!(
                            frames, 1,
                            "oversized hint/binding uses one bounded ellipsized keycap: {id}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn scrolled_rows_retain_ancestor_clip_for_partial_and_hidden_content() {
        for scale in [1.0, 1.5, 2.0] {
            let ctx = context(scale);
            for _ in 0..2 {
                let mut viewport = egui::Rect::NOTHING;
                let mut rows = Vec::new();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(420.0, 300.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.label("Header");
                        egui::ScrollArea::vertical()
                            .max_height(110.0)
                            .vertical_scroll_offset(61.0)
                            .show(ui, |ui| {
                                viewport = ui.clip_rect();
                                for index in 0..10 {
                                    rows.push(
                                        ActionRow::new(ActionPresentation {
                                            label: &format!("Action{index}"),
                                            icon: crate::action_icons::icon("copy"),
                                            primary_shortcut: "Ctrl Minus",
                                            full_shortcut_tooltip: "Ctrl Minus",
                                        })
                                        .show(ui, true)
                                        .rect,
                                    );
                                }
                            });
                        ui.label("Search");
                    },
                );
                output.textures_delta.clear();
                assert!(rows[0].bottom() < viewport.top());
                assert!(rows[9].top() > viewport.bottom());
                assert!(
                    rows.iter()
                        .any(|r| r.top() < viewport.top() && r.bottom() > viewport.top())
                );
                for clipped in output.shapes {
                    if let egui::Shape::Text(text) = clipped.shape {
                        let value = text.galley.text();
                        if (value == "Ctrl" || value == "Minus" || value.starts_with("Action"))
                            && clipped.clip_rect.is_positive()
                        {
                            assert!(viewport.contains_rect(clipped.clip_rect));
                        }
                        let Some(index) = value
                            .strip_prefix("Action")
                            .and_then(|s| s.parse::<usize>().ok())
                        else {
                            continue;
                        };
                        if !rows[index].intersects(viewport) {
                            assert!(
                                !clipped
                                    .clip_rect
                                    .intersect(text.visual_bounding_rect())
                                    .is_positive()
                            );
                        }
                    } else if let egui::Shape::Rect(rect) = clipped.shape
                        && (rect.fill == Color32::from_white_alpha(12)
                            || rect.fill == Color32::from_rgb(100, 100, 108))
                        && clipped.clip_rect.is_positive()
                    {
                        assert!(viewport.contains_rect(clipped.clip_rect));
                    }
                }
            }
        }
    }
}

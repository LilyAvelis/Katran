//! Shared compact strip for special shelf modes, styled like the footer.
use super::*;

pub(super) fn shortcut_hints(ui: &mut egui::Ui, hints: &[(&str, &str)]) {
    ui.separator();
    egui::Frame::new()
        .fill(theme_ui::footer_fill(
            ui.ctx(),
            Color32::from_rgb(54, 54, 58),
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            let width = ui.available_width();
            ui.set_min_width(width);
            ui.horizontal_wrapped(|ui| {
                for &(keys, description) in hints {
                    let caps = super::ui_style::KeyCaps::measure(ui, keys);
                    let color = ui.visuals().text_color();
                    let text_width = (width - caps.size.x - 5.0).max(1.0);
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        description.to_owned(),
                        egui::FontId::proportional(12.0),
                        color,
                    );
                    job.wrap.max_width = text_width;
                    job.wrap.max_rows = 1;
                    job.wrap.break_anywhere = true;
                    let label = ui.painter().layout_job(job);
                    let size = egui::vec2(
                        (caps.size.x + 5.0 + label.size().x).min(width),
                        caps.size.y.max(label.size().y),
                    );
                    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
                    let cap_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), rect.center().y - caps.size.y / 2.0),
                        caps.size,
                    );
                    caps.paint(ui, cap_rect);
                    let origin = egui::pos2(
                        cap_rect.right() + 5.0,
                        rect.center().y - label.size().y / 2.0,
                    );
                    ui.painter()
                        .with_clip_rect(rect)
                        .galley(origin, label, color);
                    response.on_hover_text(format!("{keys} · {description}"));
                }
            });
        });
}

pub(super) fn show<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let ctx = ui.ctx().clone();
    egui::Frame::new()
        .fill(theme_ui::footer_fill(&ctx, Color32::from_rgb(54, 54, 58)))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            let width = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                add,
            )
            .inner
        })
}

/// Flat footer-style action, keeping selection and hover feedback in theme tokens.
pub(super) fn action(
    ui: &mut egui::Ui,
    label: &str,
    icon: Option<egui_lucide::Lucide>,
    enabled: bool,
    active: bool,
    max_width: f32,
) -> egui::Response {
    let font = egui::FontId::proportional(12.0);
    let color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let icon_width = if icon.is_some() { 22.0 } else { 0.0 };
    let width = (galley.size().x + 20.0 + icon_width)
        .max(28.0)
        .min(max_width.max(28.0));
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 28.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled && (active || response.hovered() || response.has_focus()) {
        ui.painter().rect_filled(
            rect,
            4.0,
            theme_ui::color(ui.ctx(), theme_ui::Role::Selection, Color32::from_gray(78)),
        );
    }
    if let Some(icon) = icon {
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 16.0, rect.center().y),
            egui::vec2(16.0, 16.0),
        );
        icon.size(16.0).image().tint(color).paint_at(ui, icon_rect);
    }
    ui.painter()
        .with_clip_rect(rect.shrink2(egui::vec2(5.0, 0.0)))
        .galley(
            egui::pos2(
                rect.left() + 10.0 + icon_width,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
    response.on_hover_text(label)
}

pub(super) fn loading_dots(ui: &mut egui::Ui, label: &str, time: f64) {
    let font = egui::FontId::proportional(12.0);
    let color = ui.visuals().text_color();
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 48.0, 28.0),
        egui::Sense::hover(),
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
        galley.clone(),
        color,
    );
    for index in 0..3 {
        let phase = time * 8.0 - index as f64 * 0.75;
        let lift = ((phase.sin() + 1.0) * 2.0) as f32;
        ui.painter().circle_filled(
            egui::pos2(
                rect.left() + galley.size().x + 18.0 + index as f32 * 8.0,
                rect.center().y + 2.0 - lift,
            ),
            2.4,
            color,
        );
    }
}

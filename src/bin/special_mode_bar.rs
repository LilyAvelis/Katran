//! Shared compact strip for special shelf modes, styled like the footer.
use super::*;

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

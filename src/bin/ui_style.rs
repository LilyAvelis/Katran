use super::i18n::tr;
use crate::ui_foundation::tokens;
use eframe::egui::{self, Color32};

pub(super) fn install(ctx: &egui::Context) {
    crate::ui_fonts::install(ctx);
    let style = standard_style(ctx);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
    crate::theme_ui::reset(ctx);
}

pub(super) fn standard_style(ctx: &egui::Context) -> egui::Style {
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(39, 39, 42);
    visuals.window_fill = Color32::from_rgb(54, 54, 58);
    visuals.extreme_bg_color = Color32::from_rgb(32, 32, 35);
    visuals.faint_bg_color = Color32::from_rgb(49, 49, 53);
    visuals.override_text_color = Some(Color32::from_rgb(235, 235, 238));
    visuals.selection.bg_fill = Color32::from_rgb(91, 91, 99);
    visuals.selection.stroke.color = Color32::WHITE;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(tokens::CONTROL_RADIUS);
        widget.bg_stroke = egui::Stroke::NONE;
    }
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(61, 61, 66);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(79, 79, 85);
    visuals.widgets.active.bg_fill = Color32::from_rgb(95, 95, 103);
    visuals.window_corner_radius = egui::CornerRadius::same(tokens::WINDOW_RADIUS);
    style.visuals = visuals;
    style.spacing.interact_size.y = tokens::CONTROL_HEIGHT;
    style.spacing.item_spacing = egui::vec2(tokens::SM, tokens::SM);
    style.spacing.button_padding = egui::vec2(tokens::SM, tokens::SM);
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(tokens::BODY),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(tokens::BODY),
    );
    style
}

/// Katran supplies theme tokens; the shared component owns measurement and paint.
pub(super) fn keycaps_style(ui: &egui::Ui) -> avilon_egui::KeyCapsStyle {
    avilon_egui::KeyCapsStyle {
        font: egui::FontId::proportional(tokens::CAPTION),
        text_color: crate::theme_ui::color(
            ui.ctx(),
            crate::theme_ui::Role::Text,
            Color32::from_gray(220),
        ),
        fill: crate::theme_ui::color(
            ui.ctx(),
            crate::theme_ui::Role::Raised,
            Color32::from_white_alpha(12),
        ),
        stroke: egui::Stroke::new(
            1.0,
            crate::theme_ui::color(
                ui.ctx(),
                crate::theme_ui::Role::Border,
                Color32::from_white_alpha(55),
            ),
        ),
        corner_radius: tokens::XS as u8,
    }
}
pub(super) struct KeyCaps {
    inner: avilon_egui::KeyCaps,
    pub size: egui::Vec2,
}
impl KeyCaps {
    pub fn measure(ui: &egui::Ui, text: &str) -> Self {
        let inner = avilon_egui::KeyCaps::measure(ui, text, &keycaps_style(ui));
        Self {
            size: inner.size,
            inner,
        }
    }
    #[cfg(test)]
    pub fn measure_fitting(ui: &egui::Ui, text: &str, max_width: f32) -> Self {
        let inner = avilon_egui::KeyCaps::measure_fitting(ui, text, max_width, &keycaps_style(ui));
        Self {
            size: inner.size,
            inner,
        }
    }
    pub fn paint(&self, ui: &egui::Ui, region: egui::Rect) {
        self.inner.paint(ui, region, &keycaps_style(ui));
    }
}

pub(super) fn keycap(ui: &mut egui::Ui, text: &str) {
    let caps = KeyCaps::measure(ui, text);
    let (rect, _) = ui.allocate_exact_size(caps.size, egui::Sense::hover());
    caps.paint(ui, rect);
}

/// Flat, accessible segment in the shared bottom strip.
pub(super) fn status_icon(
    ui: &mut egui::Ui,
    icon: egui_lucide::Lucide,
    label: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            4.0,
            crate::theme_ui::color(
                ui.ctx(),
                crate::theme_ui::Role::Selection,
                Color32::from_gray(78),
            ),
        );
    }
    icon.size(16.0)
        .image()
        .tint(ui.visuals().text_color())
        .paint_at(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)),
        );
    response.on_hover_text(label)
}

pub(super) fn actions_button(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(116.0, 28.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            tr("Действия · Ctrl+K", "Actions · Ctrl+K"),
        )
    });
    if open || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            4.0,
            crate::theme_ui::color(
                ui.ctx(),
                crate::theme_ui::Role::Selection,
                Color32::from_gray(78),
            ),
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Actions  Ctrl K",
        egui::FontId::proportional(12.0),
        ui.visuals().text_color(),
    );
    response.on_hover_text(tr("Действия · Ctrl+K", "Actions · Ctrl+K"))
}

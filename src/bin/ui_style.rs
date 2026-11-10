use crate::ui_foundation::tokens;
use eframe::egui::{self, Color32};

pub(super) fn install(ctx: &egui::Context) {
    crate::ui_fonts::install(ctx);
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
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Measured once; the same galleys and frame geometry are used for painting.
pub(super) struct KeyCaps {
    keys: Vec<std::sync::Arc<egui::Galley>>,
    pub size: egui::Vec2,
}
impl KeyCaps {
    const PAD: egui::Vec2 = egui::vec2(5.0, 2.0);
    const GAP: f32 = 3.0;
    pub fn measure(ui: &egui::Ui, text: &str) -> Self {
        let keys: Vec<_> = text
            .split_whitespace()
            .map(|key| {
                ui.painter().layout_no_wrap(
                    key.to_owned(),
                    egui::FontId::proportional(tokens::CAPTION),
                    Color32::from_gray(220),
                )
            })
            .collect();
        let width = keys
            .iter()
            .map(|g| g.size().x + 2.0 * Self::PAD.x)
            .sum::<f32>()
            + keys.len().saturating_sub(1) as f32 * Self::GAP;
        let height = keys
            .iter()
            .map(|g| g.size().y + 2.0 * Self::PAD.y)
            .fold(0.0, f32::max);
        Self {
            keys,
            size: egui::vec2(width, height),
        }
    }
    pub fn measure_fitting(ui: &egui::Ui, text: &str, max_width: f32) -> Self {
        let measured = Self::measure(ui, text);
        if measured.size.x <= max_width {
            return measured;
        }
        let mut job = egui::text::LayoutJob::simple(
            text.to_owned(),
            egui::FontId::proportional(tokens::CAPTION),
            Color32::from_gray(220),
            (max_width - 2.0 * Self::PAD.x).max(0.0),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let size = galley.size() + 2.0 * Self::PAD;
        Self {
            keys: vec![galley],
            size,
        }
    }
    pub fn paint(&self, ui: &egui::Ui, region: egui::Rect) {
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(region));
        let mut x = region.right() - self.size.x;
        for galley in &self.keys {
            let size = galley.size() + 2.0 * Self::PAD;
            let rect =
                egui::Rect::from_min_size(egui::pos2(x, region.center().y - size.y / 2.0), size);
            painter.rect(
                rect,
                tokens::XS as u8,
                Color32::from_white_alpha(12),
                egui::Stroke::new(1.0, Color32::from_white_alpha(55)),
                egui::StrokeKind::Inside,
            );
            painter.galley(
                rect.center() - galley.size() / 2.0,
                galley.clone(),
                Color32::from_gray(220),
            );
            x += size.x + Self::GAP;
        }
    }
}

pub(super) fn keycap(ui: &mut egui::Ui, text: &str) {
    let caps = KeyCaps::measure(ui, text);
    let (rect, _) = ui.allocate_exact_size(caps.size, egui::Sense::hover());
    caps.paint(ui, rect);
}

pub(super) fn actions_button(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(140.0, tokens::CONTROL_HEIGHT),
        egui::Sense::click(),
    );
    ui.painter().rect_filled(
        rect,
        9.0,
        Color32::from_gray(if open || response.hovered() { 100 } else { 78 }),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(11.0, 3.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.label(egui::RichText::new("Actions").strong().size(14.0));
            keycap(ui, "Ctrl K");
        },
    );
    response.on_hover_text("Действия · Ctrl+K")
}

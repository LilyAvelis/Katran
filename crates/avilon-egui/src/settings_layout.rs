//! Responsive settings rows shared by desktop clients. Units are logical points.
use egui::{self, Color32};

pub mod tokens {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const CONTROL_HEIGHT: f32 = 32.0;
    pub const CONTROL_COLUMN: f32 = 360.0;
    pub const LABEL_MIN: f32 = 200.0;
    pub const BODY: f32 = 14.0;
    pub const CAPTION: f32 = 12.0;
    pub const CONTROL_RADIUS: u8 = 6;
    pub const SECTION_RADIUS: u8 = 8;
    pub const WINDOW_RADIUS: u8 = 12;
}

/// Host-owned appearance; no theme preference or persistence is stored here.
#[derive(Clone, Copy, Debug)]
pub struct SectionStyle {
    pub fill: Color32,
    pub radius: u8,
    pub padding: i8,
}
pub struct SettingsSection;
impl SettingsSection {
    pub fn show(ui: &mut egui::Ui, style: SectionStyle, content: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
            .fill(style.fill)
            .corner_radius(style.radius)
            .inner_margin(style.padding)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::vec2(tokens::SM, tokens::SM);
                content(ui);
            });
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RowGeometry {
    pub labels: egui::Rect,
    pub controls: egui::Rect,
    #[cfg_attr(not(test), allow(dead_code))] // Read by geometry regression assertions.
    pub stacked: bool,
}

pub struct SettingsControl;
impl SettingsControl {
    pub fn show(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) -> egui::Rect {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.spacing_mut().interact_size.y = tokens::CONTROL_HEIGHT;
        ui.spacing_mut().button_padding = egui::vec2(tokens::SM, tokens::SM);
        ui.spacing_mut().text_edit_width = ui.available_width().min(240.0);
        ui.spacing_mut().slider_width = (ui.available_width() - 100.0).clamp(48.0, 150.0);
        ui.horizontal_wrapped(|ui| {
            ui.set_min_height(tokens::CONTROL_HEIGHT);
            content(ui);
        })
        .response
        .rect
    }
}

pub struct SettingsRow;
impl SettingsRow {
    pub fn show(
        ui: &mut egui::Ui,
        title: &str,
        description: &str,
        controls: impl FnOnce(&mut egui::Ui),
    ) -> RowGeometry {
        let width = ui.available_width();
        let stacked = width < tokens::CONTROL_COLUMN + tokens::LABEL_MIN + tokens::LG;
        let labels = |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
            ui.add(egui::Label::new(egui::RichText::new(title).strong()).wrap());
            if !description.is_empty() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(description)
                            .weak()
                            .size(tokens::CAPTION),
                    )
                    .wrap(),
                );
            }
        };
        let mut result = RowGeometry {
            labels: egui::Rect::NOTHING,
            controls: egui::Rect::NOTHING,
            stacked,
        };
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(tokens::LG, tokens::SM);
            if stacked {
                result.labels = ui.vertical(labels).response.rect;
                result.controls = ui
                    .allocate_ui_with_layout(
                        egui::vec2(width, tokens::CONTROL_HEIGHT),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(ui.available_width());
                            SettingsControl::show(ui, controls);
                        },
                    )
                    .response
                    .rect;
            } else {
                ui.horizontal_top(|ui| {
                    result.labels = ui
                        .allocate_ui_with_layout(
                            egui::vec2(
                                width - tokens::CONTROL_COLUMN - tokens::LG,
                                tokens::CONTROL_HEIGHT,
                            ),
                            egui::Layout::top_down(egui::Align::Min),
                            labels,
                        )
                        .response
                        .rect;
                    result.controls = ui
                        .allocate_ui_with_layout(
                            egui::vec2(tokens::CONTROL_COLUMN, tokens::CONTROL_HEIGHT),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_min_width(ui.available_width());
                                SettingsControl::show(ui, controls);
                            },
                        )
                        .response
                        .rect;
                });
            }
        });
        result
    }
}

#![allow(dead_code)]
//! Shared geometry for settings and command lists. All dimensions are logical points.
use eframe::egui::{self, Color32};

pub(super) mod tokens {
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

pub(super) struct SettingsSection;
impl SettingsSection {
    pub fn show(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
            .fill(Color32::from_gray(79))
            .corner_radius(tokens::SECTION_RADIUS)
            .inner_margin(tokens::LG as i8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::vec2(tokens::SM, tokens::SM);
                content(ui);
            });
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RowGeometry {
    pub labels: egui::Rect,
    pub controls: egui::Rect,
    #[cfg_attr(not(test), allow(dead_code))] // Read by geometry regression assertions.
    pub stacked: bool,
}

pub(super) struct SettingsControl;
impl SettingsControl {
    pub fn show(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) -> egui::Rect {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.spacing_mut().interact_size.y = tokens::CONTROL_HEIGHT;
        ui.spacing_mut().button_padding = egui::vec2(tokens::SM, tokens::SM);
        ui.spacing_mut().text_edit_width = ui.available_width().min(240.0);
        ui.horizontal_wrapped(|ui| {
            ui.set_min_height(tokens::CONTROL_HEIGHT);
            content(ui);
        })
        .response
        .rect
    }
}

pub(super) struct SettingsRow;
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

/// Command lists share the label/description/control ownership and narrow behavior.
pub(super) fn command_row(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    actions: impl FnOnce(&mut egui::Ui),
) {
    SettingsRow::show(ui, title, description, actions);
}

pub(super) fn message(ui: &mut egui::Ui, text: &str, error: bool) {
    ui.add_space(tokens::SM);
    ui.add(
        egui::Label::new(if error {
            egui::RichText::new(text).color(Color32::LIGHT_RED)
        } else {
            egui::RichText::new(text).weak()
        })
        .wrap(),
    );
}

//! Katran style adapter for the reusable Avilon settings layout.
#![allow(dead_code)]
pub(super) use avilon_egui::settings_layout::{SettingsRow, tokens};
use eframe::egui::{self, Color32};
pub(super) struct SettingsSection;
impl SettingsSection {
    /// A named group keeps related preferences in one raised panel.
    pub fn group(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
        ui.add_space(tokens::SM);
        ui.label(egui::RichText::new(title).strong());
        ui.add_space(tokens::XS);
        Self::show(ui, content);
        ui.add_space(tokens::SM);
    }

    pub fn show(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
        let style = avilon_egui::settings_layout::SectionStyle {
            fill: crate::theme_ui::color(
                ui.ctx(),
                crate::theme_ui::Role::Raised,
                Color32::from_gray(79),
            ),
            radius: tokens::SECTION_RADIUS,
            padding: tokens::LG as i8,
        };
        avilon_egui::settings_layout::SettingsSection::show(ui, style, content);
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

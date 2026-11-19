//! Katran style adapter for the reusable Avilon settings layout.
#![allow(dead_code)]
pub(super) use avilon_egui::settings_layout::{SettingsRow, tokens};
use eframe::egui::{self, Color32, RichText};

/// Shared geometry for single-line fields; callers retain their IDs and behavior.
pub(super) fn text_field(edit: egui::TextEdit<'_>) -> egui::TextEdit<'_> {
    edit.min_size(egui::vec2(0.0, 36.0))
        .vertical_align(egui::Align::Center)
        .margin(egui::Margin::symmetric(10, 7))
}

/// Apply the shared slider appearance without changing surrounding widgets.
pub(super) fn slider_scope<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().slider_rail_height = 5.0;
        let visuals = ui.visuals_mut();
        visuals.handle_shape = egui::style::HandleShape::Circle;
        visuals.slider_trailing_fill = true;
        let selection = visuals.selection.stroke.color.to_opaque();
        for face in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active] {
            face.bg_stroke = egui::Stroke::new(1.0, selection);
        }
        content(ui)
    }).inner
}

pub(super) fn slider(ui: &mut egui::Ui, enabled: bool, slider: egui::Slider<'_>) -> egui::Response {
    slider_scope(ui, |ui| ui.add_enabled(enabled, slider))
}

pub(super) struct SettingsSection;
impl SettingsSection {
    pub fn set_depth(ctx: &egui::Context, enabled: bool) {
        let id = egui::Id::new(("settings-section-depth", ctx.viewport_id()));
        ctx.data_mut(|data| data.insert_temp(id, enabled));
    }

    fn depth(ui: &egui::Ui) -> bool {
        let id = egui::Id::new(("settings-section-depth", ui.ctx().viewport_id()));
        ui.data(|data| data.get_temp::<bool>(id).unwrap_or(false))
    }

    /// A named group keeps related preferences in one raised panel.
    pub fn group(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
        ui.add_space(tokens::SM);
        if Self::depth(ui) {
            ui.label(
                RichText::new(title)
                    .size(17.0)
                    .family(egui::FontFamily::Name(crate::ui_fonts::BOLD_FAMILY.into())),
            );
            section_rule(ui);
        } else {
            ui.label(RichText::new(title).strong());
        }
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
        if Self::depth(ui) {
            let frame = egui::Frame::new()
                .fill(style.fill)
                .corner_radius(style.radius)
                .inner_margin(style.padding)
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(27)))
                .shadow(egui::Shadow {
                    offset: [0, 4],
                    blur: 12,
                    spread: 0,
                    color: Color32::from_black_alpha(85),
                });
            avilon_egui::settings_layout::SettingsSection::show_with_frame(ui, frame, content);
        } else {
            avilon_egui::settings_layout::SettingsSection::show(ui, style, content);
        }
    }
}

fn section_rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 2.0), egui::Sense::hover());
    let accent = crate::theme_ui::color(
        ui.ctx(),
        crate::theme_ui::Role::Accent,
        ui.visuals().selection.stroke.color,
    );
    let mut mesh = egui::Mesh::default();
    for (x, color) in [
        (rect.left(), accent.linear_multiply(0.85)),
        (
            rect.left() + rect.width() * 0.28,
            Color32::from_white_alpha(48),
        ),
        (rect.right(), Color32::TRANSPARENT),
    ] {
        mesh.colored_vertex(egui::pos2(x, rect.top()), color);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), color);
    }
    for left in [0, 2] {
        mesh.add_triangle(left, left + 2, left + 3);
        mesh.add_triangle(left, left + 3, left + 1);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
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

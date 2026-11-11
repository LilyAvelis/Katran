//! Presentation-only theme tokens. Standard always returns each caller's legacy color.
use eframe::egui::{self, Color32};
use katran::theme::{Preset, ThemePreferences};

#[derive(Clone, Copy)]
pub(super) enum Role {
    Background,
    Surface,
    Raised,
    Border,
    Text,
    Muted,
    Accent,
    Selection,
}
fn key() -> egui::Id {
    egui::Id::new("katran-theme-preferences")
}
fn current(ctx: &egui::Context) -> ThemePreferences {
    ctx.data(|data| data.get_temp::<ThemePreferences>(key()))
        .unwrap_or_default()
}
pub(super) fn reset(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<ThemePreferences>(key()));
}
pub(super) fn color(ctx: &egui::Context, role: Role, legacy: Color32) -> Color32 {
    let preference = current(ctx);
    if preference.preset == Preset::Standard {
        return legacy;
    }
    match role {
        Role::Background => Color32::from_rgb(11, 20, 35),
        Role::Surface => Color32::from_rgb(16, 30, 50),
        Role::Raised => Color32::from_rgb(23, 43, 70),
        Role::Border => Color32::from_rgb(52, 87, 121),
        Role::Text => Color32::from_rgb(230, 241, 255),
        Role::Muted => Color32::from_rgb(155, 176, 200),
        Role::Accent => {
            let [r, g, b] = preference.accent.unwrap_or([107, 174, 255]);
            Color32::from_rgb(r, g, b)
        }
        Role::Selection => Color32::from_rgb(41, 75, 112),
    }
}
pub(super) fn apply(ctx: &egui::Context, preference: &ThemePreferences) {
    if ctx
        .data(|data| data.get_temp::<ThemePreferences>(key()))
        .as_ref()
        == Some(preference)
    {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(key(), preference.clone()));
    let mut style = crate::ui_style::standard_style(ctx);
    if preference.preset == Preset::BlueExperimental {
        let get = |role| color(ctx, role, Color32::TRANSPARENT);
        let visuals = &mut style.visuals;
        visuals.panel_fill = get(Role::Background);
        visuals.window_fill = get(Role::Surface);
        visuals.extreme_bg_color = Color32::from_rgb(7, 15, 27);
        visuals.faint_bg_color = get(Role::Surface);
        visuals.override_text_color = Some(get(Role::Text));
        visuals.selection.bg_fill = get(Role::Selection);
        visuals.selection.stroke.color = get(Role::Accent);
        visuals.widgets.inactive.bg_fill = get(Role::Raised);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 65, 96);
        visuals.widgets.active.bg_fill = get(Role::Selection);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, get(Role::Border));
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, get(Role::Accent));
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, get(Role::Accent));
    }
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}
/// Decorative arcs occupy existing bounds and never affect layout.
pub(super) fn decorate(ui: &egui::Ui, rect: egui::Rect) {
    let theme = current(ui.ctx());
    if theme.preset != Preset::BlueExperimental
        || !theme.decoration
        || rect.width() <= 0.0
        || rect.height() <= 0.0
    {
        return;
    }
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    let accent = color(ui.ctx(), Role::Accent, Color32::TRANSPARENT).gamma_multiply(0.22);
    let center = egui::pos2(
        rect.right() - rect.width() * 0.07,
        rect.bottom() + rect.height() * 0.8,
    );
    for factor in [0.60_f32, 0.82, 1.04] {
        let radius = rect.width().min(500.0) * factor;
        let points = (0..=24)
            .map(|i| {
                let angle = std::f32::consts::PI * (1.05 + 0.4 * i as f32 / 24.0);
                center + egui::vec2(angle.cos(), angle.sin()) * radius
            })
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, accent)));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoration_is_opt_in_and_clipped_to_assigned_rectangle() {
        let ctx = egui::Context::default();
        crate::ui_style::install(&ctx);
        let bounds = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(300.0, 100.0));
        let mut standard = ctx.run_ui(egui::RawInput::default(), |ui| decorate(ui, bounds));
        let standard_shapes = standard.shapes.len();
        standard.textures_delta.clear();
        assert_eq!(standard_shapes, 0);
        apply(
            &ctx,
            &ThemePreferences {
                preset: Preset::BlueExperimental,
                accent: None,
                decoration: true,
            },
        );
        let mut blue = ctx.run_ui(egui::RawInput::default(), |ui| decorate(ui, bounds));
        let count = blue.shapes.len();
        let clipped = blue
            .shapes
            .iter()
            .all(|shape| bounds.contains_rect(shape.clip_rect));
        blue.textures_delta.clear();
        assert_eq!(count, 3);
        assert!(clipped);
    }
    #[test]
    fn standard_is_exact_and_roundtrip_restores_visuals() {
        let ctx = egui::Context::default();
        crate::ui_style::install(&ctx);
        let original = ctx.style_of(egui::Theme::Dark);
        let legacy = Color32::from_rgb(23, 45, 67);
        for role in [
            Role::Background,
            Role::Surface,
            Role::Raised,
            Role::Border,
            Role::Text,
            Role::Muted,
            Role::Accent,
            Role::Selection,
        ] {
            assert_eq!(color(&ctx, role, legacy), legacy);
        }
        let blue = ThemePreferences {
            preset: Preset::BlueExperimental,
            accent: Some([18, 90, 180]),
            decoration: true,
        };
        apply(&ctx, &blue);
        assert_eq!(
            color(&ctx, Role::Accent, legacy),
            Color32::from_rgb(18, 90, 180)
        );
        assert_ne!(
            ctx.style_of(egui::Theme::Dark).visuals.panel_fill,
            original.visuals.panel_fill
        );
        apply(&ctx, &blue);
        apply(&ctx, &ThemePreferences::default());
        let restored = ctx.style_of(egui::Theme::Dark);
        assert_eq!(restored.visuals.panel_fill, original.visuals.panel_fill);
        assert_eq!(
            restored.visuals.selection.bg_fill,
            original.visuals.selection.bg_fill
        );
        assert_eq!(
            restored.visuals.widgets.inactive.bg_fill,
            original.visuals.widgets.inactive.bg_fill
        );
        assert_eq!(
            restored.spacing.interact_size,
            original.spacing.interact_size
        );
        assert_eq!(color(&ctx, Role::Accent, legacy), legacy);
    }
}

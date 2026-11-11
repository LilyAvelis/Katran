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
fn selection_color(preference: &ThemePreferences) -> Color32 {
    let accent = preference.accent.unwrap_or([107, 174, 255]);
    let surface = [16u8, 30, 50];
    let blend = |index: usize| {
        ((u16::from(surface[index]) * 78 + u16::from(accent[index]) * 22 + 50) / 100) as u8
    };
    Color32::from_rgb(blend(0), blend(1), blend(2))
}
pub(super) fn color(ctx: &egui::Context, role: Role, legacy: Color32) -> Color32 {
    let preference = current(ctx);
    if preference.preset == Preset::Standard {
        return legacy;
    }
    match role {
        Role::Background if preference.lily_background.is_some() => Color32::TRANSPARENT,
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
        Role::Selection => selection_color(&preference),
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
        visuals.hyperlink_color = get(Role::Accent);
        let normal = get(Role::Raised);
        let hover = Color32::from_rgb(36, 65, 96);
        let selection = get(Role::Selection);
        let surface = get(Role::Surface);
        let text = get(Role::Text);
        let border = get(Role::Border);
        let accent = get(Role::Accent);
        for (widget, fill, stroke) in [
            (&mut visuals.widgets.noninteractive, surface, border),
            (&mut visuals.widgets.inactive, normal, border),
            (&mut visuals.widgets.hovered, hover, accent),
            (&mut visuals.widgets.active, selection, accent),
            (&mut visuals.widgets.open, selection, accent),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = egui::Stroke::new(1.0, stroke);
            widget.fg_stroke.color = text;
        }
    }
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}
/// Embedded owner-provided artwork, decoded once per selected variant.
pub(super) fn background(ctx: &egui::Context, rect: egui::Rect) {
    let theme = current(ctx);
    if theme.preset != Preset::BlueExperimental { return; }
    let Some(index) = theme.lily_background else { return; };
    let bytes: &[u8] = match index {
        1 => include_bytes!("../../assets/themes/lilies/lily-1.png"),
        2 => include_bytes!("../../assets/themes/lilies/lily-2.png"),
        3 => include_bytes!("../../assets/themes/lilies/lily-3.png"),
        4 => include_bytes!("../../assets/themes/lilies/lily-4.png"),
        5 => include_bytes!("../../assets/themes/lilies/lily-5.png"),
        _ => return,
    };
    let id = egui::Id::new(("lily-background", index));
    let texture = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)).or_else(|| {
        let rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let texture = ctx.load_texture(format!("lily-{index}"), egui::ColorImage::from_rgba_unmultiplied(size,rgba.as_raw()), egui::TextureOptions::LINEAR);
        ctx.data_mut(|data| data.insert_temp(id,texture.clone()));
        Some(texture)
    });
    if let Some(texture) = texture {
        let painter = ctx.layer_painter(egui::LayerId::background());
        // Contain the whole illustration; navy fills any letterboxing.
        painter.rect_filled(rect,0.0,Color32::from_rgb(11,20,35));
        let size = texture.size_vec2();
        let scale = (rect.width()/size.x).min(rect.height()/size.y);
        let dest = egui::Rect::from_center_size(rect.center(),size*scale);
        painter.image(texture.id(),dest,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
    }
}
/// Decorative arcs occupy existing bounds and never affect layout.
pub(super) fn decorate(ui: &egui::Ui, rect: egui::Rect) {
    let theme = current(ui.ctx());
    if theme.preset != Preset::BlueExperimental
        || !theme.decoration
        || theme.lily_background.is_some()
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
                decoration: true, lily_background:None,
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
    fn blue_widget_fills_and_selection_follow_accent_without_changing_standard() {
        let ctx = egui::Context::default();
        crate::ui_style::install(&ctx);
        let standard = ctx.style_of(egui::Theme::Dark);
        let first = ThemePreferences {
            preset: Preset::BlueExperimental,
            accent: Some([220, 50, 80]),
            decoration: false, lily_background:None,
        };
        apply(&ctx, &first);
        let blue = ctx.style_of(egui::Theme::Dark);
        for widget in [
            &blue.visuals.widgets.noninteractive,
            &blue.visuals.widgets.inactive,
            &blue.visuals.widgets.hovered,
            &blue.visuals.widgets.active,
            &blue.visuals.widgets.open,
        ] {
            assert_eq!(widget.weak_bg_fill, widget.bg_fill);
            assert_ne!(widget.weak_bg_fill, Color32::from_gray(60));
        }
        assert_ne!(
            blue.visuals.widgets.inactive.weak_bg_fill,
            standard.visuals.widgets.inactive.weak_bg_fill
        );
        let first_selection = blue.visuals.selection.bg_fill;
        assert_eq!(
            first_selection,
            color(&ctx, Role::Selection, Color32::TRANSPARENT)
        );
        let second = ThemePreferences {
            accent: Some([40, 190, 100]),
            ..first
        };
        apply(&ctx, &second);
        let green = ctx.style_of(egui::Theme::Dark);
        assert_ne!(green.visuals.selection.bg_fill, first_selection);
        assert_eq!(
            green.visuals.selection.stroke.color,
            Color32::from_rgb(40, 190, 100)
        );
        assert_eq!(
            green.visuals.widgets.active.weak_bg_fill,
            green.visuals.selection.bg_fill
        );
        assert_eq!(
            green.visuals.widgets.open.weak_bg_fill,
            green.visuals.selection.bg_fill
        );
        apply(&ctx, &ThemePreferences::default());
        let restored = ctx.style_of(egui::Theme::Dark);
        assert_eq!(
            restored.visuals.widgets.inactive,
            standard.visuals.widgets.inactive
        );
        assert_eq!(restored.visuals.widgets.open, standard.visuals.widgets.open);
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
            decoration: true, lily_background:None,
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

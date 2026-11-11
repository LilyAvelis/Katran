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
pub(super) fn compact_header(ctx: &egui::Context) -> bool {
    let theme = current(ctx);
    theme.preset == Preset::BlueExperimental && theme.compact_header
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
    let glass = preference.glass
        && ctx
            .data(|d| d.get_temp::<bool>(egui::Id::new("native-glass-active")))
            .unwrap_or(false);
    let base = match role {
        Role::Background
            if (preference.lily_background.is_some() || preference.custom_background.is_some()) =>
        {
            Color32::TRANSPARENT
        }
        Role::Background
            if preference.glass
                && ctx
                    .data(|d| d.get_temp::<bool>(egui::Id::new("native-glass-active")))
                    .unwrap_or(false) =>
        {
            Color32::from_rgba_unmultiplied(11, 20, 35, 175)
        }
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
    };
    if glass && matches!(role, Role::Surface | Role::Raised | Role::Selection) {
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 200)
    } else {
        base
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
/// Crop rather than letterbox: artwork reaches all window edges without stretching.
fn cover_uv(view: egui::Vec2, image: egui::Vec2) -> egui::Rect {
    let scale = (view.x / image.x).max(view.y / image.y);
    let visible = egui::vec2(view.x / (image.x * scale), view.y / (image.y * scale))
        .min(egui::Vec2::splat(1.0));
    egui::Rect::from_center_size(egui::pos2(0.5, 0.5), visible)
}
pub(super) fn footer_fill(ctx: &egui::Context, legacy: Color32) -> Color32 {
    let theme = current(ctx);
    if theme.preset == Preset::BlueExperimental
        && (theme.lily_background.is_some() || theme.custom_background.is_some())
    {
        Color32::TRANSPARENT
    } else {
        color(ctx, Role::Surface, legacy)
    }
}
fn custom_key() -> egui::Id {
    egui::Id::new("active-custom-background")
}
pub(super) fn custom_cached(ctx: &egui::Context, id: &str) -> bool {
    ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(custom_key()))
        .is_some_and(|(key, _)| key == id)
        || ctx
            .data(|d| d.get_temp::<String>(egui::Id::new("custom-background-failed")))
            .as_deref()
            == Some(id)
}
pub(super) fn load_custom(
    ctx: &egui::Context,
    id: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let result = (|| {
        let rgba = image::open(path).map_err(|e| e.to_string())?.into_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let texture = ctx.load_texture(
            format!("custom-{id}"),
            egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|d| d.insert_temp(custom_key(), (id.to_owned(), texture)));
        Ok(())
    })();
    if result.is_err() {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("custom-background-failed"), id.to_owned()));
    }
    result
}
fn paint_background(
    ctx: &egui::Context,
    rect: egui::Rect,
    texture: &egui::TextureHandle,
    glass: bool,
) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    if !glass {
        painter.rect_filled(rect, 0.0, Color32::from_rgb(11, 20, 35));
    }
    painter.image(
        texture.id(),
        rect,
        cover_uv(rect.size(), texture.size_vec2()),
        if glass {
            Color32::from_white_alpha(160)
        } else {
            Color32::WHITE
        },
    );
}
/// Embedded owner-provided artwork, decoded once per selected variant.
pub(super) fn background(ctx: &egui::Context, rect: egui::Rect) {
    let theme = current(ctx);
    if theme.preset != Preset::BlueExperimental {
        return;
    }
    let glass = theme.glass
        && ctx
            .data(|d| d.get_temp::<bool>(egui::Id::new("native-glass-active")))
            .unwrap_or(false);
    if let Some(id) = &theme.custom_background {
        if let Some((cached, texture)) =
            ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(custom_key()))
            && cached == *id
        {
            paint_background(ctx, rect, &texture, glass);
        }
        return;
    }
    let Some(index) = theme.lily_background else {
        return;
    };
    let bytes: &[u8] = match index {
        1 => include_bytes!("../../assets/themes/lilies/lily-1.png"),
        2 => include_bytes!("../../assets/themes/lilies/lily-2.png"),
        3 => include_bytes!("../../assets/themes/lilies/lily-3.png"),
        4 => include_bytes!("../../assets/themes/lilies/lily-4.png"),
        5 => include_bytes!("../../assets/themes/lilies/lily-5.png"),
        _ => return,
    };
    let id = egui::Id::new(("lily-background", index));
    let texture = ctx
        .data(|data| data.get_temp::<egui::TextureHandle>(id))
        .or_else(|| {
            let rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let texture = ctx.load_texture(
                format!("lily-{index}"),
                egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                egui::TextureOptions::LINEAR,
            );
            ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
            Some(texture)
        });
    if let Some(texture) = texture {
        paint_background(ctx, rect, &texture, glass);
    }
}
/// Decorative arcs occupy existing bounds and never affect layout.
pub(super) fn decorate(ui: &egui::Ui, rect: egui::Rect) {
    let theme = current(ui.ctx());
    if theme.preset != Preset::BlueExperimental
        || !theme.decoration
        || (theme.lily_background.is_some() || theme.custom_background.is_some())
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
    fn cover_has_no_bars_and_glass_translucency_keeps_text_opaque() {
        for view in [
            egui::vec2(940.0, 680.0),
            egui::vec2(1645.0, 1190.0),
            egui::vec2(600.0, 1000.0),
        ] {
            let image = egui::vec2(1672.0, 941.0);
            let uv = cover_uv(view, image);
            assert!(uv.min.x >= 0.0 && uv.min.y >= 0.0 && uv.max.x <= 1.0 && uv.max.y <= 1.0);
            let aspect = (uv.width() * image.x) / (uv.height() * image.y);
            assert!((aspect - view.x / view.y).abs() < 0.001);
        }
        let ctx = egui::Context::default();
        crate::ui_style::install(&ctx);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("native-glass-active"), true));
        apply(
            &ctx,
            &ThemePreferences {
                preset: Preset::BlueExperimental,
                glass: true,
                lily_background: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(footer_fill(&ctx, Color32::GRAY), Color32::TRANSPARENT);
        assert!(color(&ctx, Role::Surface, Color32::BLACK).a() < 255);
        assert_eq!(color(&ctx, Role::Text, Color32::WHITE).a(), 255);
    }
    #[test]
    fn glass_falls_back_opaque_until_native_activation_and_lily_texture_is_reused() {
        let ctx = egui::Context::default();
        crate::ui_style::install(&ctx);
        let theme = ThemePreferences {
            preset: Preset::BlueExperimental,
            glass: true,
            ..Default::default()
        };
        apply(&ctx, &theme);
        assert_eq!(color(&ctx, Role::Background, Color32::BLACK).a(), 255);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("native-glass-active"), true));
        assert!(color(&ctx, Role::Background, Color32::BLACK).a() < 255);
        let theme = ThemePreferences {
            preset: Preset::BlueExperimental,
            lily_background: Some(1),
            ..Default::default()
        };
        apply(&ctx, &theme);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(940.0, 680.0));
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| background(ui.ctx(), rect));
            assert!(!out.shapes.is_empty());
            out.textures_delta.clear();
        }
        assert!(ctx.data(|d|d.get_temp::<egui::TextureHandle>(egui::Id::new(("lily-background",1u8)))).is_some());
    }
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
                lily_background: None,
                custom_background: None,
                glass: false,
                compact_header: false,
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
            decoration: false,
            lily_background: None,
            custom_background: None,
            glass: false,
            compact_header: false,
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
            decoration: true,
            lily_background: None,
            custom_background: None,
            glass: false,
            compact_header: false,
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

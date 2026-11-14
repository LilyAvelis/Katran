//! Presentation-only theme tokens. Standard always returns each caller's legacy color.
use eframe::egui::{self, Color32};
use katran::theme::{Preset, ThemePreferences};

pub(super) use avilon_egui::theme::Role;
fn key() -> egui::Id {
    egui::Id::new("katran-theme-preferences")
}
fn current(ctx: &egui::Context) -> ThemePreferences {
    ctx.data(|data| data.get_temp::<ThemePreferences>(key()))
        .unwrap_or_default()
}
pub(super) fn is_blue(ctx: &egui::Context) -> bool {
    current(ctx).preset == Preset::BlueExperimental
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
    let surface =
        avilon_egui::theme::tint(Color32::from_rgb(16, 30, 50), preference.accent).to_array();
    let blend = |index: usize| {
        ((u16::from(surface[index]) * 78 + u16::from(accent[index]) * 22 + 50) / 100) as u8
    };
    Color32::from_rgb(blend(0), blend(1), blend(2))
}
fn selection_rgb(preference: &ThemePreferences) -> Option<[u8; 3]> {
    preference.selection.or_else(|| {
        (preference.preset == Preset::BlueExperimental)
            .then_some(preference.accent.unwrap_or([107, 174, 255]))
    })
}
pub(super) fn color(ctx: &egui::Context, role: Role, legacy: Color32) -> Color32 {
    let preference = current(ctx);
    avilon_egui::theme::color(
        role,
        legacy,
        avilon_egui::theme::PaletteOptions {
            blue: preference.preset == Preset::BlueExperimental,
            accent: preference.accent,
            selection: preference.selection,
            selection_intensity: preference.selection_intensity,
            glass_active: preference.glass
                && ctx
                    .data(|d| d.get_temp::<bool>(egui::Id::new("native-glass-active")))
                    .unwrap_or(false),
            has_background: preference.lily_background.is_some()
                || preference.custom_background.is_some()
                || preference.active_video().is_some(),
        },
    )
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
        visuals.extreme_bg_color =
            avilon_egui::theme::tint(Color32::from_rgb(7, 15, 27), preference.accent);
        visuals.faint_bg_color = get(Role::Surface);
        visuals.override_text_color = Some(get(Role::Text));
        visuals.selection.bg_fill = get(Role::Selection);
        visuals.selection.stroke.color = get(Role::SelectionOutline);
        visuals.hyperlink_color = get(Role::Accent);
        let normal = get(Role::Raised);
        let hover = avilon_egui::theme::tint(Color32::from_rgb(36, 65, 96), preference.accent);
        let selection = selection_color(preference);
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
    if preference.preset == Preset::Standard && preference.selection.is_some() {
        style.visuals.selection.bg_fill =
            color(ctx, Role::Selection, style.visuals.selection.bg_fill);
        style.visuals.selection.stroke.color = color(
            ctx,
            Role::SelectionOutline,
            style.visuals.selection.stroke.color,
        );
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
        && (theme.lily_background.is_some()
            || theme.custom_background.is_some()
            || theme.active_video().is_some())
    {
        Color32::TRANSPARENT
    } else {
        color(ctx, Role::Surface, legacy)
    }
}
fn video_key() -> egui::Id {
    egui::Id::new("active-video-background")
}
/// Publish the latest worker frame, or clear it when video playback stops.
pub(super) fn set_video_texture(
    ctx: &egui::Context,
    id: &str,
    texture: Option<egui::TextureHandle>,
) {
    if texture.is_some() && current(ctx).active_video() != Some(id) {
        return;
    }
    ctx.data_mut(|data| {
        if let Some(texture) = texture {
            data.insert_temp(video_key(), (id.to_owned(), texture));
        } else if data
            .get_temp::<(String, egui::TextureHandle)>(video_key())
            .is_some_and(|(cached, _)| cached == id)
        {
            data.remove::<(String, egui::TextureHandle)>(video_key());
        }
    });
}
fn video_texture(ctx: &egui::Context, id: &str) -> Option<egui::TextureHandle> {
    ctx.data(|data| data.get_temp::<(String, egui::TextureHandle)>(video_key()))
        .filter(|(cached, _)| cached == id)
        .map(|(_, texture)| texture)
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
    if let Some(id) = theme.active_video() {
        if let Some(texture) = video_texture(ctx, id) {
            paint_background(ctx, rect, &texture, glass);
        }
        return;
    }
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
        || (theme.lily_background.is_some()
            || theme.custom_background.is_some()
            || theme.active_video().is_some())
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
                optical_strength: 0,
                ..Default::default()
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
            optical_strength: 0,
            ..Default::default()
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
    fn independent_selection_changes_fill_and_outline_without_recoloring_buttons() {
        let ctx = egui::Context::default();
        crate::ui_style::install(&ctx);
        let blue = ThemePreferences {
            preset: Preset::BlueExperimental,
            accent: Some([230, 20, 40]),
            ..Default::default()
        };
        apply(&ctx, &blue);
        let button = ctx.style_of(egui::Theme::Dark).visuals.widgets.active;
        let changed = ThemePreferences {
            selection: Some([20, 230, 70]),
            selection_intensity: 80,
            ..blue.clone()
        };
        apply(&ctx, &changed);
        assert_eq!(
            color(&ctx, Role::Accent, Color32::BLACK),
            Color32::from_rgb(230, 20, 40)
        );
        assert_eq!(
            color(&ctx, Role::SelectionOutline, Color32::BLACK),
            Color32::from_rgb(20, 230, 70)
        );
        let fill = color(&ctx, Role::Selection, Color32::BLACK);
        assert!(fill.g() > fill.r());
        assert_eq!(
            ctx.style_of(egui::Theme::Dark).visuals.widgets.active,
            button
        );
        apply(
            &ctx,
            &ThemePreferences {
                selection_intensity: 0,
                ..changed
            },
        );
        assert_eq!(
            color(&ctx, Role::Selection, Color32::BLACK),
            color(&ctx, Role::Surface, Color32::BLACK)
        );
        assert_eq!(
            color(&ctx, Role::SelectionOutline, Color32::BLACK),
            Color32::from_rgb(20, 230, 70)
        );
        let legacy = Color32::from_gray(74);
        apply(&ctx, &ThemePreferences::default());
        assert_eq!(color(&ctx, Role::Selection, legacy), legacy);
        apply(
            &ctx,
            &ThemePreferences {
                selection: Some([20, 230, 70]),
                ..Default::default()
            },
        );
        assert_ne!(color(&ctx, Role::Selection, legacy), legacy);
        assert_eq!(
            color(&ctx, Role::SelectionOutline, legacy),
            Color32::from_rgb(20, 230, 70)
        );
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
            Role::SelectionOutline,
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
            optical_strength: 0,
            ..Default::default()
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

/// Samples only the application's selected background, never other windows.
pub(super) fn optical_shape(
    ctx: &egui::Context,
    rect: egui::Rect,
    radius: f32,
) -> Option<egui::Shape> {
    optical_shape_for(ctx, rect, radius, false)
}
fn optical_shape_for(
    ctx: &egui::Context,
    rect: egui::Rect,
    radius: f32,
    selected: bool,
) -> Option<egui::Shape> {
    let theme = current(ctx);
    if theme.preset != Preset::BlueExperimental
        || theme.optical_strength == 0
        || !ctx
            .data(|d| d.get_temp::<bool>(egui::Id::new("optical-gpu-ready")))
            .unwrap_or(false)
    {
        return None;
    }
    let texture = if let Some(id) = theme.active_video() {
        video_texture(ctx, id)
    } else if let Some(id) = &theme.custom_background {
        ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(custom_key()))
            .filter(|(cached, _)| cached == id)
            .map(|(_, t)| t)
    } else {
        theme.lily_background.and_then(|i| {
            ctx.data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new(("lily-background", i))))
        })
    }?;
    let background = ctx.content_rect();
    Some(crate::optical_gpu::shape(
        rect,
        texture.id(),
        background,
        cover_uv(background.size(), texture.size_vec2()),
        radius,
        f32::from(theme.optical_strength) / 100.0,
        crate::optical_gpu::Material {
            adjustments: [
                theme.optical_refraction,
                theme.optical_blur,
                theme.optical_highlight,
                theme.optical_chromatic,
            ]
            .map(|n| f32::from(n) / 100.0),
            selection_tint: if selected {
                selection_rgb(&theme).map(|[r, g, b]| {
                    [
                        f32::from(r) / 255.0,
                        f32::from(g) / 255.0,
                        f32::from(b) / 255.0,
                        f32::from(theme.selection_intensity.min(100)) / 100.0,
                    ]
                })
            } else {
                None
            },
        },
    ))
}
pub(super) fn paint_selected_optical(ui: &egui::Ui, rect: egui::Rect, radius: f32, selected: bool) {
    if let Some(shape) = optical_shape_for(ui.ctx(), rect, radius, selected) {
        ui.painter().add(shape);
    }
}
#[cfg(test)]
mod optical_tests {
    use super::*;
    #[test]
    fn video_frames_are_scoped_to_selected_id_and_feed_the_optical_background() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(180.0, 100.0));
        apply(
            &ctx,
            &ThemePreferences {
                preset: Preset::BlueExperimental,
                video_background: Some("video-a".into()),
                optical_strength: 50,
                ..Default::default()
            },
        );
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("optical-gpu-ready"), true));
        let frame = ctx.load_texture(
            "video-test",
            egui::ColorImage::filled([2, 2], Color32::RED),
            egui::TextureOptions::LINEAR,
        );
        set_video_texture(&ctx, "video-a", Some(frame));
        let mut rendered = false;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| {
                assert_eq!(
                    color(ui.ctx(), Role::Background, Color32::BLACK),
                    Color32::TRANSPARENT
                );
                assert_eq!(footer_fill(ui.ctx(), Color32::BLACK), Color32::TRANSPARENT);
                background(ui.ctx(), rect);
                rendered = matches!(
                    optical_shape(ui.ctx(), rect, 8.0),
                    Some(egui::Shape::Callback(_))
                );
            },
        );
        output.textures_delta.clear();
        assert!(rendered);
        assert!(video_texture(&ctx, "video-a").is_some());
        apply(
            &ctx,
            &ThemePreferences {
                preset: Preset::BlueExperimental,
                video_background: Some("video-b".into()),
                optical_strength: 50,
                ..Default::default()
            },
        );
        let stale = ctx.load_texture(
            "stale",
            egui::ColorImage::filled([2, 2], Color32::BLUE),
            egui::TextureOptions::LINEAR,
        );
        set_video_texture(&ctx, "video-a", Some(stale));
        assert!(video_texture(&ctx, "video-b").is_none());
        let current = ctx.load_texture(
            "current",
            egui::ColorImage::filled([2, 2], Color32::GREEN),
            egui::TextureOptions::LINEAR,
        );
        set_video_texture(&ctx, "video-b", Some(current));
        set_video_texture(&ctx, "video-a", None);
        assert!(video_texture(&ctx, "video-b").is_some());
        set_video_texture(&ctx, "video-b", None);
        assert!(video_texture(&ctx, "video-b").is_none());
    }
    #[test]
    fn disabled_video_retains_id_but_does_not_paint_or_drive_optics() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(180.0, 100.0));
        let theme = ThemePreferences {
            preset: Preset::BlueExperimental,
            video_background: Some("saved-video".into()),
            video_enabled: Some(true),
            optical_strength: 50,
            ..Default::default()
        };
        apply(&ctx, &theme);
        let texture = ctx.load_texture(
            "disabled-video",
            egui::ColorImage::filled([2, 2], Color32::RED),
            egui::TextureOptions::LINEAR,
        );
        let texture_id = texture.id();
        set_video_texture(&ctx, "saved-video", Some(texture));
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("optical-gpu-ready"), true));
        apply(
            &ctx,
            &ThemePreferences {
                video_enabled: Some(false),
                ..theme.clone()
            },
        );
        let mut optical = true;
        let mut background_color = Color32::TRANSPARENT;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| {
                background(ui.ctx(), rect);
                background_color = color(ui.ctx(), Role::Background, Color32::BLACK);
                optical = optical_shape(ui.ctx(), rect, 8.0).is_some();
            },
        );
        let painted = output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Rect(rect) => rect
                .brush
                .as_ref()
                .is_some_and(|brush| brush.fill_texture_id == texture_id),
            egui::Shape::Mesh(mesh) => mesh.texture_id == texture_id,
            _ => false,
        });
        output.textures_delta.clear();
        assert!(!painted);
        assert!(!optical);
        assert_ne!(background_color, Color32::TRANSPARENT);
        assert!(video_texture(&ctx, "saved-video").is_some());
        apply(&ctx, &theme);
        let mut optical = false;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| {
                background(ui.ctx(), rect);
                optical = optical_shape(ui.ctx(), rect, 8.0).is_some();
            },
        );
        output.textures_delta.clear();
        assert!(optical);
    }
    #[test]
    fn optical_layer_requires_opt_in_source_and_gpu_host() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(180.0, 100.0));
        assert!(optical_shape(&ctx, rect, 8.0).is_none());
        apply(
            &ctx,
            &ThemePreferences {
                preset: Preset::BlueExperimental,
                lily_background: Some(1),
                optical_strength: 50,
                ..Default::default()
            },
        );
        background(&ctx, rect);
        assert!(optical_shape(&ctx, rect, 8.0).is_none());
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("optical-gpu-ready"), true));
        assert!(matches!(
            optical_shape(&ctx, rect, 8.0),
            Some(egui::Shape::Callback(_))
        ));
        apply(
            &ctx,
            &ThemePreferences {
                optical_strength: 50,
                ..Default::default()
            },
        );
        assert!(optical_shape(&ctx, rect, 8.0).is_none());
    }
}

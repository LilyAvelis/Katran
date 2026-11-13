//! Pure semantic palette; the host owns preferences, style application and storage.
use egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Background,
    Surface,
    Raised,
    Border,
    Text,
    Muted,
    Accent,
    Selection,
    SelectionOutline,
}

/// Inputs selected by the host. `glass_active` means glass is both requested and active.
#[derive(Clone, Copy, Debug, Default)]
pub struct PaletteOptions {
    pub blue: bool,
    pub accent: Option<[u8; 3]>,
    pub selection: Option<[u8; 3]>,
    pub selection_intensity: u8,
    pub glass_active: bool,
    pub has_background: bool,
}

/// Recolor the blue palette while retaining its brightness, contrast and alpha.
/// No custom accent preserves the existing blue palette exactly.
pub fn tint(base: Color32, accent: Option<[u8; 3]>) -> Color32 {
    let Some([r, g, b]) = accent else {
        return base;
    };
    let chosen = egui::ecolor::Hsva::from(Color32::from_rgb(r, g, b));
    let original = egui::ecolor::Hsva::from(Color32::from_rgb(107, 174, 255));
    let mut result = egui::ecolor::Hsva::from(base);
    result.h = (result.h + chosen.h - original.h).rem_euclid(1.0);
    result.s *= chosen.s;
    Color32::from(result)
}

fn selection_fill(surface: Color32, rgb: [u8; 3], intensity: u8) -> Color32 {
    let weight = (u32::from(intensity.min(100)) * 22).min(3500);
    let blend = |base: u8, selected: u8| {
        ((u32::from(base) * (3500 - weight) + u32::from(selected) * weight + 1750) / 3500) as u8
    };
    Color32::from_rgba_unmultiplied(
        blend(surface.r(), rgb[0]),
        blend(surface.g(), rgb[1]),
        blend(surface.b(), rgb[2]),
        surface.a(),
    )
}

/// Resolve one semantic role. Standard mode preserves legacy colors except for an explicit selection override.
pub fn color(role: Role, legacy: Color32, options: PaletteOptions) -> Color32 {
    if !options.blue {
        return match (role, options.selection) {
            (Role::Selection, Some(rgb)) => {
                selection_fill(legacy, rgb, options.selection_intensity)
            }
            (Role::SelectionOutline, Some([r, g, b])) => Color32::from_rgb(r, g, b),
            _ => legacy,
        };
    }
    let accent = options.accent.unwrap_or([107, 174, 255]);
    let selected = options.selection.unwrap_or(accent);
    let palette = |base| tint(base, options.accent);
    let base = match role {
        Role::Background if options.has_background => Color32::TRANSPARENT,
        Role::Background if options.glass_active => {
            palette(Color32::from_rgba_unmultiplied(11, 20, 35, 175))
        }
        Role::Background => palette(Color32::from_rgb(11, 20, 35)),
        Role::Surface => palette(Color32::from_rgb(16, 30, 50)),
        Role::Raised => palette(Color32::from_rgb(23, 43, 70)),
        Role::Border => palette(Color32::from_rgb(52, 87, 121)),
        Role::Text => palette(Color32::from_rgb(230, 241, 255)),
        Role::Muted => palette(Color32::from_rgb(155, 176, 200)),
        Role::Accent => Color32::from_rgb(accent[0], accent[1], accent[2]),
        Role::Selection => selection_fill(
            palette(Color32::from_rgb(16, 30, 50)),
            selected,
            options.selection_intensity,
        ),
        Role::SelectionOutline => Color32::from_rgb(selected[0], selected[1], selected[2]),
    };
    if options.glass_active && matches!(role, Role::Surface | Role::Raised | Role::Selection) {
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 200)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_accent_tints_surfaces_but_preserves_glass_and_selection_override() {
        let legacy = Color32::from_gray(80);
        let orange = PaletteOptions {
            blue: true,
            accent: Some([255, 140, 40]),
            selection: Some([40, 220, 100]),
            glass_active: true,
            has_background: true,
            ..Default::default()
        };
        let surface = color(Role::Surface, legacy, orange);
        let raised = color(Role::Raised, legacy, orange);
        assert!(
            surface.r() > surface.b(),
            "surface should be warm: {surface:?}"
        );
        assert!(
            raised.r() > raised.b(),
            "raised surface should be warm: {raised:?}"
        );
        assert_eq!(surface.a(), 200);
        assert_eq!(
            color(Role::Background, legacy, orange),
            Color32::TRANSPARENT
        );
        assert_eq!(
            color(Role::SelectionOutline, legacy, orange),
            Color32::from_rgb(40, 220, 100)
        );
        assert_eq!(color(Role::Text, legacy, orange).a(), 255);
        assert_eq!(
            color(Role::Surface, legacy, PaletteOptions::default()),
            legacy
        );
        let blue = PaletteOptions {
            blue: true,
            ..Default::default()
        };
        assert_eq!(
            color(Role::Surface, legacy, blue),
            Color32::from_rgb(16, 30, 50)
        );
    }
    #[test]
    fn host_options_preserve_legacy_and_blue_glass_selection() {
        let legacy = Color32::from_rgba_unmultiplied(40, 60, 80, 120);
        assert_eq!(
            color(Role::Background, legacy, PaletteOptions::default()),
            legacy
        );
        assert_eq!(
            color(Role::Selection, legacy, PaletteOptions::default()),
            legacy
        );
        let blue = PaletteOptions {
            blue: true,
            selection: Some([200, 100, 0]),
            selection_intensity: 50,
            glass_active: true,
            ..Default::default()
        };
        assert_eq!(
            color(Role::Selection, legacy, blue),
            Color32::from_rgba_unmultiplied(74, 52, 34, 200)
        );
        assert_eq!(
            color(Role::Background, legacy, blue),
            Color32::from_rgba_unmultiplied(11, 20, 35, 175)
        );
        assert_eq!(
            color(
                Role::Background,
                legacy,
                PaletteOptions {
                    has_background: true,
                    ..blue
                }
            ),
            Color32::TRANSPARENT
        );
    }
}

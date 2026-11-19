//! Explicit Windows fallback, shared by every UI surface. No system installation.
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use katran::settings::UiFont;
use std::sync::{Arc, OnceLock};

pub(crate) const BOLD_FAMILY: &str = "katran-bold";

pub(super) fn install(ctx: &egui::Context) {
    apply(ctx, UiFont::Default);
}

const WINDOWS_FACES: [(UiFont, &str, &str, &str); 6] = [
    (UiFont::SegoeUi, "Segoe UI", "segoeui.ttf", "segoeuib.ttf"),
    (UiFont::Tahoma, "Tahoma", "tahoma.ttf", "tahomabd.ttf"),
    (UiFont::Verdana, "Verdana", "verdana.ttf", "verdanab.ttf"),
    (UiFont::Arial, "Arial", "arial.ttf", "arialbd.ttf"),
    (UiFont::Calibri, "Calibri", "calibri.ttf", "calibrib.ttf"),
    (
        UiFont::Trebuchet,
        "Trebuchet MS",
        "trebuc.ttf",
        "trebucbd.ttf",
    ),
];

struct InstalledFace {
    choice: UiFont,
    label: &'static str,
    regular: Arc<FontData>,
    bold: Arc<FontData>,
}

fn installed_faces() -> &'static [InstalledFace] {
    static FACES: OnceLock<Vec<InstalledFace>> = OnceLock::new();
    FACES.get_or_init(|| {
        let Some(windows) = std::env::var_os("WINDIR") else {
            return Vec::new();
        };
        let directory = std::path::PathBuf::from(windows).join("Fonts");
        WINDOWS_FACES
            .iter()
            .filter_map(|&(choice, label, regular, bold)| {
                let read = |name| {
                    std::fs::read(directory.join(name))
                        .ok()
                        .map(|bytes| Arc::new(FontData::from_owned(bytes)))
                };
                Some(InstalledFace {
                    choice,
                    label,
                    regular: read(regular)?,
                    bold: read(bold)?,
                })
            })
            .collect()
    })
}

pub(super) fn available() -> impl Iterator<Item = (UiFont, &'static str)> {
    installed_faces()
        .iter()
        .map(|face| (face.choice, face.label))
}

pub(super) fn apply(ctx: &egui::Context, choice: UiFont) {
    let id = egui::Id::new("katran-ui-font");
    if ctx.data(|data| data.get_temp::<UiFont>(id)) == Some(choice) {
        return;
    }
    static FONTS: OnceLock<FontDefinitions> = OnceLock::new();
    let mut fonts = FONTS.get_or_init(definitions).clone();
    if choice != UiFont::Default
        && let Some(face) = installed_faces().iter().find(|face| face.choice == choice)
    {
        for (family, key, data) in [
            (FontFamily::Proportional, "katran-ui-regular", &face.regular),
            (
                FontFamily::Name(BOLD_FAMILY.into()),
                "katran-ui-bold",
                &face.bold,
            ),
        ] {
            fonts.font_data.insert(key.into(), data.clone());
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, key.into());
        }
    }
    ctx.set_fonts(fonts);
    ctx.data_mut(|data| data.insert_temp(id, choice));
    ctx.request_repaint_of(egui::ViewportId::ROOT);
    ctx.request_repaint_of(egui::ViewportId::from_hash_of("katran-settings"));
}

fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut bold = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        let directory = std::path::PathBuf::from(windows).join("Fonts");
        if let Ok(bytes) = std::fs::read(directory.join("segoeuib.ttf")) {
            fonts
                .font_data
                .insert("segoeuib.ttf".into(), Arc::new(FontData::from_owned(bytes)));
            bold.push("segoeuib.ttf".into());
        }
        // Keep the current text face; these are used only for missing glyphs.
        // Same explicit fallback pattern as Afio, without its terminal Nerd Font.
        for filename in [
            "segoeui.ttf",
            "seguisym.ttf",
            "seguiemj.ttf",
            "msyh.ttc",
            "malgun.ttf",
        ] {
            if let Ok(bytes) = std::fs::read(directory.join(filename)) {
                fonts
                    .font_data
                    .insert(filename.into(), Arc::new(FontData::from_owned(bytes)));
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .push(filename.into());
                }
            }
        }
    }
    bold.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(BOLD_FAMILY.into()), bold);
    fonts
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn live_font_switch_updates_regular_and_bold_and_restores_original() {
        let Some((face, _)) = available().next() else {
            return;
        };
        let ctx = egui::Context::default();
        let family_order = || {
            let mut order = None;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                order = Some(ui.fonts(|fonts| {
                    let families = &fonts.definitions().families;
                    let bold = FontFamily::Name(BOLD_FAMILY.into());
                    (
                        families[&FontFamily::Proportional].clone(),
                        families[&bold].clone(),
                        families[&FontFamily::Monospace].clone(),
                    )
                }));
            });
            output.textures_delta.clear();
            order.unwrap()
        };
        install(&ctx);
        let original = family_order();
        apply(&ctx, face);
        let selected = family_order();
        assert_eq!(selected.0[0], "katran-ui-regular");
        assert_eq!(selected.1[0], "katran-ui-bold");
        assert_eq!(selected.2, original.2);
        apply(&ctx, UiFont::Default);
        assert_eq!(family_order(), original);
    }

    #[test]
    fn ui_symbols_render_real_glyphs_in_both_text_families() {
        let ctx = egui::Context::default();
        install(&ctx);
        ctx.begin_pass(egui::RawInput::default());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let font = egui::FontId::new(14.0, family);
            for symbol in ['←', '→', '↑', '↓', '↵', '⌫', '⇧', 'Ф', '…', '🦐'] {
                // Compare atlas geometry with a missing glyph: epaint 0.36's
                // has_glyph may reject real glyphs sharing the fallback face.
                let (glyph, missing) = ctx.fonts_mut(|fonts| {
                    let actual = fonts.layout_no_wrap(
                        symbol.to_string(),
                        font.clone(),
                        egui::Color32::WHITE,
                    );
                    let missing = fonts.layout_no_wrap(
                        "\u{10ffff}".into(),
                        font.clone(),
                        egui::Color32::WHITE,
                    );
                    (
                        actual.rows[0].glyphs[0].uv_rect,
                        missing.rows[0].glyphs[0].uv_rect,
                    )
                });
                assert!(
                    glyph.min != missing.min || glyph.max != missing.max,
                    "missing {symbol} in {font:?}"
                );
            }
        }
        ctx.end_pass().textures_delta.clear();
    }
}

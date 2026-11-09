//! Explicit Windows fallback, shared by every UI surface. No system installation.
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::{Arc, OnceLock};

pub(super) fn install(ctx: &egui::Context) {
    static FONTS: OnceLock<FontDefinitions> = OnceLock::new();
    ctx.set_fonts(FONTS.get_or_init(definitions).clone());
}

fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    if let Some(windows) = std::env::var_os("WINDIR") {
        let directory = std::path::PathBuf::from(windows).join("Fonts");
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
    fonts
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

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

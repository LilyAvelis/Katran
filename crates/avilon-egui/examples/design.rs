//! Headless second host: no Katran profile, native window or artwork is needed.
use avilon_egui::settings_layout::{SectionStyle, SettingsRow, SettingsSection};
use egui::{Color32, Context, Pos2, RawInput, Rect, Vec2};

fn main() {
    let ctx = Context::default();
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0))),
            ..Default::default()
        },
        |ui| {
            SettingsSection::show(
                ui,
                SectionStyle {
                    fill: Color32::from_rgb(18, 30, 48),
                    radius: 8,
                    padding: 16,
                },
                |ui| {
                    SettingsRow::show(
                        ui,
                        "Example application",
                        "The host supplies style and executes intents.",
                        |ui| {
                            let _ = ui.button("Apply");
                        },
                    );
                },
            );
        },
    );
    println!(
        "Generated {} shapes without opening a window",
        output.shapes.len()
    );
    output.textures_delta.clear();
}

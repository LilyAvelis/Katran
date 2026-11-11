use avilon_egui::settings_layout::{SectionStyle, SettingsRow, SettingsSection};
use egui::{Color32, Context, Pos2, RawInput, Rect, Vec2};

#[test]
fn settings_rows_stack_and_keep_controls_separate_at_host_widths() {
    for width in [260.0, 880.0] {
        let ctx = Context::default();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            SettingsSection::show(
                ui,
                SectionStyle {
                    fill: Color32::from_rgb(20, 30, 40),
                    radius: 8,
                    padding: 16,
                },
                |ui| {
                    let row = SettingsRow::show(
                        ui,
                        "Host setting",
                        "A description that wraps on narrow windows.",
                        |ui| {
                            let _ = ui.button("First control");
                            let _ = ui.button("Second control");
                        },
                    );
                    assert_eq!(row.stacked, width < 600.0);
                    assert!(row.labels.is_finite() && row.controls.is_finite());
                    assert!(!row.labels.intersects(row.controls));
                    if row.stacked {
                        assert!(row.controls.top() >= row.labels.bottom());
                    }
                },
            );
        });
        output.textures_delta.clear();
    }
}

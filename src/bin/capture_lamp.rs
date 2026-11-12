//! Read-only presentation of `Command::Status`'s `capture_enabled` value.
use super::i18n::tr;
use eframe::egui::{self, Color32, Sense, Shape, Vec2, WidgetInfo, WidgetType};

const DIAMETER: f32 = 10.0;

fn color(enabled: bool) -> Color32 {
    if enabled {
        Color32::from_rgb(64, 190, 112)
    } else {
        Color32::from_rgb(224, 82, 82)
    }
}

fn description(enabled: bool) -> &'static str {
    if enabled {
        tr("Захват включён", "Capture enabled")
    } else {
        tr("Захват на паузе", "Capture paused")
    }
}

/// Show a passive capture indicator; all state remains owned by the domain API.
pub fn show(ui: &mut egui::Ui, enabled: bool) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(DIAMETER), Sense::hover());
    let label = description(enabled);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, label));
    response.on_hover_text(label);
    ui.painter().add(Shape::circle_filled(
        rect.center(),
        DIAMETER / 2.0,
        color(enabled),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_lamp_has_ten_pixel_geometry_and_status_color() {
        for enabled in [true, false] {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui, enabled));
            });
            let circle = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Circle(circle) => Some(circle),
                    _ => None,
                })
                .expect("lamp paints a circle");
            assert_eq!(circle.radius * 2.0, 10.0);
            assert_eq!(circle.fill, color(enabled));
            assert_eq!(circle.fill.a(), 255);
            if enabled {
                assert!(circle.fill.g() > circle.fill.r());
            } else {
                assert!(circle.fill.r() > circle.fill.g());
            }
            output.textures_delta.clear();
        }
    }
}

//! Tab presentation emits dispatcher commands; durable order belongs to Settings.
use eframe::egui;
use katran::{Command, interaction::ShelfTab};
use serde_json::Value;

pub fn show(
    ui: &mut egui::Ui,
    order: &[ShelfTab],
    active: &Value,
    enabled: bool,
) -> Option<Command> {
    let mut intent = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(enabled, egui::Button::new("All").selected(active == "all"))
            .clicked()
        {
            intent = Some(Command::SetTab { tab: ShelfTab::All });
        }
        ui.separator();
        for (index, &tab) in order.iter().enumerate() {
            let (label, value) = match tab {
                ShelfTab::All => continue,
                ShelfTab::Images => ("Images", "images"),
                ShelfTab::Screenshots => ("Screenshots", "screenshots"),
                ShelfTab::Downloads => ("Downloads", "downloads"),
            };
            let response = ui.add_enabled(
                enabled,
                egui::Button::new(format!("{} {label}", index + 1))
                    .selected(active == value)
                    .sense(egui::Sense::click_and_drag()),
            );
            response.dnd_set_drag_payload(tab);
            if enabled
                && let Some(payload) = response.dnd_hover_payload::<ShelfTab>()
                && *payload != tab
            {
                ui.painter().rect_stroke(
                    response.rect.expand(2.0),
                    3.0,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Outside,
                );
            }
            if enabled
                && let Some(payload) = response.dnd_release_payload::<ShelfTab>()
                && *payload != tab
            {
                intent = Some(Command::MoveTab {
                    tab: *payload,
                    index,
                });
            } else if response.clicked() {
                intent = Some(Command::SetTab { tab });
            }
        }
    });
    intent
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mouse_drop_emits_move_and_api_persists_order() {
        let root = tempfile::tempdir().unwrap();
        let mut api = katran::Service::open(root.path(), 10000).unwrap();
        let ctx = egui::Context::default();
        let order = katran::tab_layout::default_order();
        let mut targets = Vec::new();
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 100.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw(), |ui| {
            ui.horizontal(|ui| {
                let _ = ui.button("All");
                ui.separator();
                for (i, label) in ["Images", "Screenshots", "Downloads"].iter().enumerate() {
                    targets.push(ui.button(format!("{} {label}", i + 1)).rect.center());
                }
            });
        });
        output.textures_delta.clear();
        egui::DragAndDrop::set_payload(&ctx, ShelfTab::Downloads);
        let mut input = raw();
        input.events = vec![
            egui::Event::PointerMoved(targets[0]),
            egui::Event::PointerButton {
                pos: targets[0],
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let mut command = None;
        let mut output = ctx.run_ui(input, |ui| {
            command = show(ui, &order, &Value::String("all".into()), true);
        });
        output.textures_delta.clear();
        let command = command.expect("drop emits dispatcher intent");
        assert!(matches!(
            command,
            Command::MoveTab {
                tab: ShelfTab::Downloads,
                index: 0
            }
        ));
        let reply = api.execute(katran::Request {
            api_version: 1,
            request_id: "drop".into(),
            command,
        });
        assert_eq!(reply["ok"], true);
        drop(api);
        let mut api = katran::Service::open(root.path(), 10000).unwrap();
        let reply = api.execute(katran::Request {
            api_version: 1,
            request_id: "read".into(),
            command: Command::GetSettings,
        });
        assert_eq!(reply["result"]["tab_order"][0], "downloads");
    }
}

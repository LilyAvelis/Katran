//! Tab presentation emits dispatcher commands; durable order belongs to Settings.
use eframe::egui;
use katran::{Command, interaction::ShelfTab};
use serde_json::Value;

// The indicator follows the current measured rectangle, not a tab index. Reusing
// the current position on each frame naturally redirects an interrupted motion.
#[derive(Clone, Default)]
struct Indicator {
    rect: Option<egui::Rect>,
    active: String,
}

impl Indicator {
    fn update(&mut self, active: &str, target: Option<egui::Rect>, dt: f32, duration: f32) -> bool {
        let Some(target) = target else {
            self.rect = None;
            self.active = active.to_owned();
            return false;
        };
        let Some(current) = self.rect else {
            self.rect = Some(target);
            self.active = active.to_owned();
            return false;
        };
        // Exponential interpolation keeps retargeting continuous and bounded.
        let factor = if duration <= 0.0 {
            1.0
        } else {
            1.0 - (-dt.max(0.0) * 5.0 / duration).exp()
        };
        let next = egui::Rect::from_min_max(
            current.min + (target.min - current.min) * factor,
            current.max + (target.max - current.max) * factor,
        );
        let moving = (next.min - target.min).length_sq() > 0.25
            || (next.max - target.max).length_sq() > 0.25;
        self.rect = Some(if moving { next } else { target });
        self.active = active.to_owned();
        moving
    }
}

pub fn show(
    ui: &mut egui::Ui,
    order: &[ShelfTab],
    active: &Value,
    enabled: bool,
) -> Option<Command> {
    let mut intent = None;
    let active = active.as_str().unwrap_or("");
    let indicator_id = egui::Id::new("category-strip-indicator");
    let mut indicator = ui
        .ctx()
        .data(|d| d.get_temp::<Indicator>(indicator_id))
        .unwrap_or_default();
    let active_changed = indicator.active != active;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new("Folder").selected(active == "folder"),
            )
            .clicked()
        {
            intent = Some(Command::SetTab { tab: ShelfTab::All });
        }
        ui.separator();
        egui::ScrollArea::horizontal()
            .id_salt("category-strip-scroll")
            .auto_shrink([true, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let surface_slot = ui.painter().add(egui::Shape::Noop);
                    let indicator_slot = ui.painter().add(egui::Shape::Noop);
                    let mut surface: Option<egui::Rect> = None;
                    let mut target = None;
                    for (index, &tab) in order.iter().enumerate() {
                        let (label, value) = match tab {
                            ShelfTab::All => continue,
                            ShelfTab::Images => ("Images", "images"),
                            ShelfTab::Screenshots => ("Screenshots", "screenshots"),
                            ShelfTab::Downloads => ("Downloads", "downloads"),
                            ShelfTab::Text => ("Text", "text"),
                            ShelfTab::Links => ("Links", "links"),
                            ShelfTab::Tables => ("Tables", "tables"),
                            ShelfTab::Pods => ("Pods", "pods"),
                            ShelfTab::Files => ("Files", "files"),
                        };
                        let response = ui.add_enabled(
                            enabled,
                            egui::Button::new(format!("{} {label}", index + 1))
                                .fill(egui::Color32::TRANSPARENT)
                                .sense(egui::Sense::click_and_drag()),
                        );
                        surface =
                            Some(surface.map_or(response.rect, |rect| rect.union(response.rect)));
                        if active == value {
                            target = Some(response.rect);
                            if active_changed {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                        #[cfg(test)]
                        if index == 0 {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("test-first-tab-rect"), response.rect)
                            });
                        }
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
                            intent = Some(Command::SelectTabPosition {
                                position: (index + 1) as u8,
                            });
                        }
                    }
                    if let Some(rect) = surface {
                        ui.painter().set(
                            surface_slot,
                            egui::Shape::rect_filled(
                                rect.expand2(egui::vec2(2.0, 1.0)),
                                5.0,
                                ui.visuals().widgets.inactive.weak_bg_fill,
                            ),
                        );
                    }
                    let dt = ui.input(|i| i.stable_dt);
                    if indicator.update(active, target, dt, ui.style().animation_time) {
                        ui.ctx().request_repaint();
                    }
                    if let Some(rect) = indicator.rect {
                        ui.painter().set(
                            indicator_slot,
                            egui::Shape::rect_filled(rect, 4.0, ui.visuals().selection.bg_fill),
                        );
                    }
                });
            });
    });
    ui.ctx()
        .data_mut(|d| d.insert_temp(indicator_id, indicator));
    intent
}

/// Experimental presentation; the existing strip remains the exact default.
pub fn show_with_glass(
    ui: &mut egui::Ui,
    order: &[ShelfTab],
    active: &Value,
    enabled: bool,
    experimental_glass_tabs: bool,
) -> Option<Command> {
    if !experimental_glass_tabs {
        return show(ui, order, active, enabled);
    }
    let mut intent = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new("Folder").selected(active.as_str() == Some("folder")),
            )
            .clicked()
        {
            intent = Some(Command::SetTab { tab: ShelfTab::All });
        }
        ui.separator();
        if let Some(command) = show_glass(ui, order, active, enabled) {
            intent = Some(command);
        }
    });
    intent
}

fn show_glass(
    ui: &mut egui::Ui,
    order: &[ShelfTab],
    active: &Value,
    enabled: bool,
) -> Option<Command> {
    let active = active.as_str().unwrap_or("");
    let indicator_id = egui::Id::new("glass-category-strip-indicator");
    let mut indicator = ui
        .ctx()
        .data(|d| d.get_temp::<Indicator>(indicator_id))
        .unwrap_or_default();
    let active_changed = indicator.active != active;
    let mut intent = None;
    // Reserve the background before the scroll area's clipped contents so the
    // viewport remains one capsule even when the category list overflows.
    let body_slot = ui.painter().add(egui::Shape::Noop);
    let optics_slot = ui.painter().add(egui::Shape::Noop);
    let rim_slot = ui.painter().add(egui::Shape::Noop);
    let scroll = egui::ScrollArea::horizontal()
        .id_salt("glass-category-strip-scroll")
        .auto_shrink([true, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let indicator_slot = ui.painter().add(egui::Shape::Noop);
                let mut target = None;
                for (index, &tab) in order.iter().enumerate() {
                    let (label, value) = match tab {
                        ShelfTab::All => continue,
                        ShelfTab::Images => ("Images", "images"),
                        ShelfTab::Screenshots => ("Screenshots", "screenshots"),
                        ShelfTab::Downloads => ("Downloads", "downloads"),
                        ShelfTab::Text => ("Text", "text"),
                        ShelfTab::Links => ("Links", "links"),
                        ShelfTab::Tables => ("Tables", "tables"),
                        ShelfTab::Pods => ("Pods", "pods"),
                        ShelfTab::Files => ("Files", "files"),
                    };
                    let response = ui.add_enabled(
                        enabled,
                        egui::Button::new(format!("{} {label}", index + 1))
                            .frame(false)
                            .sense(egui::Sense::click_and_drag()),
                    );
                    if active == value {
                        target = Some(response.rect);
                        if active_changed {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                    }
                    #[cfg(test)]
                    if index == 0 {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("test-first-tab-rect"), response.rect)
                        });
                    }
                    response.dnd_set_drag_payload(tab);
                    if enabled
                        && let Some(payload) = response.dnd_hover_payload::<ShelfTab>()
                        && *payload != tab
                    {
                        ui.painter().rect_stroke(
                            response.rect.shrink(1.0),
                            response.rect.height() / 2.0,
                            ui.visuals().selection.stroke,
                            egui::StrokeKind::Inside,
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
                        intent = Some(Command::SelectTabPosition {
                            position: (index + 1) as u8,
                        });
                    }
                }
                let dt = ui.input(|i| i.stable_dt);
                if indicator.update(active, target, dt, ui.style().animation_time) {
                    ui.ctx().request_repaint();
                }
                if let Some(rect) = indicator.rect {
                    let inset = rect.shrink2(egui::vec2(1.0, 2.0));
                    ui.painter().set(
                        indicator_slot,
                        egui::Shape::rect_filled(
                            inset,
                            inset.height() / 2.0,
                            egui::Color32::from_black_alpha(205),
                        ),
                    );
                }
            });
        });
    let rect = scroll.inner_rect;
    let radius = rect.height() / 2.0;
    let surface = crate::theme_ui::color(
        ui.ctx(),
        crate::theme_ui::Role::Surface,
        egui::Color32::from_rgb(30, 30, 34),
    );
    ui.painter()
        .set(body_slot, egui::Shape::rect_filled(rect, radius, surface));
    if let Some(shape) = crate::theme_ui::optical_shape(ui.ctx(), rect, radius) {
        ui.painter().set(optics_slot, shape);
    }
    let border = crate::theme_ui::color(
        ui.ctx(),
        crate::theme_ui::Role::Border,
        egui::Color32::from_gray(95),
    );
    ui.painter().set(
        rim_slot,
        egui::Shape::rect_stroke(
            rect.shrink(0.5),
            radius,
            egui::Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        ),
    );
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("test-glass-tab-body"), rect));
    ui.ctx()
        .data_mut(|d| d.insert_temp(indicator_id, indicator));
    intent
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn experimental_strip_is_opt_in_and_fits_narrow_viewports() {
        let order = katran::tab_layout::default_order();
        for (experimental, width) in [(false, 700.0), (true, 700.0), (true, 180.0)] {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 100.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                show_with_glass(
                    ui,
                    &order,
                    &Value::String("images".into()),
                    true,
                    experimental,
                );
            });
            output.textures_delta.clear();
            let body = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("test-glass-tab-body")));
            assert_eq!(body.is_some(), experimental);
            if let Some(body) = body {
                assert!(body.is_positive());
                assert!(body.right() <= width);
            }
        }
    }

    #[test]
    fn experimental_folder_clears_indicator_and_return_starts_at_target() {
        let ctx = egui::Context::default();
        let order = katran::tab_layout::default_order();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(200.0, 100.0),
            )),
            ..Default::default()
        };
        let state = || {
            ctx.data(|d| d.get_temp::<Indicator>(egui::Id::new("glass-category-strip-indicator")))
                .unwrap()
        };
        let mut output = ctx.run_ui(input(), |ui| {
            show_with_glass(ui, &order, &Value::String("images".into()), true, true);
        });
        output.textures_delta.clear();
        assert!(state().rect.is_some());
        let mut output = ctx.run_ui(input(), |ui| {
            show_with_glass(ui, &order, &Value::String("folder".into()), true, true);
        });
        output.textures_delta.clear();
        assert!(state().rect.is_none());
        let mut output = ctx.run_ui(input(), |ui| {
            show_with_glass(ui, &order, &Value::String("downloads".into()), true, true);
        });
        output.textures_delta.clear();
        assert_eq!(state().active, "downloads");
        assert!(state().rect.is_some());
    }
    #[test]
    fn indicator_retargets_and_folder_breaks_the_path() {
        let rect = |x: f32, width: f32| {
            egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(width, 24.0))
        };
        let mut state = Indicator::default();
        let a = rect(10.0, 50.0);
        let b = rect(200.0, 80.0);
        let c = rect(90.0, 60.0);
        assert!(!state.update("images", Some(a), 0.016, 0.2));
        assert!(state.update("downloads", Some(b), 0.016, 0.2));
        let halfway = state.rect.unwrap();
        assert!(halfway.min.x > a.min.x && halfway.min.x < b.min.x);
        assert!(state.update("text", Some(c), 0.016, 0.2));
        assert!(state.rect.unwrap().min.x > halfway.min.x);
        assert!(!state.update("folder", None, 0.016, 0.2));
        assert_eq!(state.rect, None);
        assert!(!state.update("text", Some(c), 0.016, 0.2));
        assert_eq!(state.rect, Some(c));
        let resized = rect(110.0, 90.0);
        assert!(!state.update("text", Some(resized), 0.016, 0.0));
        assert_eq!(state.rect, Some(resized));
    }

    #[test]
    fn mouse_drop_emits_move_and_api_persists_order() {
        let root = tempfile::tempdir().unwrap();
        let mut api = katran::Service::open(root.path(), 10000).unwrap();
        let ctx = egui::Context::default();
        let order = katran::tab_layout::default_order();
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 100.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw(), |ui| {
            show(ui, &order, &Value::String("folder".into()), true);
        });
        output.textures_delta.clear();
        let target = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("test-first-tab-rect")))
            .unwrap()
            .center();
        egui::DragAndDrop::set_payload(&ctx, ShelfTab::Downloads);
        let mut input = raw();
        input.events = vec![
            egui::Event::PointerMoved(target),
            egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let mut command = None;
        let mut output = ctx.run_ui(input, |ui| {
            command = show(ui, &order, &Value::String("folder".into()), true);
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

//! Tab presentation emits dispatcher commands; durable order belongs to Settings.
use eframe::egui;
use katran::{Command, interaction::ShelfTab};
use serde_json::Value;

use avilon_egui::tabs::{Indicator, Pill};

pub fn show(
    ui: &mut egui::Ui,
    order: &[ShelfTab],
    active: &Value,
    enabled: bool,
) -> Option<Command> {
    let mut intent = None;
    let dragging_tab = egui::DragAndDrop::payload::<ShelfTab>(ui.ctx()).is_some();
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
            && !dragging_tab
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
                        } else if response.clicked() && !dragging_tab {
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
    let dragging_tab = egui::DragAndDrop::payload::<ShelfTab>(ui.ctx()).is_some();
    ui.horizontal(|ui| {
        let body = ui.painter().add(egui::Shape::Noop);
        let optical = ui.painter().add(egui::Shape::Noop);
        let rim = ui.painter().add(egui::Shape::Noop);
        let shine = ui.painter().add(egui::Shape::Noop);
        let label_width = ui
            .painter()
            .layout_no_wrap(
                "Folder".to_owned(),
                egui::TextStyle::Button.resolve(ui.style()),
                egui::Color32::WHITE,
            )
            .size()
            .x;
        let response = ui.add_enabled(
            enabled,
            egui::Button::new("Folder")
                .frame(false)
                .min_size(Pill::minimum_size(label_width, 12.0, 32.0)),
        );
        let pill = Pill::new(response.rect);
        let rect = pill.rect;
        let radius = pill.radius;
        let selected = active.as_str() == Some("folder");
        let surface = crate::theme_ui::color(
            ui.ctx(),
            if selected {
                crate::theme_ui::Role::Selection
            } else {
                crate::theme_ui::Role::Raised
            },
            if selected {
                ui.visuals().selection.bg_fill
            } else {
                egui::Color32::from_rgb(36, 36, 42)
            },
        );
        let border = crate::theme_ui::color(
            ui.ctx(),
            if selected {
                crate::theme_ui::Role::SelectionOutline
            } else {
                crate::theme_ui::Role::Border
            },
            if selected {
                ui.visuals().selection.stroke.color
            } else {
                egui::Color32::from_gray(100)
            },
        );
        ui.painter().set(body, pill.body(surface));
        if !selected && let Some(shape) = crate::theme_ui::optical_shape(ui.ctx(), rect, radius) {
            ui.painter().set(optical, shape);
        }
        ui.painter().set(rim, pill.rim(border));
        ui.painter()
            .set(shine, pill.shine(egui::Color32::from_white_alpha(42)));
        if response.clicked() && !dragging_tab {
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
    let dragging_tab = egui::DragAndDrop::payload::<ShelfTab>(ui.ctx()).is_some();
    let _scroll = egui::ScrollArea::horizontal()
        .id_salt("glass-category-strip-scroll")
        .auto_shrink([true, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                // Allocate all backgrounds before the indicator and labels so
                // the animated selection can pass over the individual pills.
                let slots: Vec<_> = order
                    .iter()
                    .map(|_| {
                        (
                            ui.painter().add(egui::Shape::Noop),
                            ui.painter().add(egui::Shape::Noop),
                            ui.painter().add(egui::Shape::Noop),
                            ui.painter().add(egui::Shape::Noop),
                        )
                    })
                    .collect();
                let indicator_slot = ui.painter().add(egui::Shape::Noop);
                let indicator_rim_slot = ui.painter().add(egui::Shape::Noop);
                let mut target = None;
                #[cfg(test)]
                let mut button_geometry = Vec::new();
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
                    let label = format!("{} {label}", index + 1);
                    let label_width = ui
                        .painter()
                        .layout_no_wrap(
                            label.clone(),
                            egui::TextStyle::Button.resolve(ui.style()),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    let response = ui.add_enabled(
                        enabled,
                        egui::Button::new(label)
                            .frame(false)
                            .min_size(Pill::minimum_size(label_width, 12.0, 32.0))
                            .sense(egui::Sense::click_and_drag()),
                    );
                    let pill = Pill::new(response.rect);
                    let rect = pill.rect;
                    let radius = pill.radius;
                    let surface = crate::theme_ui::color(
                        ui.ctx(),
                        crate::theme_ui::Role::Raised,
                        egui::Color32::from_rgb(36, 36, 42),
                    );
                    let border = crate::theme_ui::color(
                        ui.ctx(),
                        crate::theme_ui::Role::Border,
                        egui::Color32::from_gray(100),
                    );
                    let (body, optical, rim, shine) = slots[index];
                    ui.painter().set(body, pill.body(surface));
                    if let Some(shape) = crate::theme_ui::optical_shape(ui.ctx(), rect, radius) {
                        ui.painter().set(optical, shape);
                    }
                    ui.painter().set(rim, pill.rim(border));
                    ui.painter()
                        .set(shine, pill.shine(egui::Color32::from_white_alpha(42)));
                    #[cfg(test)]
                    button_geometry.push((response.rect, label_width));
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
                    } else if response.clicked() && !dragging_tab {
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
                    let inset = rect.shrink(1.5);
                    let selection = crate::theme_ui::color(
                        ui.ctx(),
                        crate::theme_ui::Role::Selection,
                        ui.visuals().selection.bg_fill,
                    );
                    let outline = crate::theme_ui::color(
                        ui.ctx(),
                        crate::theme_ui::Role::SelectionOutline,
                        ui.visuals().selection.stroke.color,
                    );
                    ui.painter().set(
                        indicator_slot,
                        egui::Shape::rect_filled(inset, inset.height() / 2.0, selection),
                    );
                    ui.painter().set(
                        indicator_rim_slot,
                        egui::Shape::rect_stroke(
                            inset,
                            inset.height() / 2.0,
                            egui::Stroke::new(1.0, outline),
                            egui::StrokeKind::Inside,
                        ),
                    );
                }
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("test-glass-button-geometry"), button_geometry)
                });
            });
        });
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("test-glass-tab-body"), _scroll.inner_rect));
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
                let buttons = ctx
                    .data(|d| {
                        d.get_temp::<Vec<(egui::Rect, f32)>>(egui::Id::new(
                            "test-glass-button-geometry",
                        ))
                    })
                    .unwrap();
                assert_eq!(buttons.len(), order.len());
                for (rect, text_width) in &buttons {
                    assert!(rect.width() >= text_width + 23.0);
                    assert!(rect.height() >= 32.0);
                }
                for pair in buttons.windows(2) {
                    assert!(pair[1].0.left() - pair[0].0.right() >= 5.0);
                }
            }
        }
    }

    #[test]
    fn experimental_visible_tabs_use_configured_selection_colors() {
        let ctx = egui::Context::default();
        crate::theme_ui::apply(
            &ctx,
            &katran::theme::ThemePreferences {
                selection: Some([30, 220, 80]),
                selection_intensity: 80,
                ..Default::default()
            },
        );
        let order = [ShelfTab::Images, ShelfTab::Pods];
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(220.0, 100.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            show_with_glass(ui, &order, &Value::String("pods".into()), true, true);
        });
        let fill = crate::theme_ui::color(
            &ctx,
            crate::theme_ui::Role::Selection,
            ctx.style_of(egui::Theme::Dark).visuals.selection.bg_fill,
        );
        let outline = egui::Color32::from_rgb(30, 220, 80);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::Shape::Rect(rect) if rect.fill == fill))
        );
        assert!(
            output.shapes.iter().any(
                |s| matches!(&s.shape, egui::Shape::Rect(rect) if rect.stroke.color == outline)
            )
        );
        let buttons = ctx
            .data(|d| {
                d.get_temp::<Vec<(egui::Rect, f32)>>(egui::Id::new("test-glass-button-geometry"))
            })
            .unwrap();
        assert_eq!(buttons.len(), 2);
        output.textures_delta.clear();
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
    fn experimental_drag_emits_existing_reorder_command() {
        let ctx = egui::Context::default();
        let order = [ShelfTab::Images, ShelfTab::Pods];
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 100.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input(), |ui| {
            show_with_glass(ui, &order, &Value::String("folder".into()), true, true);
        });
        output.textures_delta.clear();
        let target = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("test-first-tab-rect")))
            .unwrap()
            .center();
        egui::DragAndDrop::set_payload(&ctx, ShelfTab::Pods);
        let mut release = input();
        release.events = vec![
            egui::Event::PointerMoved(target),
            egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let mut command = None;
        let mut output = ctx.run_ui(release, |ui| {
            command = show_with_glass(ui, &order, &Value::String("folder".into()), true, true);
        });
        output.textures_delta.clear();
        assert!(matches!(
            command,
            Some(Command::MoveTab {
                tab: ShelfTab::Pods,
                index: 0
            })
        ));
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

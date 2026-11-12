//! Optional wheel-distance smoothing. Storage/validation belongs to ThemePreferences.
//!
//! Capture before the shelf ScrollArea, apply AFTER its child widgets, at the end
//! of show_rows' closure. egui's ScrollArea::end then handles the adjusted wheel
//! through the normal path. Nested areas get first refusal; no offset overrides,
//! raw-event replay, alternative storage, or ScrollArea private fields are used.
use super::*;

#[derive(Clone, Copy, Debug, Default)]
struct Motion {
    remaining: f32,
    expected_offset: Option<f32>,
    last_time: f64,
    last_frame: Option<u64>,
    applied_delta: f32,
}

impl Motion {
    fn step(&mut self, input: f32, dt: f32, speed: f32, tau: f32) -> f32 {
        // An opposite wheel movement must turn immediately, not fight the tail.
        if input != 0.0 && input.signum() != self.remaining.signum() {
            self.remaining = 0.0;
        }
        self.remaining += input * speed;
        if self.remaining.abs() < 0.05 {
            std::mem::take(&mut self.remaining)
        } else {
            let delta = self.remaining * (1.0 - (-dt / tau).exp());
            self.remaining -= delta;
            delta
        }
    }
}

pub(super) struct Frame {
    key: egui::Id,
    wheel: f32,
    offset: f32,
    viewport_height: f32,
    speed: f32,
    tau: f32,
    cancelled: bool,
}

/// Call immediately before constructing the shelf ScrollArea. `programmatic`
/// is the selection/keyboard scroll intent BEFORE clearing Shelf::scroll.
pub(super) fn begin(
    ui: &egui::Ui,
    scroll_id: egui::Id,
    theme: &katran::theme::ThemePreferences,
    programmatic: bool,
) -> Option<Frame> {
    let ctx = ui.ctx();
    let key = scroll_id.with("banana-wheel");
    if !theme.banana_scroll {
        ctx.data_mut(|data| data.remove::<Motion>(key));
        return None;
    }
    let pointer_inside = ui.rect_contains_pointer(ui.available_rect_before_wrap());
    let (wheel, cancelled) = ui.input(|input| {
        let keyboard = input
            .raw
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Key { pressed: true, .. }));
        (
            input.smooth_scroll_delta().y,
            programmatic
                || !pointer_inside
                || !input.raw.focused
                || input.pointer.any_down()
                || input.pointer.any_pressed()
                || keyboard
                || input.modifiers.any()
                || input.smooth_scroll_delta().x != 0.0,
        )
    });
    Some(Frame {
        key,
        wheel,
        offset: egui::scroll_area::State::load(ctx, scroll_id).map_or(0.0, |state| state.offset.y),
        viewport_height: ui.available_height(),
        speed: f32::from(theme.banana_scroll_speed.clamp(25, 300)) / 100.0,
        tau: f32::from(theme.banana_scroll_inertia_ms.clamp(50, 1000)) / 1000.0,
        cancelled,
    })
}

impl Frame {
    /// Call at the END of show_rows' closure, after nested scrollable children.
    /// `content_height` includes all virtual rows, excluding trailing row spacing.
    pub(super) fn apply(self, ui: &egui::Ui, content_height: f32) {
        let ctx = ui.ctx();
        let (remaining_wheel, now, dt) =
            ui.input(|input| (input.smooth_scroll_delta().y, input.time, input.stable_dt));
        let mut motion = ctx
            .data(|data| data.get_temp::<Motion>(self.key))
            .unwrap_or_default();
        let nested_consumed = (remaining_wheel - self.wheel).abs() > 0.001;
        let max_offset = (content_height - self.viewport_height).max(0.0);
        let interrupted = motion
            .expected_offset
            .is_some_and(|offset| (offset - self.offset).abs() > 2.0);
        if self.cancelled || nested_consumed || interrupted || max_offset <= 0.0 {
            ctx.data_mut(|data| data.remove::<Motion>(self.key));
            return;
        }
        let frame = ctx.cumulative_frame_nr();
        // Repeated layout passes must replay, rather than add wheel input twice.
        let delta = if motion.last_frame == Some(frame) {
            motion.applied_delta
        } else {
            if now - motion.last_time > 0.25 {
                motion.remaining = 0.0;
            }
            let delta = motion.step(self.wheel, dt.clamp(0.001, 0.05), self.speed, self.tau);
            // Match ScrollArea's limits so an inertial tail never leaks to a
            // parent scroll area at the edge. No overscroll or rebound.
            let offset = (self.offset - delta).clamp(0.0, max_offset);
            let applied = self.offset - offset;
            if (applied - delta).abs() > 0.001 {
                motion.remaining = 0.0;
            }
            motion.expected_offset = Some(offset);
            motion.last_time = now;
            motion.last_frame = Some(frame);
            motion.applied_delta = applied;
            applied
        };
        ui.input_mut(|input| input.smooth_scroll_delta.y = delta);
        if motion.remaining.abs() > 0.0 {
            ctx.request_repaint();
        }
        ctx.data_mut(|data| data.insert_temp(self.key, motion));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_distance_is_preserved_with_a_tail() {
        let mut motion = Motion::default();
        let first = motion.step(-100.0, 1.0 / 60.0, 1.5, 0.24);
        assert!(first < 0.0 && first > -25.0);
        let tail = motion.step(0.0, 1.0 / 60.0, 1.5, 0.24);
        assert!(tail < 0.0);
        let mut total = first + tail;
        for _ in 0..500 {
            total += motion.step(0.0, 1.0 / 60.0, 1.5, 0.24);
        }
        assert!((total + 150.0).abs() < 0.001);
        assert_eq!(motion.remaining, 0.0);
    }

    #[test]
    fn reversing_direction_cancels_previous_tail() {
        let mut motion = Motion::default();
        motion.step(-100.0, 1.0 / 60.0, 1.0, 0.24);
        assert!(motion.step(5.0, 1.0 / 60.0, 1.0, 0.24) > 0.0);
        assert!(motion.remaining > 0.0 && motion.remaining < 5.0);
    }

    #[test]
    fn smoothing_time_is_independent_of_frame_rate() {
        let advance = |hz: u32| {
            let mut motion = Motion {
                remaining: 100.0,
                ..Default::default()
            };
            for _ in 0..hz {
                motion.step(0.0, 1.0 / hz as f32, 1.0, 0.5);
            }
            motion.remaining
        };
        assert!((advance(30) - advance(144)).abs() < 0.001);
    }

    fn frame(
        ctx: &egui::Context,
        now: f64,
        wheel: f32,
        enabled: bool,
        programmatic: bool,
        nested: bool,
    ) -> (f32, f32) {
        let mut offsets = (0.0, 0.0);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 200.0),
                )),
                time: Some(now),
                predicted_dt: 1.0 / 60.0,
                events: vec![egui::Event::PointerMoved(egui::pos2(60.0, 60.0))],
                ..Default::default()
            },
            |ui| {
                ui.input_mut(|input| input.smooth_scroll_delta = egui::vec2(0.0, wheel));
                let id = ui.make_persistent_id(egui::IdSalt::new("test-banana-area"));
                let theme = katran::theme::ThemePreferences {
                    banana_scroll: enabled,
                    ..Default::default()
                };
                let banana = begin(ui, id, &theme, programmatic);
                let content_height =
                    100.0 * (40.0 + ui.spacing().item_spacing.y) - ui.spacing().item_spacing.y;
                let output = egui::ScrollArea::vertical()
                    .id_salt("test-banana-area")
                    .auto_shrink([false, false])
                    .show_rows(ui, 40.0, 100, |ui, range| {
                        if nested {
                            offsets.1 = egui::ScrollArea::vertical()
                                .id_salt("test-nested-area")
                                .max_height(100.0)
                                .auto_shrink([false, false])
                                .show_rows(ui, 40.0, 20, |ui, range| {
                                    for _ in range {
                                        ui.allocate_space(egui::vec2(100.0, 40.0));
                                    }
                                })
                                .state
                                .offset
                                .y;
                        } else {
                            for _ in range {
                                ui.allocate_space(egui::vec2(100.0, 40.0));
                            }
                        }
                        if let Some(banana) = banana {
                            banana.apply(ui, content_height);
                        }
                    });
                offsets.0 = output.state.offset.y;
            },
        );
        output.textures_delta.clear();
        offsets
    }

    #[test]
    fn native_scroll_area_keeps_normal_distance_when_disabled() {
        let ctx = egui::Context::default();
        frame(&ctx, 0.0, 0.0, false, false, false);
        let (offset, _) = frame(&ctx, 1.0 / 60.0, -90.0, false, false, false);
        assert!((offset - 90.0).abs() < 0.01, "offset={offset}");
        let (rest, _) = frame(&ctx, 2.0 / 60.0, 0.0, false, false, false);
        assert!((rest - offset).abs() < 0.01);
    }

    #[test]
    fn native_virtualized_area_receives_smooth_tail_and_keeps_distance() {
        let ctx = egui::Context::default();
        frame(&ctx, 0.0, 0.0, true, false, false);
        let (first, _) = frame(&ctx, 1.0 / 60.0, -90.0, true, false, false);
        assert!(first > 0.0 && first < 30.0, "first={first}");
        let (second, _) = frame(&ctx, 2.0 / 60.0, 0.0, true, false, false);
        assert!(second > first, "first={first}, second={second}");
        let mut offset = second;
        for index in 3..180 {
            offset = frame(&ctx, f64::from(index) / 60.0, 0.0, true, false, false).0;
        }
        assert!((offset - 90.0).abs() < 0.1, "offset={offset}");
    }

    #[test]
    fn native_nested_area_gets_first_refusal() {
        let ctx = egui::Context::default();
        frame(&ctx, 0.0, 0.0, true, false, true);
        let (outer, inner) = frame(&ctx, 1.0 / 60.0, -90.0, true, false, true);
        assert_eq!(outer, 0.0);
        assert!((inner - 90.0).abs() < 0.01, "inner={inner}");
        let (outer, _) = frame(&ctx, 2.0 / 60.0, 0.0, true, false, true);
        assert_eq!(outer, 0.0);
    }

    #[test]
    fn programmatic_selection_scroll_cancels_pending_tail() {
        let ctx = egui::Context::default();
        frame(&ctx, 0.0, 0.0, true, false, false);
        let first = frame(&ctx, 1.0 / 60.0, -90.0, true, false, false).0;
        let cancelled = frame(&ctx, 2.0 / 60.0, 0.0, true, true, false).0;
        let rest = frame(&ctx, 3.0 / 60.0, 0.0, true, false, false).0;
        assert_eq!(first, cancelled);
        assert_eq!(cancelled, rest);
    }
}

//! Explicit-play video preview, driven by dispatcher-owned sessions.
use super::i18n::tr;
use base64::Engine;
use eframe::egui::{self, Color32};
use katran::{Command, domain_jobs::JobId, media_playback::Action};
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub id: i64,
    pub revision: String,
    pub context: String,
}

/// Presentation routing only; the dispatcher revalidates the source and context.
pub(super) fn target(event: &Value, state: &Value, full: bool) -> Option<Target> {
    if !matches!(
        event["kind"].as_str(),
        Some("file_reference" | "managed_image")
    ) {
        return None;
    }
    let video = event["path"]
        .as_str()
        .is_some_and(|s| katran::file_types::is_native_video(Path::new(s)))
        || event["name"]
            .as_str()
            .is_some_and(|s| katran::file_types::is_native_video(Path::new(s)));
    if !video || event["available"] == false {
        return None;
    }
    Some(Target {
        id: event["id"].as_i64()?,
        revision: event["revision"].as_str()?.to_owned(),
        context: serde_json::json!([
            state["tab"],
            state["folder_path"],
            if full { "preview" } else { "details" }
        ])
        .to_string(),
    })
}

struct PendingSeek {
    position: u64,
    issued: Instant,
}

struct Pending {
    id: JobId,
    target: Target,
    cancelled: bool,
    cancel_sent: bool,
}

#[derive(Default)]
pub(super) struct State {
    target: Option<Target>,
    pending: Option<Pending>,
    session: Option<u64>,
    stopping: bool,
    snapshot: Value,
    serial: u64,
    texture: Option<egui::TextureHandle>,
    next_poll: Option<Instant>,
    error: Option<String>,
    chosen_volume: Option<f64>,
    seek_position: Option<u64>,
    pending_seek: Option<PendingSeek>,
    auto_start: bool,
    last_activity: Option<Instant>,
    controls_focused: bool,
    frame_hold: Option<bool>,
    frame_wait_serial: Option<u64>,
    confirmed_step_position: Option<u64>,
    frame_retry: Option<Instant>,
    frame_blocked: bool,
}

impl State {
    fn message(reply: &Value) -> String {
        let code = reply["error"]["code"].as_str().unwrap_or("media_error");
        let message = reply["error"]["message"]
            .as_str()
            .unwrap_or(tr("Видео недоступно", "Video unavailable"));
        format!("{message} ({code})")
    }

    /// Called every frame, including hide/close. Never starts playback itself.
    pub(super) fn sync(
        &mut self,
        ctx: &egui::Context,
        target: Option<Target>,
        call: &mut dyn FnMut(Command) -> Value,
    ) {
        if self.target != target {
            self.error = None;
            self.stop(call);
            self.auto_start = target.as_ref().is_some_and(|target| {
                serde_json::from_str::<Value>(&target.context)
                    .ok()
                    .and_then(|value| value.get(2).and_then(Value::as_str).map(str::to_owned))
                    .as_deref()
                    == Some("preview")
            });
            self.last_activity = Some(Instant::now());
            self.controls_focused = false;
            self.target = target;
            self.texture = None;
            self.serial = 0;
            self.seek_position = None;
            self.pending_seek = None;
            self.frame_hold = None;
            self.frame_wait_serial = None;
            self.confirmed_step_position = None;
            self.frame_retry = None;
            self.frame_blocked = false;
        }
        if self.stopping {
            self.stop(call);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.cancelled && !pending.cancel_sent)
        {
            self.stop(call);
        }
        if let Some(pending) = self.pending.as_ref() {
            let id = pending.id;
            let response = call(Command::PollJob { job_id: id });
            if response["ok"] != true {
                self.error = Some(Self::message(&response));
                ctx.request_repaint_after(Duration::from_millis(200));
                return;
            }
            match response["result"]["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    return;
                }
                Some("succeeded") => {
                    let pending = self.pending.take().unwrap();
                    let session = response["result"]["result"].clone();
                    if pending.cancelled || self.target.as_ref() != Some(&pending.target) {
                        if let Some(token) = session["token"].as_u64() {
                            self.session = Some(token);
                            self.stopping = true;
                            self.stop(call);
                        }
                    } else if self.accept_session(&session, &pending.target) {
                        self.play_opened(call);
                    }
                }
                Some("failed" | "cancelled") => {
                    let pending = self.pending.take().unwrap();
                    if !pending.cancelled {
                        self.error = Some(Self::message(&response["result"]));
                    }
                }
                _ => {
                    self.error = Some(
                        tr(
                            "Неизвестное состояние загрузки видео",
                            "Unknown video loading state",
                        )
                        .into(),
                    );
                    ctx.request_repaint_after(Duration::from_millis(200));
                    return;
                }
            }
        }
        if self.stopping {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }
        let Some(token) = self.session else {
            return;
        };
        if self.next_poll.is_some_and(|when| Instant::now() < when) {
            ctx.request_repaint_after(
                self.next_poll
                    .unwrap()
                    .saturating_duration_since(Instant::now()),
            );
            return;
        }
        let state = call(Command::MediaState);
        if state["ok"] != true {
            self.error = Some(Self::message(&state));
            self.stopping = true;
            self.stop(call);
            if self.stopping {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            return;
        }
        if state["result"].is_null() {
            self.session = None;
            self.texture = None;
            return;
        }
        if state["result"]["token"].as_u64() != Some(token) {
            self.session = None;
            self.texture = None;
            self.error = Some(tr("Сессия видео изменилась", "Video session changed").into());
            return;
        }
        self.snapshot = state["result"]["snapshot"].clone();
        self.observe_seek();
        if let Some(error) = self.snapshot["error"].as_str() {
            self.error = Some(error.to_owned());
        }
        if matches!(self.snapshot["state"].as_str(), Some("stopped" | "error")) {
            if self.error.is_none() {
                self.error = Some(tr("Воспроизведение остановлено", "Playback stopped").into());
            }
            self.stopping = true;
            self.stop(call);
            if self.stopping {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            return;
        }
        let frame = call(Command::MediaFrame {
            session: token,
            after_serial: self.serial,
        });
        if frame["ok"] != true {
            self.error = Some(Self::message(&frame));
            self.stopping = true;
            self.stop(call);
            if self.stopping {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            return;
        }
        if frame["result"]["session"].as_u64() == Some(token) && !frame["result"]["frame"].is_null()
        {
            if let Some((serial, image)) = decode_frame(&frame["result"]["frame"], self.serial) {
                self.serial = serial;
                if let Some(texture) = self.texture.as_mut() {
                    texture.set(image, egui::TextureOptions::LINEAR);
                } else {
                    self.texture = Some(ctx.load_texture(
                        "media-preview",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            } else {
                self.error = Some(tr("Некорректный кадр видео", "Invalid video frame").into());
                self.stopping = true;
                self.stop(call);
                if self.stopping {
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
                return;
            }
        }
        if self.snapshot["step_pending"] != true
            && self
                .frame_wait_serial
                .is_some_and(|serial| self.serial > serial)
        {
            self.frame_wait_serial = None;
            self.confirmed_step_position = if self.frame_hold.is_some() {
                self.snapshot["position_ms"].as_u64()
            } else {
                None
            };
        }
        let delay = if self.snapshot["state"] == "playing" {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(150)
        };
        self.next_poll = Some(Instant::now() + delay);
        ctx.request_repaint_after(delay);
    }

    /// Cancel unfinished preparation before polling it; retain unconfirmed stops.
    pub(super) fn stop(&mut self, call: &mut dyn FnMut(Command) -> Value) {
        self.seek_position = None;
        self.pending_seek = None;
        self.frame_hold = None;
        self.frame_wait_serial = None;
        self.confirmed_step_position = None;
        self.frame_retry = None;
        self.frame_blocked = false;
        if let Some(pending) = self.pending.as_mut() {
            pending.cancelled = true;
            if !pending.cancel_sent {
                let response = call(Command::CancelJob { job_id: pending.id });
                if response["ok"] == true || response["error"]["code"] == "job_not_cancellable" {
                    pending.cancel_sent = true;
                } else {
                    self.error = Some(Self::message(&response));
                }
            }
        }
        if let Some(token) = self.session {
            let response = call(Command::ControlMedia {
                session: token,
                action: Action::Stop,
            });
            if response["ok"] == true
                || response["error"]["code"] == "stale_media_session"
                || response["error"]["code"] == "media_context_closed"
                || response["error"]["code"] == "source_changed"
            {
                self.session = None;
                self.stopping = false;
                self.snapshot = Value::Null;
                self.texture = None;
                self.serial = 0;
            } else {
                self.stopping = true;
                self.error = Some(Self::message(&response));
            }
        }
    }

    fn accept_session(&mut self, value: &Value, target: &Target) -> bool {
        let Some(token) = value["token"].as_u64() else {
            self.error = Some(tr("Некорректная сессия видео", "Invalid video session").into());
            return false;
        };
        if value["identity"]["id"].as_i64() != Some(target.id)
            || value["identity"]["revision"].as_str() != Some(target.revision.as_str())
        {
            self.session = Some(token);
            self.stopping = true;
            self.error = Some(tr("Сессия видео устарела", "Video session is stale").into());
            return false;
        }
        self.session = Some(token);
        self.snapshot = value["snapshot"].clone();
        self.next_poll = None;
        true
    }

    fn play_opened(&mut self, call: &mut dyn FnMut(Command) -> Value) {
        if let Some(volume) = self.chosen_volume
            && !self.control(Action::SetVolume { volume }, call)
        {
            return;
        }
        self.control(Action::Play, call);
    }

    fn control(&mut self, action: Action, call: &mut dyn FnMut(Command) -> Value) -> bool {
        let Some(token) = self.session else {
            return false;
        };
        let ordinary_transport =
            matches!(&action, Action::Play | Action::Pause | Action::Seek { .. });
        let requested_seek = match &action {
            Action::Seek { position_ms } => Some(*position_ms),
            _ => None,
        };
        let requested_volume = match &action {
            Action::SetVolume { volume } => Some(*volume),
            _ => None,
        };
        let result = call(Command::ControlMedia {
            session: token,
            action,
        });
        if result["ok"] == true {
            if ordinary_transport {
                self.confirmed_step_position = None;
            }
            if result["result"].is_null() {
                self.session = None;
                self.texture = None;
            } else if result["result"]["token"].as_u64() == Some(token) {
                self.snapshot = result["result"]["snapshot"].clone();
                self.error = None;
                if let Some(position) = requested_seek {
                    self.pending_seek = Some(PendingSeek {
                        position,
                        issued: Instant::now(),
                    });
                }
                if let Some(volume) = requested_volume {
                    self.chosen_volume = Some(volume);
                }
            } else {
                self.error = Some(tr("Сессия видео изменилась", "Video session changed").into());
                return false;
            }
        } else {
            if requested_seek.is_some() {
                self.pending_seek = None;
                self.seek_position = None;
            }
            self.error = Some(Self::message(&result));
            return false;
        }
        self.next_poll = None;
        true
    }

    fn observe_seek(&mut self) {
        let Some(pending) = &self.pending_seek else {
            return;
        };
        let elapsed = pending.issued.elapsed();
        let observed = self.snapshot["position_ms"].as_u64().unwrap_or(0);
        let playing = self.snapshot["state"] == "playing";
        let max_progress = if playing {
            elapsed.as_millis().min(u64::MAX as u128) as u64
        } else {
            0
        };
        let lower = pending.position.saturating_sub(250);
        let upper = pending
            .position
            .saturating_add(max_progress)
            .saturating_add(250);
        if (lower..=upper).contains(&observed) {
            self.pending_seek = None;
        } else if elapsed >= Duration::from_secs(4) {
            self.pending_seek = None;
            self.error =
                Some(tr("Не удалось подтвердить перемотку", "Could not confirm seek").into());
        }
    }

    /// Release key ownership without sending commands or discarding an in-flight step.
    pub(super) fn cancel_frame_hold(&mut self) {
        self.frame_hold = None;
        self.frame_blocked = false;
        if self.frame_wait_serial.is_none() && self.snapshot["step_pending"] != true {
            self.confirmed_step_position = None;
        }
    }

    /// One in-flight native step at a time; release never cancels an accepted step.
    pub(super) fn handle_frame_hold(
        &mut self,
        ctx: &egui::Context,
        direction: Option<bool>,
        call: &mut dyn FnMut(Command) -> Value,
    ) {
        let Some(backwards) = direction else {
            self.cancel_frame_hold();
            return;
        };
        if self.session.is_none() || self.stopping || self.pending.is_some() {
            self.cancel_frame_hold();
            return;
        }
        self.frame_hold = Some(backwards);
        self.last_activity = Some(Instant::now());
        // Holding overrides the normal paused polling interval; native completion
        // remains the pacing gate, with at most one accepted step in flight.
        self.next_poll = None;
        self.sync(ctx, self.target.clone(), call);
        ctx.request_repaint_after(Duration::from_millis(16));
        let Some(token) = self.session else {
            return;
        };
        if self.frame_blocked
            || self.pending_seek.is_some()
            || self.snapshot["step_pending"] == true
        {
            return;
        }
        if self.frame_retry.is_some_and(|retry| Instant::now() < retry) {
            return;
        }
        if let Some(serial) = self.frame_wait_serial {
            if self.serial <= serial {
                return;
            }
            self.frame_wait_serial = None;
        }
        let position = self.snapshot["position_ms"].as_u64().unwrap_or(0);
        if (backwards && position == 0 && self.snapshot["state"] != "playing")
            || (!backwards && self.snapshot["state"] == "ended")
        {
            return;
        }
        let before = self.serial;
        self.confirmed_step_position.get_or_insert(position);
        let result = call(Command::ControlMedia {
            session: token,
            action: Action::StepFrame { backwards },
        });
        if result["ok"] == true {
            if result["result"]["token"].as_u64() == Some(token) {
                self.snapshot = result["result"]["snapshot"].clone();
                self.frame_wait_serial = Some(before);
                self.frame_retry = None;
                self.error = None;
            } else {
                self.error = Some(tr("Сессия видео изменилась", "Video session changed").into());
                self.frame_blocked = true;
            }
        } else {
            let code = result["error"]["code"].as_str().unwrap_or("");
            let message = result["error"]["message"].as_str().unwrap_or("");
            if code == "media_step_busy"
                || (code == "media_control_failed" && message.contains("media_step_busy"))
            {
                self.frame_retry = Some(Instant::now() + Duration::from_millis(100));
            } else {
                self.error = Some(Self::message(&result));
                self.frame_blocked = true;
            }
        }
    }

    pub(super) fn seek_by(
        &mut self,
        delta_ms: i64,
        call: &mut dyn FnMut(Command) -> Value,
    ) -> bool {
        if self.session.is_none() || self.stopping || self.pending.is_some() {
            return false;
        }
        let Some(duration) = self.snapshot["duration_ms"]
            .as_u64()
            .filter(|duration| *duration > 0)
        else {
            return false;
        };
        let current = self
            .seek_position
            .or_else(|| self.pending_seek.as_ref().map(|seek| seek.position))
            .unwrap_or_else(|| self.snapshot["position_ms"].as_u64().unwrap_or(0));
        let position_ms = current.saturating_add_signed(delta_ms).min(duration);
        self.seek_position = None;
        self.control(Action::Seek { position_ms }, call);
        true
    }

    pub(super) fn toggle_pause(&mut self, call: &mut dyn FnMut(Command) -> Value) -> bool {
        if self.session.is_none() || self.stopping || self.pending.is_some() {
            return false;
        }
        if self.snapshot["state"] == "ended" && !self.control(Action::Seek { position_ms: 0 }, call)
        {
            return true;
        }
        let action = if self.snapshot["state"] == "playing" {
            Action::Pause
        } else {
            Action::Play
        };
        self.control(action, call);
        true
    }

    fn start(&mut self, target: &Target, call: &mut dyn FnMut(Command) -> Value) {
        self.target = Some(target.clone());
        self.error = None;
        let result = call(Command::BeginJob {
            command: Box::new(Command::OpenMedia {
                id: target.id,
                revision: target.revision.clone(),
            }),
        });
        if result["ok"] != true {
            self.error = Some(Self::message(&result));
        } else if result["result"]["state"] == "succeeded" {
            if self.accept_session(&result["result"]["result"], target) {
                self.play_opened(call);
            }
        } else if let Some(id) = result["result"]["job_id"].as_u64() {
            self.pending = Some(Pending {
                id: JobId(id),
                target: target.clone(),
                cancelled: false,
                cancel_sent: false,
            });
        } else {
            self.error = Some(tr("Не удалось запустить видео", "Could not start video").into());
        }
    }

    pub(super) fn show_with_poster(
        &mut self,
        ui: &mut egui::Ui,
        target: &Target,
        event: &Value,
        images: &mut super::images::Images,
        call: &mut dyn FnMut(Command) -> Value,
    ) {
        self.sync(ui.ctx(), Some(target.clone()), call);
        if self.texture.is_none() {
            let size = egui::vec2(
                ui.available_width().max(1.0),
                ui.available_height().max(1.0),
            );
            let canvas = egui::Rect::from_min_size(ui.next_widget_position(), size);
            ui.painter().rect_filled(canvas, 5.0, Color32::BLACK);
            images.paint_at(ui, event, canvas, 512);
        }
        self.show(ui, target, call);
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        target: &Target,
        call: &mut dyn FnMut(Command) -> Value,
    ) {
        self.sync(ui.ctx(), Some(target.clone()), call);
        if self.auto_start && self.pending.is_none() && self.session.is_none() && !self.stopping {
            self.auto_start = false;
            self.start(target, call);
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let width = ui.available_width().max(1.0);
        let height = ui.available_height().max(1.0);
        let (canvas, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        if let Some(texture) = &self.texture {
            ui.painter().rect_filled(canvas, 5.0, Color32::BLACK);
            let rect = super::gallery_card::fit_rect(canvas, texture.size_vec2());
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let busy = self.pending.is_some() || self.stopping;
        let now = Instant::now();
        let activity = ui.input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|p| canvas.contains(p))
                && (input.pointer.delta() != egui::Vec2::ZERO || input.pointer.any_down())
        });
        if activity {
            self.last_activity = Some(now);
        }
        let stepping = self.frame_hold.is_some()
            || self.frame_wait_serial.is_some()
            || self.snapshot["step_pending"] == true;
        let visible = controls_visible(
            if stepping {
                Some("paused")
            } else {
                self.snapshot["state"].as_str()
            },
            busy,
            self.error.is_some(),
            self.controls_focused,
            self.last_activity,
            now,
        );
        if !visible {
            return;
        }
        if let Some(acceleration) = self.snapshot["acceleration"].as_object() {
            let gpu = acceleration.get("renderer").and_then(Value::as_str) == Some("gpu");
            let requested =
                acceleration.get("requested_gpu").and_then(Value::as_bool) == Some(true);
            let label = if gpu {
                "GPU"
            } else if requested {
                "CPU · fallback"
            } else {
                "CPU"
            };
            let decoder = acceleration
                .get("decoder")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let reason = acceleration
                .get("fallback_reason")
                .and_then(Value::as_str)
                .unwrap_or("");
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(
                    canvas.min + egui::vec2(10.0, 8.0),
                    canvas.max,
                )),
                |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(170))
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.small(label).on_hover_text(format!(
                                "{}: {decoder}\n{reason}",
                                tr("Декодер", "Decoder")
                            ));
                        });
                },
            );
        }
        if self.snapshot["state"] == "playing" && !busy && self.error.is_none() {
            ui.ctx().request_repaint_after(Duration::from_secs(2));
        }
        let inset = 8.0_f32.min(canvas.height() * 0.05);
        let button_font = egui::TextStyle::Button.resolve(ui.style());
        let button_text_height = ui.fonts_mut(|fonts| fonts.row_height(&button_font));
        let row_height = ui
            .spacing()
            .interact_size
            .y
            .max(button_text_height + 2.0 * ui.spacing().button_padding.y)
            .max(18.0);
        let timeline_height = if self.session.is_some()
            && self.snapshot["duration_ms"].as_u64().is_some_and(|d| d > 0)
        {
            ui.spacing().interact_size.y.max(16.0) + ui.spacing().item_spacing.y
        } else {
            0.0
        };
        let error_height = if self.error.is_some() {
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y
        } else {
            0.0
        };
        // Slider creates an inner horizontal UI, whose row can extend beyond the
        // allocated response at larger interaction sizes. Reserve that extent.
        let nested_row_reserve = ui.spacing().interact_size.y * 0.5;
        let panel_height =
            (12.0 + timeline_height + row_height + error_height + nested_row_reserve)
                .min((canvas.height() - inset).max(0.0));
        let bottom = canvas.bottom() - inset;
        let panel = egui::Rect::from_min_max(
            egui::pos2(canvas.left(), bottom - panel_height),
            egui::pos2(canvas.right(), bottom),
        );
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::new("media-canvas"), canvas);
            data.insert_temp(egui::Id::new("media-panel"), panel);
        });
        ui.painter()
            .rect_filled(panel, 5.0, Color32::from_black_alpha(190));
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("media-controls")
                .max_rect(panel.shrink(6.0)),
            |ui| {
                ui.set_clip_rect(canvas.intersect(ui.clip_rect()));
                let mut focused = false;
                let mut position = if stepping {
                    self.confirmed_step_position
                        .or_else(|| self.snapshot["position_ms"].as_u64())
                        .unwrap_or(0)
                } else {
                    self.snapshot["position_ms"].as_u64().unwrap_or(0)
                };
                let duration = self.snapshot["duration_ms"].as_u64().unwrap_or(0);
                if self.session.is_some() && duration > 0 {
                    position = self
                        .seek_position
                        .or_else(|| self.pending_seek.as_ref().map(|seek| seek.position))
                        .unwrap_or(position)
                        .min(duration);
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::new("media-position"), position)
                    });
                    let width = ui.available_width().max(1.0);
                    ui.spacing_mut().slider_width = width;
                    let response = ui.add_sized(
                        [width, ui.spacing().interact_size.y.max(16.0)],
                        egui::Slider::new(&mut position, 0..=duration).show_value(false),
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::new("media-seek"), response.rect)
                    });
                    focused |= response.has_focus();
                    if response.dragged() {
                        self.seek_position = Some(position);
                    }
                    if response.drag_stopped() {
                        let position_ms = self.seek_position.take().unwrap_or(position);
                        self.control(Action::Seek { position_ms }, call);
                    } else if response.changed() && !response.dragged() {
                        self.control(
                            Action::Seek {
                                position_ms: position,
                            },
                            call,
                        );
                    }
                }
                let mut group_rect = egui::Rect::NOTHING;
                let label = if self.session.is_none() {
                    tr("Воспроизвести", "Play")
                } else if !stepping && self.snapshot["state"] == "playing" {
                    tr("Пауза", "Pause")
                } else if self.snapshot["state"] == "ended" {
                    tr("Сначала", "Replay")
                } else {
                    tr("Воспроизвести", "Play")
                };
                let button_font = egui::TextStyle::Button.resolve(ui.style());
                let body_font = egui::TextStyle::Body.resolve(ui.style());
                let time_font = egui::TextStyle::Monospace.resolve(ui.style());
                let measure = |ui: &egui::Ui, text: &str, font: egui::FontId| {
                    ui.painter()
                        .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
                        .size()
                        .x
                };
                let gap = ui.spacing().item_spacing.x;
                let button_width = (measure(ui, label, button_font)
                    + 2.0 * ui.spacing().button_padding.x)
                    .max(ui.spacing().interact_size.x);
                let mut row_width = if self.stopping { 0.0 } else { button_width };
                if busy {
                    row_width += gap + measure(ui, tr("Загрузка…", "Loading…"), body_font.clone());
                }
                if self.session.is_some() && !self.stopping {
                    row_width += 2.0 * gap
                        + measure(
                            ui,
                            &format!("{} / {}", timestamp(position), timestamp(duration)),
                            time_font,
                        )
                        + 130.0;
                }
                row_width = row_width.min(ui.available_width()).max(1.0);
                let (allocated, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Sense::hover(),
                );
                let row = egui::Rect::from_center_size(
                    allocated.center(),
                    egui::vec2(row_width, allocated.height()),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(row)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        if self.session.is_none() {
                            let play = ui
                                .add_enabled(!busy, egui::Button::new(tr("Воспроизвести", "Play")));
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::new("media-play"), play.rect)
                            });
                            group_rect = group_rect.union(play.rect);
                            focused |= play.has_focus();
                            if play.clicked() {
                                self.start(target, call);
                                ui.ctx().request_repaint_after(Duration::from_millis(100));
                            }
                        } else if !self.stopping {
                            let playing = !stepping && self.snapshot["state"] == "playing";
                            let button = ui.button(if playing {
                                tr("Пауза", "Pause")
                            } else if self.snapshot["state"] == "ended" {
                                tr("Сначала", "Replay")
                            } else {
                                tr("Воспроизвести", "Play")
                            });
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::new("media-transport-playing"), playing);
                                data.insert_temp(egui::Id::new("media-button"), button.rect)
                            });
                            group_rect = group_rect.union(button.rect);
                            focused |= button.has_focus();
                            if button.clicked() {
                                self.toggle_pause(call);
                            }
                        }
                        if busy {
                            ui.weak(tr("Загрузка…", "Loading…"));
                        }
                        if self.session.is_some() && !self.stopping {
                            group_rect = group_rect.union(
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} / {}",
                                        timestamp(position),
                                        timestamp(duration)
                                    ))
                                    .monospace(),
                                )
                                .rect,
                            );
                            let mut volume = self.snapshot["volume"]
                                .as_f64()
                                .unwrap_or(1.0)
                                .clamp(0.0, 1.0);
                            let volume_width = 130.0_f32.min(ui.available_width().max(1.0));
                            ui.spacing_mut().slider_width = volume_width;
                            let response = ui.add_sized(
                                [volume_width, 18.0],
                                egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false),
                            );
                            let response = response.on_hover_text(tr("Громкость", "Volume"));
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::new("media-volume"), response.rect)
                            });
                            group_rect = group_rect.union(response.rect);
                            focused |= response.has_focus();
                            if response.changed() {
                                self.control(Action::SetVolume { volume }, call);
                            }
                        }
                    },
                );
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|data| data.insert_temp(egui::Id::new("media-group"), group_rect));
                self.controls_focused = focused;
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            },
        );
    }
}

fn controls_visible(
    state: Option<&str>,
    busy: bool,
    error: bool,
    focused: bool,
    last: Option<Instant>,
    now: Instant,
) -> bool {
    busy || error
        || focused
        || state != Some("playing")
        || last.is_none_or(|last| now.saturating_duration_since(last) < Duration::from_secs(2))
}

fn timestamp(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn decode_frame(value: &Value, previous: u64) -> Option<(u64, egui::ColorImage)> {
    let serial = value["serial"].as_u64()?;
    let width = usize::try_from(value["width"].as_u64()?).ok()?;
    let height = usize::try_from(value["height"].as_u64()?).ok()?;
    if serial <= previous || width == 0 || height == 0 || width > 1280 || height > 720 {
        return None;
    }
    let bytes = width.checked_mul(height)?.checked_mul(4)?;
    let encoded = value["rgba_base64"].as_str()?;
    if encoded.len() != bytes.checked_add(2)?.checked_div(3)?.checked_mul(4)? {
        return None;
    }
    let rgba = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    if rgba.len() != bytes {
        return None;
    }
    Some((
        serial,
        egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn backward_at_start_of_playing_video_still_requests_pause_step() {
        let ctx = egui::Context::default();
        let target = t("details");
        let mut state = State {
            target: Some(target),
            session: Some(7),
            snapshot: session(7, "playing")["snapshot"].clone(),
            ..Default::default()
        };
        let mut count = 0;
        let mut call = |command| match command {
            Command::MediaState => ok(session(7, "playing")),
            Command::MediaFrame { .. } => ok(json!({"session":7,"frame":null})),
            Command::ControlMedia {
                action: Action::StepFrame { backwards: true },
                ..
            } => {
                count += 1;
                ok(session(7, "paused"))
            }
            _ => panic!("unexpected command"),
        };
        let mut output = ctx.run_ui(Default::default(), |ui| {
            state.handle_frame_hold(ui.ctx(), Some(true), &mut call)
        });
        output.textures_delta.clear();
        assert_eq!(count, 1);
    }
    #[test]
    fn pause_toggle_uses_acknowledged_media_session() {
        let mut state = State {
            session: Some(8),
            snapshot: json!({"state":"playing"}),
            ..Default::default()
        };
        let mut actions = Vec::new();
        let mut call = |command| match command {
            Command::ControlMedia {
                session: 8,
                action: Action::Pause,
            } => {
                actions.push("pause");
                ok(session(8, "paused"))
            }
            Command::ControlMedia {
                session: 8,
                action: Action::Play,
            } => {
                actions.push("play");
                ok(session(8, "playing"))
            }
            _ => panic!("Unexpected pause command"),
        };
        assert!(state.toggle_pause(&mut call));
        assert!(state.toggle_pause(&mut call));
        assert_eq!(actions, ["pause", "play"]);
    }

    fn t(context: &str) -> Target {
        Target {
            id: 5,
            revision: "r1".into(),
            context: context.into(),
        }
    }
    fn session(token: u64, state: &str) -> Value {
        json!({"token":token,"identity":{"id":5,"revision":"r1","context":"domain"},"snapshot":{"state":state,"duration_ms":5000,"position_ms":0,"volume":1.0,"has_audio":true,"width":1,"height":1,"error":null}})
    }
    fn ok(value: Value) -> Value {
        json!({"ok":true,"result":value})
    }

    #[test]
    fn stepping_keeps_actual_controls_stable_until_confirmed_then_autohides() {
        let ctx = egui::Context::default();
        let target = t(r#"["images",null,"details"]"#);
        let mut state = State {
            target: Some(target.clone()),
            session: Some(7),
            snapshot: session(7, "paused")["snapshot"].clone(),
            serial: 1,
            frame_hold: Some(false),
            frame_wait_serial: Some(1),
            confirmed_step_position: Some(1000),
            last_activity: Some(Instant::now() - Duration::from_secs(5)),
            ..Default::default()
        };
        let mut call = |command| match command {
            Command::MediaState => ok(session(7, "playing")),
            Command::MediaFrame { .. } => ok(json!({"session":7,"frame":null})),
            _ => panic!("unexpected command"),
        };
        let mut previous = None;
        for (playback, position) in [("paused", 1000), ("playing", 1900), ("paused", 1100)] {
            state.snapshot["state"] = json!(playback);
            state.snapshot["position_ms"] = json!(position);
            state.snapshot["step_pending"] = json!(true);
            state.next_poll = Some(Instant::now() + Duration::from_secs(60));
            ctx.data_mut(|data| {
                data.remove::<egui::Rect>(egui::Id::new("media-panel"));
            });
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 300.0),
                    )),
                    ..Default::default()
                },
                |ui| state.show(ui, &target, &mut call),
            );
            out.textures_delta.clear();
            let (panel, seek, button, playing): (egui::Rect, egui::Rect, egui::Rect, bool) = ctx
                .data(|data| {
                    (
                        data.get_temp(egui::Id::new("media-panel")).unwrap(),
                        data.get_temp(egui::Id::new("media-seek")).unwrap(),
                        data.get_temp(egui::Id::new("media-button")).unwrap(),
                        data.get_temp(egui::Id::new("media-transport-playing"))
                            .unwrap(),
                    )
                });
            assert!(!playing);
            assert_eq!(
                ctx.data(|data| data.get_temp::<u64>(egui::Id::new("media-position"))),
                Some(1000)
            );
            if let Some((old_panel, old_seek, old_button)) = previous {
                assert_eq!((panel, seek, button), (old_panel, old_seek, old_button));
            }
            previous = Some((panel, seek, button));
        }
        state.serial = 2;
        state.next_poll = None;
        let mut out = ctx.run_ui(Default::default(), |ui| {
            state.sync(ui.ctx(), Some(target.clone()), &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(state.confirmed_step_position, Some(0));
        state.frame_wait_serial = Some(2);
        state.cancel_frame_hold();
        state.serial = 3;
        state.next_poll = None;
        let mut out = ctx.run_ui(Default::default(), |ui| {
            state.sync(ui.ctx(), Some(target.clone()), &mut call)
        });
        out.textures_delta.clear();
        assert!(state.frame_wait_serial.is_none());
        assert!(state.confirmed_step_position.is_none());
        state.last_activity = Some(Instant::now() - Duration::from_secs(5));
        ctx.data_mut(|data| {
            data.remove::<egui::Rect>(egui::Id::new("media-panel"));
        });
        let mut out = ctx.run_ui(Default::default(), |ui| state.show(ui, &target, &mut call));
        out.textures_delta.clear();
        assert!(
            ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new("media-panel")))
                .is_none()
        );
    }

    #[test]
    fn held_steps_wait_for_completion_and_new_frame_and_release_stops_repeating() {
        let ctx = egui::Context::default();
        let target = t(r#"["images",null,"details"]"#);
        let mut state = State {
            target: Some(target),
            session: Some(7),
            snapshot: session(7, "paused")["snapshot"].clone(),
            serial: 1,
            ..Default::default()
        };
        state.snapshot["position_ms"] = json!(1000);
        let pending = std::cell::Cell::new(false);
        let serial = std::cell::Cell::new(1u64);
        let steps = std::cell::RefCell::new(Vec::new());
        let mut call = |command| match command {
            Command::MediaState => {
                let mut value = session(7, "paused");
                value["snapshot"]["position_ms"] = json!(1000);
                value["snapshot"]["step_pending"] = json!(pending.get());
                ok(value)
            }
            Command::MediaFrame { after_serial, .. } => ok(
                json!({"session":7,"frame": if serial.get() > after_serial { json!({"serial":serial.get(),"width":1,"height":1,"rgba_base64":"AQIDBA=="}) } else { Value::Null }}),
            ),
            Command::ControlMedia {
                action: Action::StepFrame { backwards },
                ..
            } => {
                steps.borrow_mut().push(backwards);
                pending.set(true);
                let mut value = session(7, "paused");
                value["snapshot"]["position_ms"] = json!(1000);
                value["snapshot"]["step_pending"] = json!(true);
                ok(value)
            }
            _ => panic!("unexpected command"),
        };
        for _ in 0..3 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                state.handle_frame_hold(ui.ctx(), Some(false), &mut call)
            });
            out.textures_delta.clear();
        }
        assert_eq!(&*steps.borrow(), &[false]);
        pending.set(false);
        let mut out = ctx.run_ui(Default::default(), |ui| {
            state.handle_frame_hold(ui.ctx(), Some(true), &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(&*steps.borrow(), &[false]);
        serial.set(2);
        let mut out = ctx.run_ui(Default::default(), |ui| {
            state.handle_frame_hold(ui.ctx(), Some(true), &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(&*steps.borrow(), &[false, true]);
        state.cancel_frame_hold();
        pending.set(false);
        let mut out = ctx.run_ui(Default::default(), |ui| {
            state.handle_frame_hold(ui.ctx(), None, &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(&*steps.borrow(), &[false, true]);
    }

    #[test]
    fn relative_seek_accumulates_against_stale_ack_and_clamps() {
        let mut state = State {
            session: Some(7),
            snapshot: session(7, "paused")["snapshot"].clone(),
            ..Default::default()
        };
        state.snapshot["position_ms"] = json!(2000);
        let mut positions = Vec::new();
        let mut call = |command| match command {
            Command::ControlMedia {
                action: Action::Seek { position_ms },
                ..
            } => {
                positions.push(position_ms);
                ok(session(7, "paused"))
            }
            _ => panic!("unexpected command"),
        };
        assert!(state.seek_by(1000, &mut call));
        assert!(state.seek_by(1000, &mut call));
        assert!(state.seek_by(10000, &mut call));
        assert!(state.seek_by(-10000, &mut call));
        assert_eq!(positions, [3000, 4000, 5000, 0]);
        assert_eq!(state.snapshot["state"], "paused");
        assert_eq!(state.pending_seek.as_ref().unwrap().position, 0);
        state.session = None;
        assert!(!state.seek_by(1000, &mut |_| panic!("no session")));
    }

    #[test]
    fn seek_remains_visible_through_stale_ack_and_polls_then_confirms_or_fails() {
        let mut state = State {
            session: Some(7),
            snapshot: session(7, "playing")["snapshot"].clone(),
            ..Default::default()
        };
        state.snapshot["position_ms"] = json!(1000);
        let mut call = |command| match command {
            Command::ControlMedia {
                action: Action::Seek { .. },
                ..
            } => ok(session(7, "playing")),
            _ => ok(Value::Null),
        };
        assert!(state.control(Action::Seek { position_ms: 4000 }, &mut call));
        assert_eq!(state.pending_seek.as_ref().unwrap().position, 4000);
        state.snapshot["position_ms"] = json!(1000);
        state.observe_seek();
        assert!(state.pending_seek.is_some());
        // A second seek replaces the first, including a backwards seek.
        assert!(state.control(Action::Seek { position_ms: 500 }, &mut call));
        state.snapshot["position_ms"] = json!(4000);
        state.observe_seek();
        assert_eq!(state.pending_seek.as_ref().unwrap().position, 500);
        state.snapshot["position_ms"] = json!(550);
        state.observe_seek();
        assert!(state.pending_seek.is_none());
        assert!(state.control(Action::Seek { position_ms: 4900 }, &mut call));
        state.snapshot["position_ms"] = json!(5000);
        state.snapshot["state"] = json!("ended");
        state.observe_seek();
        assert!(state.pending_seek.is_none());
        let mut failure = |_| json!({"ok":false,"error":{"code":"seek_failed","message":"failed"}});
        assert!(!state.control(Action::Seek { position_ms: 3000 }, &mut failure));
        assert!(state.pending_seek.is_none() && state.error.is_some());
        state.pending_seek = Some(PendingSeek {
            position: 4000,
            issued: Instant::now() - Duration::from_secs(5),
        });
        state.observe_seek();
        assert!(state.pending_seek.is_none() && state.error.is_some());
        state.pending_seek = Some(PendingSeek {
            position: 4900,
            issued: Instant::now(),
        });
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            state.sync(ui.ctx(), Some(t("changed")), &mut call)
        });
        out.textures_delta.clear();
        assert!(state.pending_seek.is_none());
    }

    #[test]
    fn large_controls_fit_inside_video_canvas() {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| {
            style.spacing.interact_size.y = 45.0;
            style.spacing.button_padding.y = 10.0;
        });
        let target = t(r#"["images",null,"details"]"#);
        let mut state = State {
            target: Some(target.clone()),
            session: Some(7),
            snapshot: session(7, "paused")["snapshot"].clone(),
            next_poll: Some(Instant::now() + Duration::from_secs(60)),
            ..Default::default()
        };
        let mut call = |_| ok(Value::Null);
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 300.0),
                    )),
                    ..Default::default()
                },
                |ui| state.show(ui, &target, &mut call),
            );
            out.textures_delta.clear();
        }
        ctx.data(|data| {
            let canvas: egui::Rect = data.get_temp(egui::Id::new("media-canvas")).unwrap();
            let panel: egui::Rect = data.get_temp(egui::Id::new("media-panel")).unwrap();
            for id in ["media-seek", "media-button", "media-volume"] {
                let rect: egui::Rect = data.get_temp(egui::Id::new(id)).unwrap();
                assert!(
                    panel.contains_rect(rect) && canvas.contains_rect(rect),
                    "{id}: {rect:?}, panel {panel:?}"
                );
            }
            assert!(panel.bottom() < canvas.bottom());
        });
    }

    #[test]
    fn full_preview_autostarts_once_and_failure_does_not_retry() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        let target = t(r#"["images",null,"preview"]"#);
        let mut opens = 0;
        let mut call = |command: Command| {
            if matches!(command, Command::BeginJob { .. }) {
                opens += 1;
            }
            json!({"ok":false,"error":{"code":"unavailable","message":"unavailable"}})
        };
        for _ in 0..3 {
            let mut out = ctx.run_ui(Default::default(), |ui| state.show(ui, &target, &mut call));
            out.textures_delta.clear();
        }
        assert_eq!(opens, 1);
        assert!(state.error.is_some());
    }

    #[test]
    fn controls_hide_only_during_idle_play_and_stay_within_canvas() {
        let now = Instant::now();
        let old = Some(now - Duration::from_secs(3));
        assert!(!controls_visible(
            Some("playing"),
            false,
            false,
            false,
            old,
            now
        ));
        assert!(controls_visible(
            Some("paused"),
            false,
            false,
            false,
            old,
            now
        ));
        assert!(controls_visible(
            Some("playing"),
            true,
            false,
            false,
            old,
            now
        ));
        assert!(controls_visible(
            Some("playing"),
            false,
            true,
            false,
            old,
            now
        ));
        assert!(controls_visible(
            Some("playing"),
            false,
            false,
            true,
            old,
            now
        ));
        assert!(controls_visible(
            Some("playing"),
            false,
            false,
            false,
            Some(now),
            now
        ));
        let ctx = egui::Context::default();
        let mut state = State::default();
        let target = t(r#"["images",null,"details"]"#);
        let mut call = |_| ok(Value::Null);
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 250.0),
                )),
                ..Default::default()
            },
            |ui| state.show(ui, &target, &mut call),
        );
        out.textures_delta.clear();
        let (canvas, panel): (egui::Rect, egui::Rect) = ctx.data(|data| {
            (
                data.get_temp(egui::Id::new("media-canvas")).unwrap(),
                data.get_temp(egui::Id::new("media-panel")).unwrap(),
            )
        });
        assert!(canvas.contains_rect(panel));
        assert!(canvas.height() > 200.0);
        assert_eq!(panel.width(), canvas.width());
        state.target = Some(target.clone());
        state.session = Some(7);
        state.snapshot = session(7, "paused")["snapshot"].clone();
        state.next_poll = Some(Instant::now() + Duration::from_secs(60));
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 250.0),
                    )),
                    ..Default::default()
                },
                |ui| state.show(ui, &target, &mut call),
            );
            out.textures_delta.clear();
        }
        let (seek, group): (egui::Rect, egui::Rect) = ctx.data(|data| {
            (
                data.get_temp(egui::Id::new("media-seek")).unwrap(),
                data.get_temp(egui::Id::new("media-group")).unwrap(),
            )
        });
        assert!(
            seek.width() >= canvas.width() - 14.0,
            "seek {seek:?}, canvas {canvas:?}"
        );
        assert!(
            (group.center().x - canvas.center().x).abs() < 2.0,
            "group {group:?}, canvas {canvas:?}"
        );
    }

    #[test]
    fn mini_view_does_not_open_until_explicit_play() {
        {
            let context = r#"["images",null,"details"]"#;
            let ctx = egui::Context::default();
            let mut state = State::default();
            let target = t(context);
            let calls = std::cell::RefCell::new(Vec::new());
            let mut call = |command: Command| {
                match command {
                    Command::BeginJob { .. } => calls.borrow_mut().push("open"),
                    _ => calls.borrow_mut().push("other"),
                }
                ok(json!({"state":"queued","job_id":7}))
            };
            for _ in 0..2 {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(500.0, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| state.show(ui, &target, &mut call),
                );
                out.textures_delta.clear();
            }
            assert!(calls.borrow().is_empty());
            let rect: egui::Rect = ctx.data(|d| d.get_temp(egui::Id::new("media-play")).unwrap());
            for pressed in [true, false] {
                let pos = rect.center();
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(500.0, 500.0),
                        )),
                        events: vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| state.show(ui, &target, &mut call),
                );
                out.textures_delta.clear();
            }
            assert!(state.pending.is_some());
            assert_eq!(*calls.borrow(), ["open"]);
        }
    }

    #[test]
    fn changed_context_cancels_before_poll_and_stops_late_session() {
        let ctx = egui::Context::default();
        let mut state = State {
            target: Some(t("preview")),
            pending: Some(Pending {
                id: JobId(1),
                target: t("preview"),
                cancelled: false,
                cancel_sent: false,
            }),
            ..Default::default()
        };
        let mut calls = Vec::new();
        let mut call = |command: Command| match command {
            Command::CancelJob { .. } => {
                calls.push("cancel");
                json!({"ok":false,"error":{"code":"job_not_cancellable"}})
            }
            Command::PollJob { .. } => {
                calls.push("poll");
                ok(json!({"state":"succeeded","result":session(9,"paused")}))
            }
            Command::ControlMedia {
                action: Action::Stop,
                ..
            } => {
                calls.push("stop");
                ok(Value::Null)
            }
            _ => panic!("unexpected media command"),
        };
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {
            state.sync(&ctx, Some(t("details")), &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(calls, ["cancel", "poll", "stop"]);
        assert!(state.pending.is_none() && state.session.is_none());
    }

    #[test]
    fn watchdog_stop_is_recoverable_and_selected_volume_precedes_play() {
        let ctx = egui::Context::default();
        let mut state = State {
            target: Some(t("details")),
            session: Some(4),
            ..Default::default()
        };
        let mut calls = Vec::new();
        let mut call = |command: Command| match command {
            Command::MediaState => {
                calls.push("state");
                ok(session(4, "stopped"))
            }
            Command::ControlMedia {
                action: Action::Stop,
                ..
            } => {
                calls.push("stop");
                ok(Value::Null)
            }
            _ => panic!("unexpected command"),
        };
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {
            state.sync(&ctx, Some(t("details")), &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(calls, ["state", "stop"]);
        assert!(state.session.is_none() && state.error.is_some());
        state.chosen_volume = Some(0.25);
        assert!(state.accept_session(&session(8, "paused"), &t("details")));
        let mut calls = Vec::new();
        let mut call = |command: Command| match command {
            Command::ControlMedia {
                session: 8,
                action: Action::SetVolume { volume },
            } => {
                assert_eq!(volume, 0.25);
                calls.push("volume");
                ok(session(8, "paused"))
            }
            Command::ControlMedia {
                session: 8,
                action: Action::Play,
            } => {
                calls.push("play");
                ok(session(8, "playing"))
            }
            _ => panic!("unexpected command"),
        };
        state.play_opened(&mut call);
        assert_eq!(calls, ["volume", "play"]);
    }

    #[test]
    fn frame_decoder_rejects_stale_large_and_invalid_frames() {
        let frame = json!({"serial":2,"width":1,"height":1,"rgba_base64":"AQIDBA=="});
        assert!(decode_frame(&frame, 1).is_some());
        assert!(decode_frame(&frame, 2).is_none());
        assert!(
            decode_frame(
                &json!({"serial":3,"width":1281,"height":1,"rgba_base64":""}),
                1
            )
            .is_none()
        );
        assert!(
            decode_frame(
                &json!({"serial":3,"width":1,"height":1,"rgba_base64":"????????"}),
                1
            )
            .is_none()
        );
        assert!(
            decode_frame(
                &json!({"serial":3,"width":1,"height":1,"rgba_base64":"AQID"}),
                1
            )
            .is_none()
        );
    }

    #[test]
    fn polling_reuses_texture_and_rejects_foreign_session_frame() {
        let ctx = egui::Context::default();
        let mut state = State {
            target: Some(t("preview")),
            session: Some(8),
            ..Default::default()
        };
        let frame_no = std::cell::Cell::new(0u64);
        let mut call = |command: Command| match command {
            Command::MediaState => ok(session(8, "paused")),
            Command::MediaFrame { session: 8, .. } => {
                let next = frame_no.get() + 1;
                frame_no.set(next);
                ok(
                    json!({"session": if next == 3 { 99 } else { 8 }, "frame":{"serial":next,"width":1,"height":1,"rgba_base64":"AQIDBA=="}}),
                )
            }
            _ => panic!("unexpected command"),
        };
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {
            state.sync(&ctx, Some(t("preview")), &mut call)
        });
        out.textures_delta.clear();
        let id = state.texture.as_ref().unwrap().id();
        state.next_poll = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {
            state.sync(&ctx, Some(t("preview")), &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(state.texture.as_ref().unwrap().id(), id);
        assert_eq!(state.serial, 2);
        state.next_poll = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {
            state.sync(&ctx, Some(t("preview")), &mut call)
        });
        out.textures_delta.clear();
        assert_eq!(state.serial, 2);
        assert_eq!(state.texture.as_ref().unwrap().id(), id);
    }

    #[test]
    fn target_routing_uses_video_path_but_excludes_nonvideo_and_missing() {
        let state = json!({"tab":"desktop","folder_path":null});
        let event = json!({"id":-42,"revision":"r1","kind":"file_reference","name":"wallpaper","path":"C:/media/video.mp4","available":true});
        let full = target(&event, &state, true).unwrap();
        let details = target(&event, &state, false).unwrap();
        assert_ne!(full.context, details.context);
        let mut missing = event.clone();
        missing["available"] = false.into();
        assert!(target(&missing, &state, true).is_none());
        let mut text = event;
        text["kind"] = "text_fragment".into();
        assert!(target(&text, &state, true).is_none());
    }
}

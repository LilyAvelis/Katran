//! Presentation-only global summon client; copy/paste policy lives in Service.
use super::*;
#[derive(Default)]
pub(super) struct State {
    configured: Option<String>,
    #[cfg(windows)]
    listener: Option<katran::overlay_windows::HotkeyListener>,
    pending: bool,
    summoned: bool,
    registration_status: String,
    recorder_suspended: bool,
}
impl Shelf {
    pub(super) fn close_or_hide(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        let resident = self.overlay_ui.listener.is_some();
        #[cfg(not(windows))]
        let resident = false;
        ctx.send_viewport_cmd(if resident {
            egui::ViewportCommand::Visible(false)
        } else {
            egui::ViewportCommand::Close
        });
        self.overlay_ui.summoned = false;
    }
    pub(super) fn overlay_escape(&mut self, ctx: &egui::Context) {
        if self.overlay_ui.summoned
            && !self.overlay_ui.pending
            && self.state["preview"].is_null()
            && !self.actions
            && !self.open_apps.open
            && !self.workspace.open
            && !self.info.is_open()
            && !self.pinned_ui.open
            && !self.save
            && self.pending_delete.is_empty()
            && self.shortcut_editor.is_none()
            && self.state["settings_open"] != true
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.close_or_hide(ctx);
        }
    }
    pub(super) fn overlay_tick(&mut self, ctx: &egui::Context) {
        if self.api.is_none() || self.native_window == 0 {
            return;
        }
        if self.settings_ui.is_recording_overlay() {
            #[cfg(windows)]
            {
                self.overlay_ui.listener = None;
            }
            self.overlay_ui.recorder_suspended = true;
            self.settings_ui
                .set_overlay_status("Нажмите новое сочетание клавиш".into());
            return;
        }
        if self.overlay_ui.recorder_suspended {
            self.overlay_ui.recorder_suspended = false;
            self.overlay_ui.configured = None;
        }
        let binding = self.call(Command::GetSettings)["overlay_hotkey"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_owned();
        let retry = self.settings_ui.take_overlay_retry();
        if retry || self.overlay_ui.configured.as_deref() != Some(&binding) {
            self.overlay_ui.configured = Some(binding.clone());
            #[cfg(windows)]
            {
                self.overlay_ui.listener = None;
                self.overlay_ui.registration_status = "Глобальная клавиша выключена".into();
                if !binding.is_empty() {
                    let wake = ctx.clone();
                    match katran::overlay::Hotkey::parse(&binding).and_then(|key| {
                        katran::overlay_windows::HotkeyListener::start(key, move || {
                            wake.request_repaint()
                        })
                    }) {
                        Ok(listener) => {
                            self.overlay_ui.listener = Some(listener);
                            self.overlay_ui.registration_status = format!("Активно: {binding}");
                        }
                        Err(error) => {
                            self.message = format!(
                                "Глобальная клавиша: {}. Возможно, сочетание занято другим приложением.",
                                error.message
                            );
                            self.overlay_ui.registration_status = self.message.clone();
                        }
                    }
                }
            }
        }
        #[cfg(windows)]
        if let Some(listener) = self.overlay_ui.listener.as_ref() {
            match listener.try_recv_capture() {
                Ok(Some(Ok(capture))) => {
                    let reply = self.call(Command::SummonCapturedOverlay {
                        owner_window: self.native_window,
                        capture,
                    });
                    if let (Ok(target), Ok(area)) = (
                        serde_json::from_value(reply["previous"].clone()),
                        serde_json::from_value(reply["work_area"].clone()),
                    ) {
                        match katran::overlay_windows::summon_window(
                            self.native_window,
                            target,
                            area,
                        ) {
                            Ok(()) => {
                                self.overlay_ui.summoned = true;
                                self.message.clear();
                                if let Some(log) = &self.logger {
                                    log.record(
                                        "overlay_shown",
                                        serde_json::json!({"owner":self.native_window}),
                                    );
                                }
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                                ctx.request_repaint();
                            }
                            Err(error) => {
                                if let Some(log) = &self.logger {
                                    log.record("overlay_show_failed",serde_json::json!({"code":error.code,"message":error.message}));
                                }
                                self.message =
                                    format!("Не удалось вызвать Katran: {}", error.message);
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                                ctx.request_repaint();
                            }
                        }
                    }
                }
                Ok(None) => {}
                Ok(Some(Err(error))) => {
                    self.message = format!("Вызов Katran: {}", error.message);
                }
                Err(error) => {
                    self.message = error.message;
                    self.overlay_ui.registration_status = self.message.clone();
                    self.overlay_ui.listener = None;
                }
            }
        }
        self.settings_ui
            .set_overlay_status(self.overlay_ui.registration_status.clone());
        if self.overlay_ui.pending {
            let reply = self.call(Command::CompletePastePrevious {
                owner_window: self.native_window,
            });
            if reply["state"] == "copying" {
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                self.overlay_ui.pending = false;
                if reply["state"] == "sent" {
                    if let Some(log) = &self.logger {
                        log.record(
                            "overlay_hidden",
                            serde_json::json!({"reason":"paste_sent","resident":true}),
                        );
                    }
                    self.overlay_ui.summoned = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
            }
        }
    }
    pub(super) fn paste_previous(&mut self, ctx: &egui::Context) {
        if self.job.is_some() || self.overlay_ui.pending {
            self.message = "Дождитесь завершения текущей операции".into();
            return;
        }
        let reply = self.call(Command::BeginPastePrevious {
            owner_window: self.native_window,
        });
        if reply["state"] == "copying" {
            self.overlay_ui.pending = true;
            self.actions = false;
            ctx.request_repaint();
        }
    }
}

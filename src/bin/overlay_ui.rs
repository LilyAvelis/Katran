//! Presentation-only global summon client; copy/paste policy lives in Service.
use super::i18n::tr;
use super::*;
#[derive(Default)]
pub(super) struct State {
    pub(super) animation: summon_animation::State,
    configured: Option<String>,
    #[cfg(windows)]
    listener: Option<katran::overlay_windows::HotkeyListener>,
    #[cfg(windows)]
    tray: Option<katran::tray::Listener>,
    tray_attempted: bool,
    pub(super) pending: bool,
    summoned: bool,
    registration_status: String,
    recorder_suspended: bool,
    registration_retry_at: Option<Instant>,
    quitting: bool,
}
impl Shelf {
    pub(super) fn quit_application(&mut self, ctx: &egui::Context) {
        self.overlay_ui.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub(super) fn resident_close_requested(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        let resident = self.overlay_ui.tray.is_some() || self.overlay_ui.listener.is_some();
        #[cfg(not(windows))]
        let resident = false;
        if resident && !self.overlay_ui.quitting && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.apply_window_visibility(ctx, false);
        }
    }
    pub(super) fn apply_window_visibility(&mut self, ctx: &egui::Context, visible: bool) {
        self.call(Command::SetWindowVisibility { visible });
        let theme = serde_json::from_value::<katran::theme::ThemePreferences>(
            self.call(Command::GetSettings)["theme"].clone(),
        )
        .unwrap_or_default();
        if !visible
            && self.overlay_ui.animation.start_hide_with(
                self.native_window,
                &theme.animation_hide_style,
                theme.animation_hide_ms.into(),
            )
        {
            self.overlay_ui.summoned = false;
            ctx.request_repaint();
            return;
        }
        if visible {
            self.overlay_ui.animation.cancel(self.native_window);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        } else {
            self.overlay_ui.summoned = false;
        }
    }
    pub(super) fn tray_tick(&mut self, ctx: &egui::Context) {
        if self.api.is_none() || self.native_window == 0 {
            return;
        }
        if !self.overlay_ui.tray_attempted {
            self.overlay_ui.tray_attempted = true;
            #[cfg(all(windows, not(test)))]
            {
                let wake = ctx.clone();
                let language = if super::i18n::is_english() {
                    katran::settings::Language::English
                } else {
                    katran::settings::Language::Russian
                };
                match katran::tray::Listener::start(language, move || wake.request_repaint()) {
                    Ok(tray) => self.overlay_ui.tray = Some(tray),
                    Err(error) => {
                        self.message = format!("{}: {error}", tr("Трей Katran", "Katran tray"))
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            let mut events = Vec::new();
            if let Some(tray) = &self.overlay_ui.tray {
                tray.set_language(if super::i18n::is_english() {
                    katran::settings::Language::English
                } else {
                    katran::settings::Language::Russian
                });
                for _ in 0..8 {
                    match tray.try_recv() {
                        Ok(Some(event)) => events.push(event),
                        Ok(None) => break,
                        Err(error) => {
                            self.message = error;
                            break;
                        }
                    }
                }
            }
            for event in events {
                match event {
                    katran::tray::Event::Show => self.apply_window_visibility(ctx, true),
                    katran::tray::Event::Hide => self.apply_window_visibility(ctx, false),
                    katran::tray::Event::Toggle => {
                        let result = self.call(Command::ToggleWindowVisibility);
                        if let Some(visible) = result["visible"].as_bool() {
                            self.apply_window_visibility(ctx, visible);
                        }
                    }
                    katran::tray::Event::Quit => self.run("quit".into(), ctx),
                }
            }
        }
    }
    pub(super) fn close_or_hide(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        let resident = self.overlay_ui.listener.is_some() || self.overlay_ui.tray.is_some();
        #[cfg(not(windows))]
        let resident = false;
        if resident {
            self.apply_window_visibility(ctx, false);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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
            self.settings_ui.set_overlay_status(
                tr("Нажмите новое сочетание клавиш", "Press a new shortcut").into(),
            );
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
        let timed_retry = self
            .overlay_ui
            .registration_retry_at
            .is_some_and(|at| Instant::now() >= at);
        if retry || timed_retry || self.overlay_ui.configured.as_deref() != Some(&binding) {
            self.overlay_ui.registration_retry_at = None;
            self.overlay_ui.configured = Some(binding.clone());
            #[cfg(windows)]
            {
                self.overlay_ui.listener = None;
                self.overlay_ui.registration_status =
                    tr("Глобальная клавиша выключена", "Global shortcut disabled").into();
                if !binding.is_empty() {
                    let wake = ctx.clone();
                    match katran::overlay::Hotkey::parse(&binding).and_then(|key| {
                        katran::overlay_windows::HotkeyListener::start(key, move || {
                            wake.request_repaint()
                        })
                    }) {
                        Ok(listener) => {
                            self.overlay_ui.listener = Some(listener);
                            self.overlay_ui.registration_status =
                                format!("{}: {binding}", tr("Активно", "Active"));
                            if let Some(log) = &self.logger {
                                log.record(
                                    "hotkey_registered",
                                    serde_json::json!({"binding":binding}),
                                );
                            }
                        }
                        Err(error) => {
                            self.overlay_ui.registration_retry_at =
                                Some(Instant::now() + Duration::from_secs(3));
                            if let Some(log) = &self.logger {
                                log.record("hotkey_registration_failed",serde_json::json!({"binding":binding,"code":error.code,"message":error.message}));
                            }
                            self.message = format!(
                                "{}: {}. {}",
                                tr("Глобальная клавиша", "Global shortcut"),
                                error.message,
                                tr(
                                    "Возможно, сочетание занято другим приложением.",
                                    "The shortcut may be in use by another app."
                                )
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
                    let reply = self.call(Command::ToggleResidentOverlay {
                        owner_window: self.native_window,
                        capture: Some(capture),
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
                                let theme =
                                    serde_json::from_value::<katran::theme::ThemePreferences>(
                                        self.call(Command::GetSettings)["theme"].clone(),
                                    )
                                    .unwrap_or_default();
                                self.overlay_ui.animation.start_with(
                                    self.native_window,
                                    self.video_background.frame_serial(),
                                    self.video_background_source.is_some(),
                                    &theme.animation_show_style,
                                    theme.animation_show_ms.into(),
                                );
                                self.overlay_ui.summoned = true;
                                self.message.clear();
                                if let Some(log) = &self.logger {
                                    log.record(
                                        "overlay_shown",
                                        serde_json::json!({"owner":self.native_window}),
                                    );
                                }
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                                self.call(Command::SetWindowVisibility { visible: true });
                                ctx.request_repaint();
                            }
                            Err(error) => {
                                if let Some(log) = &self.logger {
                                    log.record("overlay_show_failed",serde_json::json!({"code":error.code,"message":error.message}));
                                }
                                self.message = format!(
                                    "{}: {}",
                                    tr("Не удалось вызвать Katran", "Could not show Katran"),
                                    error.message
                                );
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                                self.call(Command::SetWindowVisibility { visible: true });
                                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                                ctx.request_repaint();
                            }
                        }
                    }
                }
                Ok(None) => {}
                Ok(Some(Err(error))) => {
                    if error.code == "self_target" {
                        let reply = self.call(Command::ToggleResidentOverlay {
                            owner_window: self.native_window,
                            capture: None,
                        });
                        if reply["visible"] == false {
                            self.apply_window_visibility(ctx, false);
                        }
                    } else {
                        self.message =
                            format!("{}: {}", tr("Вызов Katran", "Show Katran"), error.message);
                    }
                }
                Err(error) => {
                    self.overlay_ui.registration_retry_at =
                        Some(Instant::now() + Duration::from_secs(3));
                    if let Some(log) = &self.logger {
                        log.record(
                            "hotkey_listener_stopped",
                            serde_json::json!({"code":error.code,"message":error.message}),
                        );
                    }
                    self.message = error.message;
                    self.overlay_ui.registration_status = self.message.clone();
                    self.overlay_ui.listener = None;
                }
            }
        }
        if self.overlay_ui.registration_retry_at.is_some() {
            ctx.request_repaint_after(Duration::from_secs(3));
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
                    self.apply_window_visibility(ctx, false);
                }
            }
        }
    }
    pub(super) fn paste_previous(&mut self, ctx: &egui::Context) {
        if self.job.is_some() || self.overlay_ui.pending {
            self.message = tr(
                "Дождитесь завершения текущей операции",
                "Wait for the current operation to finish",
            )
            .into();
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

//! Per-tab Windows registration and presentation; dispatch owns summon policy.
use super::*;
use katran::{
    interaction::ShelfTab,
    overlay::{OverlayError, SummonReceipt},
};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct State {
    signature: Option<String>,
    status: BTreeMap<ShelfTab, String>,
    retry: Option<Instant>,
    #[cfg(windows)]
    listeners: BTreeMap<ShelfTab, katran::overlay_windows::HotkeyListener>,
}
impl State {
    pub fn statuses(&self) -> BTreeMap<ShelfTab, String> {
        self.status.clone()
    }
    pub fn is_active(&self) -> bool {
        #[cfg(windows)]
        {
            !self.listeners.is_empty()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub fn suspend(&mut self) {
        #[cfg(windows)]
        self.listeners.clear();
        self.signature = None;
        self.retry = None;
        for status in self.status.values_mut() {
            *status = i18n::tr(
                "Запись сочетания — регистрация приостановлена",
                "Recording — registration suspended",
            )
            .into();
        }
    }
    /// Release all obsolete registrations before attempting a swapped assignment.
    pub fn prepare(&mut self, settings: &katran::settings::Settings) -> bool {
        let entries: Vec<_> = settings
            .tab_hotkeys
            .iter()
            .map(|(tab, binding)| (*tab, binding, settings.tab_enabled(*tab)))
            .collect();
        let signature = serde_json::json!([settings.overlay_hotkey, entries]).to_string();
        if self.signature.as_deref() == Some(&signature) {
            return false;
        }
        #[cfg(windows)]
        self.listeners.clear();
        self.status.clear();
        self.retry = None;
        self.signature = Some(signature);
        true
    }
}
impl Shelf {
    pub(super) fn tab_hotkeys_tick(
        &mut self,
        ctx: &egui::Context,
        settings: &katran::settings::Settings,
    ) {
        let retry = self
            .overlay_ui
            .tab_hotkeys
            .retry
            .is_none_or(|at| Instant::now() >= at);
        #[cfg(all(windows, not(test)))]
        let mut failed = false;
        #[cfg(any(not(windows), test))]
        let failed = false;
        for (tab, binding) in &settings.tab_hotkeys {
            if !settings.tab_enabled(*tab) {
                self.overlay_ui.tab_hotkeys.status.insert(
                    *tab,
                    i18n::tr("Неактивно — вкладка отключена", "Inactive — tab disabled").into(),
                );
                continue;
            }
            #[cfg(all(windows, not(test)))]
            if !self.overlay_ui.tab_hotkeys.listeners.contains_key(tab) && retry {
                let wake = ctx.clone();
                match katran::overlay::Hotkey::parse(binding).and_then(|key| {
                    katran::overlay_windows::HotkeyListener::start(key, move || {
                        wake.request_repaint()
                    })
                }) {
                    Ok(listener) => {
                        self.overlay_ui.tab_hotkeys.listeners.insert(*tab, listener);
                        self.overlay_ui.tab_hotkeys.status.insert(
                            *tab,
                            format!("{}: {binding}", i18n::tr("Активно", "Active")),
                        );
                    }
                    Err(error) => {
                        self.overlay_ui.tab_hotkeys.status.insert(
                            *tab,
                            format!(
                                "{}: {} · {}",
                                i18n::tr("Не зарегистрировано", "Not registered"),
                                error.code,
                                i18n::tr(
                                    "Сочетание может быть занято Windows или другим приложением",
                                    "The shortcut may be reserved by Windows or another app"
                                )
                            ),
                        );
                        failed = true;
                    }
                }
            }
            #[cfg(any(not(windows), test))]
            {
                let _ = (binding, retry);
                self.overlay_ui.tab_hotkeys.status.insert(
                    *tab,
                    i18n::tr(
                        "Ожидает регистрации Windows",
                        "Waiting for Windows registration",
                    )
                    .into(),
                );
            }
        }
        if failed {
            self.overlay_ui.tab_hotkeys.retry = Some(Instant::now() + Duration::from_secs(3));
        } else if retry {
            self.overlay_ui.tab_hotkeys.retry = None;
        }
        if let Some(at) = self.overlay_ui.tab_hotkeys.retry {
            ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
        self.settings_ui
            .set_tab_hotkey_statuses(self.overlay_ui.tab_hotkeys.statuses());
        #[cfg(windows)]
        {
            let mut events = Vec::new();
            let mut stopped = Vec::new();
            for (tab, listener) in &self.overlay_ui.tab_hotkeys.listeners {
                match listener.try_recv_capture() {
                    Ok(Some(event)) => events.push((*tab, event)),
                    Ok(None) => {}
                    Err(error) => {
                        stopped.push(*tab);
                        self.overlay_ui
                            .tab_hotkeys
                            .status
                            .insert(*tab, error.message);
                    }
                }
            }
            for tab in stopped {
                self.overlay_ui.tab_hotkeys.listeners.remove(&tab);
                self.overlay_ui.tab_hotkeys.retry = Some(Instant::now() + Duration::from_secs(3));
                ctx.request_repaint_after(Duration::from_secs(3));
            }
            for (tab, event) in events {
                self.summon_tab_hotkey(tab, event, ctx);
            }
        }
    }
    pub(super) fn summon_tab_hotkey(
        &mut self,
        tab: ShelfTab,
        event: Result<SummonReceipt, OverlayError>,
        ctx: &egui::Context,
    ) {
        let capture = match event {
            Ok(capture) => Some(capture),
            Err(error) if error.code == "self_target" => None,
            Err(error) => {
                self.message = error.message;
                return;
            }
        };
        let result = self.call(Command::SummonTabOverlay {
            tab,
            owner_window: self.native_window,
            capture,
        });
        if result["visible"] != true {
            return;
        }
        // Close UI-only overlays through their ordinary cleanup paths.
        self.workspace_close();
        self.pinned_close();
        self.close_open_apps();
        self.actions = false;
        self.sorting_input.close();
        self.tag_menu.close(ctx);
        self.info = Default::default();
        self.pods_ui.selector_open = false;
        self.pods_ui.category_open = false;
        self.save = false;
        self.pending_delete.clear();
        self.query.clear();
        self.state = self.call(Command::Interaction);
        self.refresh();
        #[cfg(all(windows, not(test)))]
        if let (Ok(previous), Ok(area)) = (
            serde_json::from_value(result["capture"]["previous"].clone()),
            serde_json::from_value(result["capture"]["work_area"].clone()),
        ) && let Err(error) =
            katran::overlay_windows::summon_window(self.native_window, previous, area)
        {
            self.message = error.message;
        }
        self.apply_window_visibility(ctx, true);
        self.overlay_ui.summoned = true;
        ctx.request_repaint_of(egui::ViewportId::from_hash_of("katran-settings"));
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_tracks_enablement_and_suspension_without_registering_keys() {
        let mut state = State::default();
        let mut settings = katran::settings::Settings::default();
        settings
            .tab_hotkeys
            .insert(ShelfTab::Images, "Ctrl+Alt+1".into());
        assert!(state.prepare(&settings));
        assert!(!state.prepare(&settings));
        settings.hidden_tabs.push(ShelfTab::Images);
        assert!(state.prepare(&settings));
        state.suspend();
        assert!(state.prepare(&settings));
    }
}

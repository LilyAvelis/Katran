//! Feature switches and navigation are domain policy, shared by every client.
use crate::{ApiError, Command, Result, Service, interaction::ShelfTab};
use serde_json::{Value, json};

pub fn label(tab: ShelfTab) -> &'static str {
    match tab {
        ShelfTab::All => "Folder",
        ShelfTab::Images => "Images",
        ShelfTab::Screenshots => "Screenshots",
        ShelfTab::Downloads => "Downloads",
        ShelfTab::Text => "Text",
        ShelfTab::Links => "Links",
        ShelfTab::Tables => "Tables",
    }
}

impl Service {
    pub(crate) fn action_available(&self, id: &str) -> bool {
        match id {
            "add_to_katran" => self.interaction.tab==ShelfTab::All,
            "images" => self.settings.tab_enabled(ShelfTab::Images),
            "screenshots" => self.settings.tab_enabled(ShelfTab::Screenshots),
            "downloads" => self.settings.tab_enabled(ShelfTab::Downloads),
            "text" => self.settings.tab_enabled(ShelfTab::Text),
            "links" => self.settings.tab_enabled(ShelfTab::Links),
            "tables" => self.settings.tab_enabled(ShelfTab::Tables),
            _ => true,
        }
    }

    pub(crate) fn action_label(&self, id: &str, fallback: &str) -> String {
        let tab = match id {
            "toggle_images" => Some(ShelfTab::Images),
            "toggle_screenshots" => Some(ShelfTab::Screenshots),
            "toggle_downloads" => Some(ShelfTab::Downloads),
            "toggle_text" => Some(ShelfTab::Text),
            "toggle_links" => Some(ShelfTab::Links),
            "toggle_tables" => Some(ShelfTab::Tables),
            _ => None,
        };
        if let Some(tab) = tab {
            format!(
                "{} {}",
                if self.settings.tab_enabled(tab) {
                    "Отключить"
                } else {
                    "Включить"
                },
                label(tab)
            )
        } else if id == "capture" {
            if self.capture_enabled {
                "Приостановить захват"
            } else {
                "Включить захват"
            }
            .into()
        } else {
            fallback.into()
        }
    }

    pub(crate) fn tabs(&self) -> Value {
        let visible = self.settings.visible_tabs();
        let tabs: Vec<_> = std::iter::once(ShelfTab::All)
            .chain(self.settings.tab_order.iter().copied())
            .map(|tab| json!({
                "tab":tab,"label":label(tab),"enabled":self.settings.tab_enabled(tab),
                "can_toggle":tab!=ShelfTab::All,
                "position":if tab==ShelfTab::All {Some(0)} else {visible.iter().position(|t|*t==tab).map(|i|i+1)}
            }))
            .collect();
        json!({"tabs":tabs,"visible_tabs":visible})
    }

    pub(crate) fn set_tab_enabled(&mut self, tab: ShelfTab, enabled: bool) -> Result<Value> {
        if tab == ShelfTab::All {
            return Err(ApiError::new("fixed_tab", "Folder is always enabled"));
        }
        let mut settings = self.settings.clone();
        settings.hidden_tabs.retain(|t| *t != tab);
        if !enabled {
            settings.hidden_tabs.push(tab);
        }
        if tab == ShelfTab::Downloads {
            settings.downloads_enabled = enabled;
        }
        if tab == ShelfTab::Text {
            settings.text_enabled = enabled;
        }
        if tab == ShelfTab::Tables {
            settings.tables_enabled = enabled;
        }
        if tab == ShelfTab::Links {
            settings.links_enabled = enabled;
        }
        self.dispatch(Command::SetSettings { settings })?;
        Ok(self.tabs())
    }

    pub(crate) fn select_tab(&mut self, tab: ShelfTab) -> Result<Value> {
        if !self.settings.tab_enabled(tab) {
            return Err(ApiError::new(
                "tab_disabled",
                "Enable this tab in Settings → Tabs first",
            ));
        }
        self.interaction.set_tab(tab);
        Ok(json!(self.interaction))
    }

    pub(crate) fn normalize_tabs(&mut self) {
        if !self.settings.tab_enabled(self.interaction.tab) {
            self.interaction.set_tab(ShelfTab::All);
            self.interaction.visible(Vec::new());
        }
        if self.interaction.last_tab == ShelfTab::All
            || !self.settings.tab_enabled(self.interaction.last_tab)
        {
            self.interaction.last_tab = self
                .settings
                .visible_tabs()
                .first()
                .copied()
                .unwrap_or(ShelfTab::All);
        }
    }

    pub(crate) fn check_ingest_enabled(&self, name: &str) -> Result<()> {
        if !self.settings.tab_enabled(ShelfTab::Images)
            && crate::file_types::is_native_image(std::path::Path::new(name))
        {
            return Err(ApiError::new(
                "tab_disabled",
                "Images is disabled; image ingestion is stopped",
            ));
        }
        Ok(())
    }
    pub(crate) fn check_links_enabled(&self) -> Result<()> {
        if !self.settings.tab_enabled(ShelfTab::Links) {
            return Err(ApiError::new(
                "tab_disabled",
                "Links is disabled; enable its eye in Settings → Tabs",
            ));
        }
        Ok(())
    }
    pub(crate) fn check_text_enabled(&self) -> Result<()> {
        if !self.settings.tab_enabled(ShelfTab::Text) {
            return Err(ApiError::new(
                "tab_disabled",
                "Text is disabled; enable its eye in Settings → Tabs",
            ));
        }
        Ok(())
    }
}

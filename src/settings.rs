use crate::interaction::View;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub theme: crate::theme::ThemePreferences,
    pub overlay_hotkey: String,
    pub sort_notifications: bool,
    pub pods_tile_size: u16,
    pub sort: crate::sorting::SortSpec,
    #[serde(
        default = "crate::tab_layout::default_order",
        deserialize_with = "crate::tab_layout::deserialize_order"
    )]
    pub tab_order: Vec<crate::interaction::ShelfTab>,
    pub hidden_tabs: Vec<crate::interaction::ShelfTab>,
    pub text_enabled: bool,
    pub links_enabled: bool,
    pub tables_enabled: bool,
    pub pods_enabled: bool,
    pub gallery_columns: u8,
    pub stream_rows: u8,
    pub quota_bytes: u64,
    pub downloads_enabled: bool,
    pub downloads_folder: Option<std::path::PathBuf>,
    pub sharex_folder: Option<std::path::PathBuf>,
    pub card_width: f32,
    pub default_view: View,
    pub capture_on_start: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self::defaults(crate::DEFAULT_QUOTA)
    }
}
impl Settings {
    pub fn defaults(quota: u64) -> Self {
        Self {
            theme: Default::default(),
            overlay_hotkey: String::new(),
            sort_notifications: true,
            pods_tile_size: 96,
            sort: Default::default(),
            quota_bytes: quota,
            gallery_columns: 4,
            stream_rows: 3,
            tab_order: crate::tab_layout::default_order(),
            hidden_tabs: Vec::new(),
            text_enabled: false,
            links_enabled: false,
            tables_enabled: false,
            pods_enabled: false,
            sharex_folder: None,
            downloads_enabled: true,
            downloads_folder: None,
            card_width: 280.0,
            default_view: View::Gallery,
            capture_on_start: true,
        }
    }
    pub fn tab_enabled(&self, tab: crate::interaction::ShelfTab) -> bool {
        use crate::interaction::ShelfTab;
        tab == ShelfTab::All
            || (!self.hidden_tabs.contains(&tab)
                && (tab != ShelfTab::Downloads || self.downloads_enabled)
                && (tab != ShelfTab::Text || self.text_enabled)
                && (tab != ShelfTab::Links || self.links_enabled)
                && (tab != ShelfTab::Tables || self.tables_enabled)
                && (tab != ShelfTab::Pods || self.pods_enabled))
    }
    pub fn visible_tabs(&self) -> Vec<crate::interaction::ShelfTab> {
        self.tab_order
            .iter()
            .copied()
            .filter(|tab| self.tab_enabled(*tab))
            .collect()
    }
    pub fn validate(&self, usage: u64) -> Result<(), String> {
        if self.theme.custom_background.is_some() && self.theme.lily_background.is_some() {
            return Err("Выберите пресет или собственный фон".into());
        }
        if self
            .theme
            .lily_background
            .is_some_and(|n| !(1..=5).contains(&n))
        {
            return Err("Неизвестный фон лилии".into());
        }
        if !(48..=192).contains(&self.pods_tile_size) {
            return Err("Pods: размер плитки должен быть 48–192".into());
        }
        if !self.overlay_hotkey.trim().is_empty() {
            crate::overlay::Hotkey::parse(&self.overlay_hotkey).map_err(|error| error.message)?;
        }
        crate::tab_layout::validate_order(&self.tab_order)?;
        crate::tab_layout::validate_visible(&self.hidden_tabs)?;
        if !(1..=12).contains(&self.gallery_columns) || !(1..=12).contains(&self.stream_rows) {
            return Err("Density must be in 1..12".into());
        }
        if let Some(folder) = &self.sharex_folder
            && !folder.is_absolute()
        {
            return Err("ShareX: укажите абсолютную папку".into());
        }
        if self
            .downloads_folder
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
        {
            return Err("Downloads: absolute folder required".into());
        }
        if self.quota_bytes == 0 || self.quota_bytes > 1024 * 1024 * 1024 * 1024 {
            return Err("Квота должна быть больше нуля и не больше 1 ТиБ".into());
        }
        if self.quota_bytes < usage {
            return Err("Квота меньше текущего кэша: файлы не были удалены".into());
        }
        if !self.card_width.is_finite() || !(160.0..=360.0).contains(&self.card_width) {
            return Err("Размер карточки должен быть от 160 до 360".into());
        }
        Ok(())
    }
}

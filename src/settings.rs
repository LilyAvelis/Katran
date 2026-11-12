use crate::interaction::{ShelfTab, View};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TabPreferences {
    pub sort: crate::sorting::SortSpec,
    pub view: View,
    pub gallery_columns: u8,
    pub stream_rows: u8,
    pub compact_columns: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub theme: crate::theme::ThemePreferences,
    pub overlay_hotkey: String,
    pub emoji_font: String,
    pub sort_notifications: bool,
    /// Legacy preference retained so existing settings documents still load.
    pub pods_tile_size: u16,
    /// Desired count per row for the compact Pods shelf.
    pub pods_columns: u8,
    /// Desired count per row for the compact Links shelf, independent of Pods.
    pub links_columns: u8,
    pub sort: crate::sorting::SortSpec,
    #[serde(default)]
    pub tab_preferences: BTreeMap<ShelfTab, TabPreferences>,
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
    pub search_keyboard_only: bool,
    /// Opt-in network retrieval of site favicons for Links cards.
    pub link_favicons_online: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self::defaults(crate::DEFAULT_QUOTA)
    }
}
impl Settings {
    pub fn defaults(quota: u64) -> Self {
        let mut settings = Self {
            theme: crate::theme::ThemePreferences {
                preset: crate::theme::Preset::BlueExperimental,
                lily_background: Some(1),
                ..Default::default()
            },
            overlay_hotkey: String::new(),
            emoji_font: "apple".into(),
            sort_notifications: true,
            pods_tile_size: 96,
            pods_columns: 9,
            links_columns: 9,
            sort: Default::default(),
            tab_preferences: BTreeMap::new(),
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
            search_keyboard_only: false,
            link_favicons_online: false,
        };
        settings.materialize_tab_preferences();
        for tab in [ShelfTab::Text, ShelfTab::Links] {
            settings.tab_preferences.get_mut(&tab).unwrap().view = View::List;
        }
        settings
    }
    /// Missing entries inherit the legacy preferences, including for hidden tabs.
    pub fn tab_preferences(&self, tab: ShelfTab) -> TabPreferences {
        self.tab_preferences
            .get(&tab)
            .cloned()
            .unwrap_or_else(|| TabPreferences {
                sort: self.sort.normalized(),
                view: self.default_view,
                gallery_columns: self.gallery_columns,
                stream_rows: self.stream_rows,
                compact_columns: if tab == ShelfTab::Links {
                    self.links_columns
                } else {
                    self.pods_columns
                },
            })
    }
    pub fn materialize_tab_preferences(&mut self) {
        for tab in std::iter::once(ShelfTab::All).chain(crate::tab_layout::default_order()) {
            if !self.tab_preferences.contains_key(&tab) {
                let preferences = self.tab_preferences(tab);
                self.tab_preferences.insert(tab, preferences);
            }
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
        if !crate::emoji_fonts::valid_id(&self.emoji_font) {
            return Err("Неизвестный шрифт эмодзи".into());
        }
        if self.theme.selection_intensity > 100 {
            return Err("Интенсивность выделения: от 0 до 100".into());
        }
        if !(25..=300).contains(&self.theme.banana_scroll_speed)
            || !(50..=1000).contains(&self.theme.banana_scroll_inertia_ms)
        {
            return Err("Плавная прокрутка: скорость 25–300%, инерция 50–1000 мс".into());
        }
        for (style, duration) in [
            (
                &self.theme.animation_show_style,
                self.theme.animation_show_ms,
            ),
            (
                &self.theme.animation_hide_style,
                self.theme.animation_hide_ms,
            ),
        ] {
            if !matches!(
                style.as_str(),
                "none" | "slide_down" | "slide_up" | "slide_left" | "slide_right"
            ) || !(50..=800).contains(&duration)
            {
                return Err("Анимация окна: выберите доступный стиль и время 50–800 мс".into());
            }
        }
        if self.theme.optical_strength > 100 {
            return Err("Сила оптического стекла: от 0 до 100".into());
        }
        if [
            self.theme.optical_refraction,
            self.theme.optical_blur,
            self.theme.optical_highlight,
            self.theme.optical_chromatic,
        ]
        .iter()
        .any(|&value| value > 200)
        {
            return Err("Оптические параметры: от 0 до 200".into());
        }
        if [
            self.theme.custom_background.is_some(),
            self.theme.lily_background.is_some(),
            self.theme.active_video().is_some(),
        ]
        .into_iter()
        .filter(|v| *v)
        .count()
            > 1
        {
            return Err("Выберите один фон: изображение, лилии или видео".into());
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
        if !(1..=12).contains(&self.pods_columns) || !(1..=12).contains(&self.links_columns) {
            return Err("Pods/Links: число плиток в строке должно быть 1–12".into());
        }
        if !self.overlay_hotkey.trim().is_empty() {
            crate::overlay::Hotkey::parse(&self.overlay_hotkey).map_err(|error| error.message)?;
        }
        crate::tab_layout::validate_order(&self.tab_order)?;
        crate::tab_layout::validate_visible(&self.hidden_tabs)?;
        if self.tab_preferences.values().any(|prefs| {
            !(1..=12).contains(&prefs.gallery_columns)
                || !(1..=12).contains(&prefs.stream_rows)
                || !(1..=12).contains(&prefs.compact_columns)
        }) {
            return Err("Tab density must be in 1..12".into());
        }
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

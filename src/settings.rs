use crate::interaction::{ShelfTab, View};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Interface language. Wire values are stable ISO language codes.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ru")]
    Russian,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiFont {
    #[default]
    Default,
    SegoeUi,
    Tahoma,
    Verdana,
    Arial,
    Calibri,
    Trebuchet,
}

impl UiFont {
    fn is_default(&self) -> bool {
        *self == Self::Default
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranslationTarget {
    #[default]
    Interface,
    Russian,
    English,
}

impl TranslationTarget {
    fn is_interface(&self) -> bool {
        *self == Self::Interface
    }

    pub fn language(self, interface: Language) -> Language {
        match self {
            Self::Interface => interface,
            Self::Russian => Language::Russian,
            Self::English => Language::English,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn legacy_language() -> Language {
    Language::Russian
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TabPreferences {
    pub sort: crate::sorting::SortSpec,
    pub view: View,
    pub gallery_columns: u8,
    pub stream_rows: u8,
    pub compact_columns: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct NotionReaderPreferences {
    pub landscape: bool,
    pub cinema: bool,
    pub color: [u8; 3],
    pub opacity: u8,
    pub text_percent: u8,
}

impl Default for NotionReaderPreferences {
    fn default() -> Self {
        Self {
            landscape: false,
            cinema: false,
            color: [15, 17, 29],
            opacity: 205,
            text_percent: 100,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct ActionMenu {
    pub groups: Vec<ActionMenuGroup>,
    pub command_visibility: BTreeMap<String, bool>,
}

pub const INTERFACE_SETTINGS_SECTION: usize = 8;

const EXTENSION_KEY: &str = "interface_4_3";
const EXTENSION_FIELDS: [&str; 4] = [
    "action_menu",
    "gallery_reveal",
    "gallery_reveal_ms",
    "snipping_tool_folder",
];

pub(crate) fn read_profile(
    db: &rusqlite::Connection,
    fallback: impl FnOnce() -> Settings,
) -> crate::Result<(Settings, bool)> {
    use rusqlite::OptionalExtension;
    use serde_json::{Map, Value};

    let document = |key| -> crate::Result<Option<Map<String, Value>>> {
        let raw: Option<String> = db
            .query_row(
                "SELECT document FROM preferences WHERE key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        raw.map(|raw| {
            serde_json::from_str(&raw).map_err(|e| crate::ApiError::new("settings_error", e))
        })
        .transpose()
    };
    let Some(mut general) = document("general")? else {
        return Ok((fallback(), false));
    };
    let migrate = EXTENSION_FIELDS
        .iter()
        .any(|field| general.contains_key(*field));
    if let Some(extension) = document(EXTENSION_KEY)? {
        for (key, value) in extension {
            if EXTENSION_FIELDS.contains(&key.as_str()) {
                general.entry(key).or_insert(value);
            }
        }
    }
    let settings = serde_json::from_value(Value::Object(general))
        .map_err(|e| crate::ApiError::new("settings_error", e))?;
    Ok((settings, migrate))
}

pub(crate) fn write_profile(db: &rusqlite::Connection, settings: &Settings) -> crate::Result<()> {
    use serde_json::Value;
    let Value::Object(mut general) =
        serde_json::to_value(settings).map_err(|e| crate::ApiError::new("settings_error", e))?
    else {
        return Err(crate::ApiError::new(
            "settings_error",
            "Expected a settings object",
        ));
    };
    let mut extension = serde_json::Map::new();
    for field in EXTENSION_FIELDS {
        if let Some(value) = general.remove(field) {
            extension.insert(field.into(), value);
        }
    }
    let tx = db.unchecked_transaction()?;
    for (key, fields) in [("general", general), (EXTENSION_KEY, extension)] {
        tx.execute(
            "INSERT INTO preferences(key,document) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET document=excluded.document",
            rusqlite::params![key, Value::Object(fields).to_string()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

impl ActionMenu {
    pub fn is_default(&self) -> bool {
        self.groups.is_empty() && self.command_visibility.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct ActionMenuGroup {
    pub id: String,
    pub title: Option<String>,
    pub hidden: bool,
    pub commands: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GalleryReveal {
    #[default]
    Off,
    Fade,
    Rise,
    Grow,
    Unfold,
}

impl GalleryReveal {
    fn is_off(&self) -> bool {
        *self == Self::Off
    }
}

fn is_default_gallery_duration(value: &u16) -> bool {
    *value == 350
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Old settings without this field retain the Russian interface.
    #[serde(default = "legacy_language")]
    pub language: Language,
    #[serde(skip_serializing_if = "TranslationTarget::is_interface")]
    pub translation_target: TranslationTarget,
    #[serde(skip_serializing_if = "is_false")]
    pub translation_numbered_only: bool,
    pub theme: crate::theme::ThemePreferences,
    pub overlay_hotkey: String,
    /// Global hotkeys are retained even when the associated tab is hidden.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tab_hotkeys: BTreeMap<ShelfTab, String>,
    /// Allow the user to resize the main shelf window. Defaults to a fixed window.
    pub window_resizable: bool,
    /// Last wallpaper installed by Katran; retained even when Desktop tab is hidden.
    pub desktop_wallpaper_id: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snipping_tool_folder: Option<std::path::PathBuf>,
    pub card_width: f32,
    pub default_view: View,
    pub capture_on_start: bool,
    pub search_keyboard_only: bool,
    /// Optional depth and icons for Settings navigation.
    #[serde(skip_serializing_if = "is_false")]
    pub settings_sidebar_depth: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub settings_sidebar_glass: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub settings_summon_popover: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub settings_raised_keycaps: bool,
    /// Missing uses the previous shared preference until a placement is changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_raised_keycaps_in_rows: Option<bool>,
    #[serde(skip_serializing_if = "is_false")]
    pub settings_section_headers: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub settings_section_depth: bool,
    #[serde(skip_serializing_if = "UiFont::is_default")]
    pub ui_font: UiFont,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub settings_section_order: Vec<String>,
    #[serde(skip_serializing_if = "ActionMenu::is_default")]
    pub action_menu: ActionMenu,
    #[serde(skip_serializing_if = "GalleryReveal::is_off")]
    pub gallery_reveal: GalleryReveal,
    #[serde(skip_serializing_if = "is_default_gallery_duration")]
    pub gallery_reveal_ms: u16,
    /// Optional local Folder AI acceleration. Missing means CPU.
    #[serde(skip_serializing_if = "is_false")]
    pub folder_ai_gpu: bool,
    /// Request GPU video decoding/rendering; unsupported systems use CPU.
    #[serde(skip_serializing_if = "is_false")]
    pub video_gpu_acceleration: bool,
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
            language: Language::English,
            translation_target: TranslationTarget::Interface,
            translation_numbered_only: false,
            theme: crate::theme::ThemePreferences {
                preset: crate::theme::Preset::BlueExperimental,
                lily_background: Some(1),
                ..Default::default()
            },
            overlay_hotkey: String::new(),
            tab_hotkeys: BTreeMap::new(),
            window_resizable: false,
            desktop_wallpaper_id: None,
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
            snipping_tool_folder: None,
            downloads_enabled: true,
            downloads_folder: None,
            card_width: 280.0,
            default_view: View::Gallery,
            capture_on_start: true,
            search_keyboard_only: false,
            settings_sidebar_depth: false,
            settings_sidebar_glass: false,
            settings_summon_popover: false,
            settings_raised_keycaps: false,
            settings_raised_keycaps_in_rows: None,
            settings_section_headers: false,
            settings_section_depth: false,
            ui_font: UiFont::Default,
            settings_section_order: Vec::new(),
            action_menu: ActionMenu::default(),
            gallery_reveal: GalleryReveal::Off,
            gallery_reveal_ms: 350,
            link_favicons_online: false,
            folder_ai_gpu: false,
            video_gpu_acceleration: false,
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
    pub(crate) fn hide_excess_numbered_tabs(&mut self) {
        self.hidden_tabs.extend(
            self.visible_tabs()
                .into_iter()
                .skip(crate::tab_layout::MAX_NUMBERED_TABS),
        );
    }
    pub(crate) fn validate_tab_hotkeys(&self) -> crate::overlay::Result<()> {
        let mut seen = Vec::new();
        if !self.overlay_hotkey.trim().is_empty() {
            seen.push(crate::overlay::Hotkey::parse(&self.overlay_hotkey)?);
        }
        for binding in self.tab_hotkeys.values() {
            let hotkey = crate::overlay::Hotkey::parse(binding)?;
            if seen.contains(&hotkey) {
                return Err(crate::overlay::OverlayError::new(
                    "hotkey_conflict",
                    "Global hotkey is already assigned",
                ));
            }
            seen.push(hotkey);
        }
        Ok(())
    }
    pub(crate) fn canonicalize_tab_hotkeys(&mut self) -> crate::overlay::Result<()> {
        self.validate_tab_hotkeys()?;
        for binding in self.tab_hotkeys.values_mut() {
            *binding = crate::overlay::Hotkey::parse(binding)?.canonical();
        }
        Ok(())
    }
    pub fn validate(&self, usage: u64) -> Result<(), String> {
        crate::tab_layout::validate_visible(&self.visible_tabs())?;
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
                "none"
                    | "slide_down"
                    | "slide_up"
                    | "slide_left"
                    | "slide_right"
                    | "zoom"
                    | "fade"
                    | "zoom_fade"
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
        self.validate_tab_hotkeys().map_err(|error| error.message)?;
        if self
            .desktop_wallpaper_id
            .as_ref()
            .is_some_and(|id| id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid managed desktop wallpaper ID".into());
        }
        crate::tab_layout::validate_order(&self.tab_order)?;
        crate::tab_layout::validate_visible(&self.hidden_tabs)?;
        if self
            .tab_preferences
            .get(&ShelfTab::All)
            .is_some_and(|prefs| prefs.sort.mode == crate::sorting::SortMode::Tags)
        {
            return Err("Folder does not support tag sorting".into());
        }
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
            .snipping_tool_folder
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
        {
            return Err("Windows Snipping Tool: absolute folder required".into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_extension_survives_a_legacy_general_settings_write() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE preferences(key TEXT PRIMARY KEY,document TEXT NOT NULL)")
            .unwrap();
        let mut settings = Settings {
            gallery_reveal: GalleryReveal::Grow,
            gallery_reveal_ms: 420,
            snipping_tool_folder: Some(std::env::temp_dir()),
            ..Settings::default()
        };
        settings
            .action_menu
            .command_visibility
            .insert("rename".into(), false);
        db.execute(
            "INSERT INTO preferences VALUES('general',?1)",
            [serde_json::to_string(&settings).unwrap()],
        )
        .unwrap();
        let (loaded, migrate) = read_profile(&db, Settings::default).unwrap();
        assert!(migrate);
        write_profile(&db, &loaded).unwrap();
        let raw: String = db
            .query_row(
                "SELECT document FROM preferences WHERE key='general'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut general: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            EXTENSION_FIELDS
                .iter()
                .all(|field| general.get(field).is_none())
        );
        general["capture_on_start"] = false.into();
        db.execute(
            "UPDATE preferences SET document=?1 WHERE key='general'",
            [general.to_string()],
        )
        .unwrap();
        let (loaded, migrate) = read_profile(&db, Settings::default).unwrap();
        assert!(!migrate);
        assert!(!loaded.capture_on_start);
        assert_eq!(loaded.gallery_reveal_ms, 420);
        assert_eq!(loaded.gallery_reveal, GalleryReveal::Grow);
        assert_eq!(loaded.snipping_tool_folder, settings.snipping_tool_folder);
        assert_eq!(loaded.action_menu, settings.action_menu);
    }

    #[test]
    fn window_resizable_defaults_off_and_round_trips() {
        let defaults = Settings::default();
        assert!(!defaults.window_resizable);

        let mut enabled = defaults;
        enabled.window_resizable = true;
        let encoded = serde_json::to_value(&enabled).unwrap();
        let decoded: Settings = serde_json::from_value(encoded).unwrap();
        assert!(decoded.window_resizable);

        let mut legacy = serde_json::to_value(Settings::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("window_resizable");
        let decoded: Settings = serde_json::from_value(legacy).unwrap();
        assert!(!decoded.window_resizable);
    }

    #[test]
    fn desktop_wallpaper_tracking_round_trips() {
        let mut settings = Settings::default();
        assert!(settings.desktop_wallpaper_id.is_none());
        settings.desktop_wallpaper_id = Some("a".repeat(64));
        assert_eq!(settings.validate(0), Ok(()));
        let encoded = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&encoded)
                .unwrap()
                .desktop_wallpaper_id,
            settings.desktop_wallpaper_id
        );
    }
}

//! Persistent presentation preferences; domain behavior is independent of theme.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    #[default]
    Standard,
    BlueExperimental,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemePreferences {
    pub preset: Preset,
    pub accent: Option<[u8; 3]>,
    pub selection: Option<[u8; 3]>,
    pub selection_intensity: u8,
    pub decoration: bool,
    pub lily_background: Option<u8>,
    pub custom_background: Option<String>,
    pub video_background: Option<String>,
    pub video_enabled: Option<bool>,
    pub glass: bool,
    pub compact_header: bool,
    pub optical_strength: u8,
    pub optical_refraction: u8,
    pub optical_blur: u8,
    pub optical_highlight: u8,
    pub optical_chromatic: u8,
}
impl Default for ThemePreferences {
    fn default() -> Self {
        Self {
            preset: Preset::Standard,
            accent: None,
            selection: None,
            selection_intensity: 35,
            decoration: true,
            lily_background: None,
            custom_background: None,
            video_background: None,
            video_enabled: None,
            glass: false,
            compact_header: true,
            optical_strength: 0,
            optical_refraction: 100,
            optical_blur: 100,
            optical_highlight: 100,
            optical_chromatic: 100,
        }
    }
}
impl ThemePreferences {
    /// Missing flag preserves active videos from the first video-background build.
    pub fn video_is_enabled(&self) -> bool {
        self.video_enabled
            .unwrap_or(self.video_background.is_some())
    }
    pub fn active_video(&self) -> Option<&str> {
        if self.video_is_enabled() {
            self.video_background.as_deref()
        } else {
            None
        }
    }
}
pub fn catalogue(current: &ThemePreferences) -> serde_json::Value {
    serde_json::json!({"current":current,"themes":[{"id":"standard","label":"Стандартная","experimental":false},{"id":"blue_experimental","label":"Синяя · экспериментальная","experimental":true}]})
}

use crate::interaction::View;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub quota_bytes: u64,
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
            quota_bytes: quota,
            card_width: 280.0,
            default_view: View::Gallery,
            capture_on_start: false,
        }
    }
    pub fn validate(&self, usage: u64) -> Result<(), String> {
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

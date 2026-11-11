//! Background selection is a domain operation shared by Actions and API jobs.
use crate::*;
impl Service {
    pub(crate) fn selected_background_image(&self) -> Result<Event> {
        let items = self.selected_refs()?;
        if items.len() != 1 {
            return Err(ApiError::new(
                "invalid_selection",
                "Выберите одно изображение",
            ));
        }
        let event = self.get(items[0].id)?;
        if event.revision != items[0].revision
            || matches!(
                event.kind,
                EventKind::TextFragment | EventKind::LinkFragment | EventKind::TableFragment
            )
        {
            return Err(ApiError::new("invalid_image", "Выберите изображение"));
        }
        if !file_types::is_native_image(std::path::Path::new(&event.name)) {
            return Err(ApiError::new("invalid_image", "Выберите изображение"));
        }
        Ok(event)
    }
    pub(crate) fn activate_background(
        &mut self,
        mut result: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = result["id"]
            .as_str()
            .ok_or_else(|| ApiError::new("invalid_background", "Background ID missing"))?
            .to_owned();
        let mut settings = self.settings.clone();
        settings.theme.preset = theme::Preset::BlueExperimental;
        settings.theme.custom_background = Some(id);
        settings.theme.lily_background = None;
        self.dispatch(Command::SetSettings { settings })?;
        result["background_applied"] = serde_json::json!(true);
        Ok(result)
    }
}

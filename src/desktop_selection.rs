//! Standard selection deletion for managed Desktop assets.
use crate::*;
use serde_json::{Value, json};

impl Service {
    pub(crate) fn delete_desktop_selection(
        &mut self,
        ids: Vec<i64>,
        confirmed: bool,
    ) -> Result<Value> {
        let mut seen = std::collections::HashSet::new();
        let ids: Vec<_> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
        if ids.is_empty()
            || ids.len() > 1000
            || ids.iter().any(|id| !desktop_projection::is_desktop_id(*id))
        {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected only Desktop asset IDs",
            ));
        }
        if ids.len() > 1 && !confirmed {
            return Ok(
                json!({"confirmation_required":true,"ids":ids,"original_files":false,"permanent_files":false,"recycle_bin":false,"deleted":0}),
            );
        }
        let mut results = Vec::new();
        let mut removed = Vec::new();
        for id in &ids {
            let result = (|| {
                let (event, typed) = desktop_projection::get(&self.root, *id)?;
                let (_, typed_checked) =
                    desktop_projection::resolve(&self.root, *id, &event.revision)?;
                if typed != typed_checked {
                    return Err(ApiError::new("stale_selection", "Desktop asset changed"));
                }
                let wallpapers = if typed.starts_with("image:") {
                    desktop_wallpaper::current_wallpapers().unwrap_or_default()
                } else {
                    Vec::new()
                };
                let value = desktop_assets::delete(
                    &self.root,
                    &typed,
                    &self.settings.theme,
                    self.settings.desktop_wallpaper_id.as_deref(),
                    &wallpapers,
                )?;
                removed.push(*id);
                Ok(value)
            })();
            results.push(match result {
                Ok(value) => json!({"id":id,"ok":true,"result":value}),
                Err(error) => json!({"id":id,"ok":false,"error":error}),
            });
        }
        self.interaction.selected.retain(|id| !removed.contains(id));
        if self
            .interaction
            .preview
            .is_some_and(|id| removed.contains(&id))
        {
            self.interaction.set_preview(None);
        }
        Ok(
            json!({"ids":ids,"confirmation_required":false,"deleted":removed.len(),"items":results,"original_files":false,"permanent_files":false,"recycle_bin":false}),
        )
    }
}

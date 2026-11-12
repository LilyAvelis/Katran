//! Dispatcher-facing tag slot commands. Widgets are clients of these results.
use crate::{ApiError, Result, Service, tag_slots};
use serde_json::{Value, json};

fn error(value: tag_slots::StoreError) -> ApiError {
    match value {
        tag_slots::StoreError::Invalid(message) => ApiError::new("invalid_tag_slot", message),
        tag_slots::StoreError::Database(error) => ApiError::new("storage_error", error),
    }
}

impl Service {
    pub(crate) fn filter_tag_slot(&mut self, slot: Option<u8>) -> Result<Value> {
        if self.interaction.tab == crate::interaction::ShelfTab::All {
            return Err(ApiError::new(
                "folder_has_no_tags",
                "Folder has no tag filters",
            ));
        }
        if slot.is_some_and(|number| !(1..=9).contains(&number)) {
            return Err(ApiError::new("invalid_tag_slot", "slot must be 1..9"));
        }
        let next = if self.interaction.tag_slot_filter == slot {
            None
        } else {
            slot
        };
        let pinned_only = self.interaction.pinned_only;
        self.interaction.reset_context();
        self.interaction.pinned_only = pinned_only;
        self.interaction.tag_slot_filter = next;
        Ok(json!(self.interaction))
    }

    pub(crate) fn get_tag_slots(&self, ids: Vec<i64>) -> Result<Value> {
        if ids.len() > 1000 || ids.iter().any(|id| *id <= 0) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected at most 1000 positive IDs",
            ));
        }
        let encoded =
            serde_json::to_string(&ids).map_err(|e| ApiError::new("invalid_argument", e))?;
        let mut statement = self.db.prepare("SELECT CAST(j.value AS INTEGER), a.slot_number FROM json_each(?1) j LEFT JOIN katran_properties p ON p.event_id=CAST(j.value AS INTEGER) AND p.pinned=1 LEFT JOIN tag_slot_assignments a ON a.event_id=p.event_id ORDER BY CAST(j.key AS INTEGER)")?;
        let items = statement
            .query_map([encoded], |row| {
                Ok(json!({"id":row.get::<_,i64>(0)?,"slot":row.get::<_,Option<u8>>(1)?}))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({"items":items}))
    }

    pub(crate) fn list_tag_slots(&self) -> Result<Value> {
        let slots = tag_slots::list_slots(&self.db).map_err(error)?;
        Ok(json!({"slots":slots}))
    }

    pub(crate) fn save_tag_slot(&mut self, slot: tag_slots::Slot) -> Result<Value> {
        tag_slots::save_slot(&self.db, &slot).map_err(error)?;
        self.interaction.view_generation = self.interaction.view_generation.wrapping_add(1);
        Ok(json!({"slot":slot}))
    }

    pub(crate) fn assign_tag_slot(&mut self, ids: Vec<i64>, slot: Option<u8>) -> Result<Value> {
        let items = tag_slots::assign(&mut self.db, &ids, slot).map_err(error)?;
        if items
            .iter()
            .any(|item| item.outcome == tag_slots::Outcome::Applied)
        {
            self.interaction.view_generation = self.interaction.view_generation.wrapping_add(1);
        }
        Ok(json!({"items":items,"slot":slot,"atomic":false}))
    }

    pub(crate) fn get_tag_slot(&self, id: i64) -> Result<Value> {
        self.get(id)?;
        let number = tag_slots::visible_slot(&self.db, id).map_err(error)?;
        Ok(json!({"slot":number}))
    }
}

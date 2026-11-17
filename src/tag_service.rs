//! Dispatcher-facing tag slot commands. Widgets are clients of these results.
use crate::{ApiError, Result, Service, tag_slots};
use serde_json::{Value, json};

fn error(value: tag_slots::StoreError) -> ApiError {
    match value {
        tag_slots::StoreError::Invalid(message) => ApiError::new("invalid_tag_slot", message),
        tag_slots::StoreError::Database(error) => ApiError::new("storage_error", error),
    }
}

pub(crate) fn scope(tab: crate::interaction::ShelfTab) -> Result<&'static str> {
    use crate::interaction::ShelfTab::*;
    Ok(match tab {
        All => {
            return Err(ApiError::new(
                "folder_has_no_tags",
                "Folder has no tag slots",
            ));
        }
        Images => "images",
        Screenshots => "screenshots",
        Downloads => "downloads",
        Text => "text",
        Links => "links",
        Tables => "tables",
        Pods => "pods",
        Files => "files",
        Desktop => "desktop",
    })
}

impl Service {
    pub(crate) fn filter_tag_slot(&mut self, slot: Option<u8>) -> Result<Value> {
        scope(self.interaction.tab)?;
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
        self.get_tag_slots_for_tab(self.interaction.tab, ids)
    }

    pub(crate) fn get_tag_slots_for_tab(
        &self,
        tab: crate::interaction::ShelfTab,
        ids: Vec<i64>,
    ) -> Result<Value> {
        let scope = scope(tab)?;
        if ids
            .iter()
            .any(|id| crate::desktop_projection::is_desktop_id(*id))
        {
            if scope != "desktop" {
                return Err(ApiError::new(
                    "invalid_argument",
                    "Desktop IDs require the Desktop tab",
                ));
            }
            if ids.len() > 1000
                || ids
                    .iter()
                    .any(|id| !crate::desktop_projection::is_desktop_id(*id))
            {
                return Err(ApiError::new(
                    "invalid_argument",
                    "Cannot mix Desktop and catalogue IDs",
                ));
            }
            let mut items = Vec::new();
            for id in ids {
                items.push(json!({"id":id,"slot":crate::desktop_pins::slot(&self.db,id)?}));
            }
            return Ok(json!({"items":items}));
        }
        if ids.len() > 1000 || ids.iter().any(|id| *id <= 0) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected at most 1000 positive IDs",
            ));
        }
        let encoded =
            serde_json::to_string(&ids).map_err(|e| ApiError::new("invalid_argument", e))?;
        let mut statement = self.db.prepare("SELECT CAST(j.value AS INTEGER), a.slot_number FROM json_each(?1) j LEFT JOIN katran_properties p ON p.event_id=CAST(j.value AS INTEGER) AND p.pinned=1 LEFT JOIN tag_scoped_assignments a ON a.event_id=p.event_id AND a.scope=?2 ORDER BY CAST(j.key AS INTEGER)")?;
        let items = statement
            .query_map(rusqlite::params![encoded, scope], |row| {
                Ok(json!({"id":row.get::<_,i64>(0)?,"slot":row.get::<_,Option<u8>>(1)?}))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({"items":items}))
    }

    pub(crate) fn list_tag_slots(&self) -> Result<Value> {
        self.list_tag_slots_for_tab(self.interaction.tab)
    }

    pub(crate) fn list_tag_slots_for_tab(
        &self,
        tab: crate::interaction::ShelfTab,
    ) -> Result<Value> {
        let slots = tag_slots::list_slots(&self.db, scope(tab)?).map_err(error)?;
        Ok(json!({"slots":slots,"tab":tab}))
    }

    pub(crate) fn save_tag_slot(&mut self, slot: tag_slots::Slot) -> Result<Value> {
        self.save_tag_slot_for_tab(self.interaction.tab, slot)
    }

    pub(crate) fn save_tag_slot_for_tab(
        &mut self,
        tab: crate::interaction::ShelfTab,
        slot: tag_slots::Slot,
    ) -> Result<Value> {
        tag_slots::save_slot(&self.db, scope(tab)?, &slot).map_err(error)?;
        self.interaction.view_generation = self.interaction.view_generation.wrapping_add(1);
        Ok(json!({"slot":slot,"tab":tab}))
    }

    pub(crate) fn assign_tag_slot(&mut self, ids: Vec<i64>, slot: Option<u8>) -> Result<Value> {
        let scope = scope(self.interaction.tab)?;
        if ids
            .iter()
            .any(|id| crate::desktop_projection::is_desktop_id(*id))
        {
            if scope != "desktop" {
                return Err(ApiError::new(
                    "invalid_argument",
                    "Desktop IDs require the Desktop tab",
                ));
            }
            if ids
                .iter()
                .any(|id| !crate::desktop_projection::is_desktop_id(*id))
            {
                return Err(ApiError::new(
                    "invalid_argument",
                    "Cannot mix Desktop and catalogue IDs",
                ));
            }
            for id in &ids {
                crate::desktop_projection::get(&self.root, *id)?;
            }
            let items = crate::desktop_pins::assign_slot(&self.db, &ids, slot)?;
            if items
                .iter()
                .any(|item| item.outcome == tag_slots::Outcome::Applied)
            {
                self.interaction.view_generation = self.interaction.view_generation.wrapping_add(1);
            }
            return Ok(json!({"items":items,"slot":slot,"atomic":false}));
        }
        let items = tag_slots::assign(&mut self.db, scope, &ids, slot).map_err(error)?;
        if items
            .iter()
            .any(|item| item.outcome == tag_slots::Outcome::Applied)
        {
            self.interaction.view_generation = self.interaction.view_generation.wrapping_add(1);
        }
        Ok(json!({"items":items,"slot":slot,"atomic":false}))
    }

    pub(crate) fn get_tag_slot(&self, id: i64) -> Result<Value> {
        let scope = scope(self.interaction.tab)?;
        if crate::desktop_projection::is_desktop_id(id) && scope != "desktop" {
            return Err(ApiError::new(
                "invalid_argument",
                "Desktop IDs require the Desktop tab",
            ));
        }
        self.get(id)?;
        if crate::desktop_projection::is_desktop_id(id) {
            return Ok(json!({"slot":crate::desktop_pins::slot(&self.db,id)?}));
        }
        let number = tag_slots::visible_slot(&self.db, scope, id).map_err(error)?;
        Ok(json!({"slot":number}))
    }
}

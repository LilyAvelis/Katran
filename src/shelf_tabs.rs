//! Shelf browsing is a client of the SQL catalogue projection.
use crate::{Result, Service};

impl Service {
    pub(crate) fn move_visible_pinned(
        &self,
        id: i64,
        direction: crate::pinned_order::Direction,
    ) -> crate::Result<crate::pinned_order::MoveResult> {
        use crate::{
            ApiError,
            interaction::ShelfTab,
            sorting::{SortMode, SortSpec},
        };
        if self.interaction.tab == ShelfTab::All {
            return Err(ApiError::new(
                "folder_has_no_pins",
                "Folder has no pinned order",
            ));
        }
        if crate::pinned_order::rank(&self.db, id)?.is_none() {
            return crate::pinned_order::swap(&self.db, id, id);
        }
        let mut ids = Vec::new();
        let mut cursor = None;
        loop {
            let (events, next) = crate::catalogue_query::query_sorted_slice(
                &self.db,
                self.interaction.tab,
                &self.interaction.browse_query,
                1000,
                cursor.as_deref(),
                SortSpec {
                    mode: SortMode::Pinned,
                    reverse: false,
                    seed: 0,
                },
                true,
                self.interaction.tag_filter.as_deref(),
                self.interaction.tag_slot_filter,
                self.interaction.view_generation,
                (
                    self.settings.tab_enabled(ShelfTab::Images),
                    self.settings.tab_enabled(ShelfTab::Text),
                    self.settings.tab_enabled(ShelfTab::Links),
                    self.settings.tab_enabled(ShelfTab::Tables),
                ),
            )?;
            ids.extend(events.into_iter().map(|event| event.id));
            cursor = next;
            if cursor.is_none() {
                break;
            }
        }
        let index = ids
            .iter()
            .position(|candidate| *candidate == id)
            .ok_or_else(|| {
                ApiError::new(
                    "not_visible",
                    "Pinned object is outside the current shelf view",
                )
            })?;
        let other = match direction {
            crate::pinned_order::Direction::Up => index.checked_sub(1),
            crate::pinned_order::Direction::Down => (index + 1 < ids.len()).then_some(index + 1),
        }
        .map_or(id, |position| ids[position]);
        crate::pinned_order::swap(&self.db, id, other)
    }
    pub(crate) fn browse_shelf_sorted(
        &self,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<serde_json::Value> {
        if self.interaction.tab == crate::interaction::ShelfTab::All {
            return self
                .work_folder
                .browse(query, limit, cursor, self.interaction.sort);
        }
        let (events, next) = crate::catalogue_query::query_sorted_slice(
            &self.db,
            self.interaction.tab,
            query,
            limit,
            cursor,
            self.interaction.sort,
            self.interaction.pinned_only,
            self.interaction.tag_filter.as_deref(),
            self.interaction.tag_slot_filter,
            self.interaction.view_generation,
            (
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Images),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Text),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Links),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Tables),
            ),
        )?;
        self.decorate_page(
            serde_json::json!({"events":events,"next_cursor":next,"sort":self.interaction.sort}),
        )
    }
    pub(crate) fn browse_shelf(
        &self,
        query: &str,
        limit: u32,
        after: Option<i64>,
    ) -> Result<serde_json::Value> {
        if self.interaction.tab == crate::interaction::ShelfTab::All {
            if after.is_some() {
                return Err(crate::ApiError::new(
                    "invalid_cursor",
                    "Folder requires BrowseShelfSorted cursors",
                ));
            }
            return self
                .work_folder
                .browse(query, limit, None, self.interaction.sort);
        }
        if self.interaction.pinned_only || self.interaction.tag_slot_filter.is_some() {
            return self.browse_shelf_sorted(query, limit, None);
        }
        let events = crate::catalogue_query::query_features(
            &self.db,
            self.interaction.tab,
            query,
            limit,
            after,
            (
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Images),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Text),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Links),
                self.settings
                    .tab_enabled(crate::interaction::ShelfTab::Tables),
            ),
        )?;
        let next = (events.len() == limit as usize).then(|| events.last().unwrap().id);
        self.decorate_page(serde_json::json!({"events":events,"next_cursor":next}))
    }
}

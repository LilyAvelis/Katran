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
        if self.interaction.tab == ShelfTab::Desktop {
            let events = crate::desktop_projection::list(&self.root)?;
            let mut ranks = std::collections::HashMap::new();
            for event in &events {
                if let Some(rank) = crate::desktop_pins::rank(&self.db, event.id)? {
                    ranks.insert(event.id, rank);
                }
            }
            let mut ids = Vec::new();
            let query = self.interaction.browse_query.to_lowercase();
            for event in events {
                if !ranks.contains_key(&event.id)
                    || !event.name.to_lowercase().contains(&query)
                    || self.interaction.tag_filter.is_some()
                {
                    continue;
                }
                if let Some(slot) = self.interaction.tag_slot_filter {
                    if crate::desktop_pins::slot(&self.db, event.id)? != Some(slot) {
                        continue;
                    }
                }
                ids.push(event.id);
            }
            ids.sort_by_key(|id| ranks[id]);
            let index = ids
                .iter()
                .position(|candidate| *candidate == id)
                .ok_or_else(|| {
                    crate::ApiError::new("not_visible", "Pinned asset is outside the Desktop view")
                })?;
            let other = match direction {
                crate::pinned_order::Direction::Up => index.checked_sub(1),
                crate::pinned_order::Direction::Down => {
                    (index + 1 < ids.len()).then_some(index + 1)
                }
            }
            .map_or(id, |position| ids[position]);
            return crate::desktop_pins::swap_visible(&self.db, id, other);
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
        if self.interaction.tab == crate::interaction::ShelfTab::Pods
            && self.interaction.emoji_catalogue
        {
            return crate::pods::browse_emoji(
                query,
                limit,
                cursor,
                self.interaction.emoji_category.as_deref(),
            );
        }
        if self.interaction.tab == crate::interaction::ShelfTab::All {
            return self
                .work_folder
                .browse(query, limit, cursor, self.interaction.sort);
        }
        if self.interaction.tab == crate::interaction::ShelfTab::Desktop {
            let mut events = crate::desktop_projection::list(&self.root)?;
            let mut ranks = std::collections::HashMap::new();
            for event in &events {
                if let Some(rank) = crate::desktop_pins::rank(&self.db, event.id)? {
                    ranks.insert(event.id, rank);
                }
            }
            if self.interaction.pinned_only {
                events.retain(|event| ranks.contains_key(&event.id));
            }
            if let Some(slot) = self.interaction.tag_slot_filter {
                let mut matching = std::collections::HashSet::new();
                for event in &events {
                    if crate::desktop_pins::slot(&self.db, event.id)? == Some(slot) {
                        matching.insert(event.id);
                    }
                }
                events.retain(|event| matching.contains(&event.id));
            }
            if self.interaction.tag_filter.is_some() {
                events.clear();
            }
            let (events, next) = crate::desktop_projection::browse(
                events,
                query,
                limit,
                cursor,
                self.interaction.sort,
                &ranks,
            )?;
            return self.decorate_page(serde_json::json!({"events":events,"next_cursor":next,"sort":self.interaction.sort}));
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
        if self.interaction.tab == crate::interaction::ShelfTab::Desktop {
            if after.is_some() {
                return Err(crate::ApiError::new(
                    "invalid_cursor",
                    "Desktop requires BrowseShelfSorted cursors",
                ));
            }
            return self.browse_shelf_sorted(query, limit, None);
        }
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

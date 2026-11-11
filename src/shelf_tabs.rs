//! Shelf browsing is a client of the SQL catalogue projection.
use crate::{Result, Service};

impl Service {
    pub(crate) fn browse_shelf_sorted(
        &self,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<serde_json::Value> {
        if self.interaction.tab == crate::interaction::ShelfTab::All {
            return self.work_folder.browse(query,limit,cursor,self.interaction.sort);
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
        self.decorate_page(serde_json::json!({"events":events,"next_cursor":next,"sort":self.interaction.sort}))
    }
    pub(crate) fn browse_shelf(
        &self,
        query: &str,
        limit: u32,
        after: Option<i64>,
    ) -> Result<serde_json::Value> {
        if self.interaction.tab == crate::interaction::ShelfTab::All {
            if after.is_some() {return Err(crate::ApiError::new("invalid_cursor","Folder requires BrowseShelfSorted cursors"));}
            return self.work_folder.browse(query,limit,None,self.interaction.sort);
        }
        if self.interaction.pinned_only {
            return self.browse_shelf_sorted(query,limit,None);
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

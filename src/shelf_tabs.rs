//! Shelf browsing is a client of the SQL catalogue projection.
use crate::{Result, Service};

impl Service {
    pub(crate) fn browse_shelf_sorted(
        &self,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<serde_json::Value> {
        let (events, next) = crate::catalogue_query::query_sorted(
            &self.db,
            self.interaction.tab,
            query,
            limit,
            cursor,
            self.interaction.sort,
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
        Ok(serde_json::json!({"events":events,"next_cursor":next,"sort":self.interaction.sort}))
    }
    pub(crate) fn browse_shelf(
        &self,
        query: &str,
        limit: u32,
        after: Option<i64>,
    ) -> Result<serde_json::Value> {
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
        Ok(serde_json::json!({"events":events,"next_cursor":next}))
    }
}

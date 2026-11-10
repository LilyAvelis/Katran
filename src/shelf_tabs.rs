//! Shelf browsing is a client of the SQL catalogue projection.
use crate::{Result, Service};

impl Service {
    pub(crate) fn browse_shelf(
        &self,
        query: &str,
        limit: u32,
        after: Option<i64>,
    ) -> Result<serde_json::Value> {
        let events =
            crate::catalogue_query::query(&self.db, self.interaction.tab, query, limit, after)?;
        let next = (events.len() == limit as usize).then(|| events.last().unwrap().id);
        Ok(serde_json::json!({"events":events,"next_cursor":next}))
    }
}

//! First-page snapshots for asynchronous browse clients.
//!
//! Each result is one SQLite read transaction, including current persisted
//! settings, event metadata and tag materials. No Interaction state changes.
//! Callers must validate a CatalogueStamp from their SAME writer connection
//! before and after worker work and before reusing a cached page. Reader and
//! writer PRAGMA data_version values cannot be compared across connections.
use crate::{ApiError, Result, Service, interaction::ShelfTab};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

impl Service {
    pub(crate) fn catalogue_stamp(&self) -> Result<Value> {
        let data_version: u64 = self
            .db
            .query_row("PRAGMA data_version", [], |row| row.get(0))?;
        Ok(json!({"changes":self.db.total_changes(),"data_version":data_version}))
    }

    pub(crate) fn browse_tab_snapshot(
        &self,
        tab: ShelfTab,
        query: &str,
        limit: u32,
    ) -> Result<Value> {
        if !(1..=1000).contains(&limit) {
            return Err(ApiError::new("invalid_argument", "Expected limit 1..1000"));
        }
        if tab == ShelfTab::All {
            return Err(ApiError::new(
                "session_only_tab",
                "Folder requires the active session browse API",
            ));
        }
        let transaction = self.db.unchecked_transaction()?;
        // A long-lived preview reader's startup settings may be stale. Read the
        // document inside this snapshot, together with all of its catalogue data.
        let document: Option<String> = transaction
            .query_row(
                "SELECT document FROM preferences WHERE key='general'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let settings: crate::settings::Settings = match document {
            Some(document) => serde_json::from_str(&document)
                .map_err(|error| ApiError::new("settings_error", error))?,
            None => self.settings.clone(),
        };
        if !settings.tab_enabled(tab) {
            return Err(ApiError::new(
                "tab_disabled",
                "Enable this tab in Settings → Tabs first",
            ));
        }
        let sort = settings.tab_preferences(tab).sort.normalized();
        let (events, next) = crate::catalogue_query::query_sorted_slice(
            &transaction,
            tab,
            query,
            limit,
            None,
            sort,
            false,
            None,
            None,
            0,
            (
                settings.tab_enabled(ShelfTab::Images),
                settings.tab_enabled(ShelfTab::Text),
                settings.tab_enabled(ShelfTab::Links),
                settings.tab_enabled(ShelfTab::Tables),
            ),
        )?;
        let ids: Vec<_> = events.iter().map(|event| event.id).collect();
        let materials = self.get_tag_slots(ids)?;
        let assignments: Vec<_> = materials["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| !item["slot"].is_null())
            .cloned()
            .collect();
        let slots = self.list_tag_slots()?;
        let page = self.decorate_page(json!({
            "events":events,
            // Active-session cursors carry view_generation and cannot be
            // transferred from a prefetched tab to the active Interaction.
            "next_cursor":null,
            "has_more":next.is_some(),
            "slots":slots["slots"],
            "assignments":assignments,
            "view":{"tab":tab,"query":query,"sort":sort,
                "pinned_only":false,"tag":null,"tag_slot":null}
        }))?;
        transaction.commit()?;
        Ok(page)
    }
}

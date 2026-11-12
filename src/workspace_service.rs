//! Workspace policy is shared by the GUI and JSON API.
use crate::*;
use serde_json::{Value, json};

impl Service {
    pub(crate) fn choose_folder(&mut self, path: PathBuf) -> Result<Value> {
        // Folder clipboard exports are independent files. Their session IDs
        // expire on changing the working folder; paste those files normally.
        if self
            .transfer_intent
            .as_ref()
            .is_some_and(|(items, _)| items.iter().any(|item| item.id < 0))
        {
            self.transfer_intent = None;
            self.transfer_sequence = None;
        }
        self.work_folder.choose(&path)?;
        let path = self.work_folder.current().unwrap().to_path_buf();
        pinned::record_folder(&self.db, &path)?;
        self.interaction.reset_context();
        self.interaction.set_tab(interaction::ShelfTab::All);
        self.interaction.folder_path = Some(path);
        Ok(json!(self.interaction))
    }
    pub(crate) fn folder_entries(&self, favorites: bool, query: &str) -> Result<Value> {
        let entries = if favorites {
            pinned::folder_favorites(&self.db)?
        } else {
            pinned::folder_history(&self.db)?
        };
        let query = query.to_lowercase();
        Ok(
            json!({"items":entries.into_iter().filter(|e| e.path.to_string_lossy().to_lowercase().contains(&query)).collect::<Vec<_>>()}),
        )
    }
    pub(crate) fn toggle_pins(&mut self, ids: Vec<i64>) -> Result<Value> {
        // A background delete may already own an authorized filesystem effect.
        // Refuse pin changes until that receipt has been collected.
        if self.jobs.stats().active > 0 {
            return Err(ApiError::new(
                "domain_busy",
                "Wait for active operations before changing pins",
            ));
        }
        if ids.iter().any(|id| *id < 0) {
            return Err(ApiError::new(
                "folder_has_no_pins",
                "Folder files cannot be pinned",
            ));
        }
        let result = pinned::toggle_pins(&self.db, &ids)?;
        self.interaction.view_generation = self.interaction.view_generation.wrapping_add(1);
        Ok(json!({"items":result}))
    }
    pub(crate) fn pinned_tags(&self) -> Result<Value> {
        let category = self.pin_category_sql();
        let sql = format!(
            "SELECT DISTINCT t.tag_key,t.tag_display FROM katran_tags t JOIN katran_properties k ON k.event_id=t.event_id AND k.pinned=1 JOIN events e ON e.id=t.event_id WHERE {category} ORDER BY t.tag_key"
        );
        let mut statement = self.db.prepare(&sql)?;
        let items = statement
            .query_map([], |r| {
                Ok(pinned::Tag {
                    key: r.get(0)?,
                    display: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({"items":items}))
    }
    pub(crate) fn pin_category_sql(&self) -> String {
        use interaction::ShelfTab;
        match self.interaction.tab {
            ShelfTab::All => "0".into(),
            ShelfTab::Downloads => "e.source='downloads'".into(),
            ShelfTab::Screenshots => "e.source='sharex'".into(),
            ShelfTab::Text => "EXISTS(SELECT 1 FROM text_fragments f JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE f.event_id=e.id AND katran_is_pod(c.content)=0)".into(),
            ShelfTab::Pods => "EXISTS(SELECT 1 FROM text_fragments f JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE f.event_id=e.id AND katran_is_pod(c.content)=1)".into(),
            ShelfTab::Files => "EXISTS(SELECT 1 FROM files_pointers f WHERE f.event_id=e.id)".into(),
            ShelfTab::Links => {
                "EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id)".into()
            }
            ShelfTab::Tables => {
                "EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id)".into()
            }
            ShelfTab::Images => format!(
                "{} AND e.source NOT IN ('sharex','downloads') AND NOT EXISTS(SELECT 1 FROM text_fragments f WHERE f.event_id=e.id) AND NOT EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id) AND NOT EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id)",
                file_types::sql_image_predicate()
            ),
        }
    }
    pub(crate) fn decorate_page(&self, mut value: Value) -> Result<Value> {
        if let Some(events) = value["events"].as_array_mut() {
            for event in events {
                if let Some(id) = event["id"].as_i64().filter(|id| *id > 0) {
                    event["properties"] = json!(pinned::properties(&self.db, id)?);
                    event["tags"] = json!(pinned::tags(&self.db, id)?);
                }
            }
        }
        Ok(value)
    }
}

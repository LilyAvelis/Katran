//! The dispatcher owns configuration generations and accepts only private worker receipts.
use crate::{
    source_policy::{SourceConfiguration, SourceId},
    *,
};
impl Service {
    pub(crate) fn ensure_sources(&mut self) -> Result<serde_json::Value> {
        self.configure_sources(true)
    }
    pub(crate) fn configure_sources(&mut self, start: bool) -> Result<serde_json::Value> {
        if self.source_configured && (!start || self.source_runtime_started) {
            return Ok(serde_json::json!({"sources":self.source_runtime.statuses()}));
        }
        let downloads = if self.settings.downloads_enabled {
            downloads::configured_folder(self.settings.downloads_folder.as_deref())?
        } else {
            self.settings.downloads_folder.clone().unwrap_or_default()
        };
        let desired = [
            (
                SourceId::Sharex,
                self.settings.sharex_folder.clone().unwrap_or_default(),
                self.settings.sharex_folder.is_some(),
            ),
            (
                SourceId::Downloads,
                downloads,
                self.settings.downloads_enabled,
            ),
        ];
        let mut configurations = Vec::new();
        for (source_id, root, enabled) in desired {
            let encoded = root.to_string_lossy().into_owned();
            let previous: Option<(u64, String, bool)> = self
                .db
                .query_row(
                    "SELECT generation,root,enabled FROM source_configurations WHERE source_id=?1",
                    [source_id.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let generation = match previous {
                Some((generation, old_root, old_enabled))
                    if old_root == encoded && old_enabled == enabled =>
                {
                    generation
                }
                Some((generation, _, _)) => generation.checked_add(1).ok_or_else(|| {
                    ApiError::new("generation_exhausted", "Source generation exhausted")
                })?,
                None => 1,
            };
            self.db.execute("INSERT INTO source_configurations VALUES(?1,?2,?3,?4) ON CONFLICT(source_id) DO UPDATE SET generation=excluded.generation,root=excluded.root,enabled=excluded.enabled",params![source_id.as_str(),generation,encoded,enabled])?;
            if source_id == SourceId::Sharex {
                self.db.execute("DELETE FROM classification_work WHERE kind='source' AND (source_generation<>?1 OR ?2=0)",params![generation,enabled])?;
                self.db.execute(
                    "DELETE FROM verified_source_digests WHERE source_generation<>?1 OR ?2=0",
                    params![generation, enabled],
                )?;
            }
            configurations.push(SourceConfiguration {
                source_id,
                generation,
                root,
                enabled,
            });
        }
        if start {
            self.source_runtime.configure(configurations)?;
            self.source_runtime_started = true;
        }
        self.source_configured = true;
        Ok(serde_json::json!({"sources":self.source_runtime.statuses()}))
    }
    pub(crate) fn poll_sources(&mut self, limit: u8) -> Result<serde_json::Value> {
        self.ensure_sources()?;
        if !(1..=8).contains(&limit) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected source poll limit 1..8",
            ));
        }
        let batch = self.source_runtime.poll(limit);
        let mut snapshot_changed = false;
        for snapshot in batch.snapshots {
            if !self.current_source(&snapshot.configuration)? {
                continue;
            }
            let source = snapshot.configuration.source_id.as_str();
            let paths: std::collections::HashSet<_> = snapshot
                .paths
                .into_iter()
                .map(|p| p.to_string_lossy().to_lowercase())
                .collect();
            let current = {
                let mut stmt=self.db.prepare("SELECT path_key,available FROM source_projection WHERE source_id=?1 AND generation=?2")?;
                stmt.query_map(params![source, snapshot.configuration.generation], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
            };
            let tx = self.db.unchecked_transaction()?;
            for (key, was_available) in current {
                let available = paths.contains(&key);
                if available != was_available {
                    snapshot_changed = true;
                    tx.execute("UPDATE source_projection SET available=?1 WHERE source_id=?2 AND generation=?3 AND path_key=?4",params![available,source,snapshot.configuration.generation,key])?;
                }
            }
            tx.commit()?;
        }
        let mut items = Vec::new();
        for prepared in batch.prepared {
            let path = prepared.candidate.path.clone();
            match self.commit_source_file(prepared) {
                Ok(Some(event)) => items.push(serde_json::json!({"ok":true,"event":event})),
                Ok(None) => {}
                Err(error) => items.push(serde_json::json!({"ok":false,"path":path,"error":error})),
            }
        }
        let classification = self.classification.poll(&self.db)?;
        Ok(
            serde_json::json!({"classification_changed":classification["classification_changed"],"classification":classification,"scanned":true,"items":items,"sources":batch.statuses,"source_errors":batch.errors,"refresh":snapshot_changed}),
        )
    }
    pub(crate) fn current_source(&self, configuration: &SourceConfiguration) -> Result<bool> {
        Ok(self.db.query_row("SELECT EXISTS(SELECT 1 FROM source_configurations WHERE source_id=?1 AND generation=?2 AND root=?3 AND enabled=1)",params![configuration.source_id.as_str(),configuration.generation,configuration.root.to_string_lossy()],|r|r.get(0))?)
    }
}

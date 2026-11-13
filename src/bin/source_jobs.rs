//! Thin source API client. The domain owns workers, generations and classification.
use super::i18n::tr;
use super::*;

#[derive(Default)]
pub(super) struct Sources {
    started: bool,
    pub(super) downloads_over_limit: bool,
    // Compatibility with capture completion until its refresh bookkeeping is removed.
    pub match_dirty: bool,
}
impl Shelf {
    /// Clipboard and queued transfers receive the dispatcher before source work.
    pub(super) fn source_tick(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(40));
        if self.api.is_none()
            || self.native_drag.is_some()
            || self.pending_copy.is_some()
            || self.pending_copy_ids.is_some()
        {
            return;
        }
        #[cfg(not(feature = "clipboard-events"))]
        if self.capture && self.last_poll.elapsed() >= CLIPBOARD_POLL_INTERVAL {
            return;
        }
        if !self.sources.started {
            let result = self.call(Command::EnsureSources);
            if !result["sources"].is_array() {
                return;
            }
            self.sources.started = true;
        }
        // Poll also synchronizes settings in the domain; the client does not
        // fingerprint configuration, create watchers or accept worker receipts.
        let result = self.call(Command::PollSources { limit: 1 });
        if let Some(sources) = result["sources"].as_array() {
            self.sources.downloads_over_limit = sources.iter().any(downloads_over_limit);
        }
        let mut errors = Vec::new();
        let item_errors = item_failures(&result["items"]);
        if !item_errors.is_empty() {
            errors.push(item_errors);
        }
        if let Some(source_errors) = result["source_errors"].as_array() {
            errors.extend(
                source_errors
                    .iter()
                    .filter_map(|error| error["message"].as_str().map(str::to_owned)),
            );
        }
        if result["refresh"] == true
            || result["classification_changed"] == true
            || result["items"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["ok"] == true))
        {
            self.refresh();
        }
        if !errors.is_empty() {
            self.message = format!("{}: {}", tr("Папки", "Folders"), errors.join("; "));
        }
    }
}

fn downloads_over_limit(source: &Value) -> bool {
    source["source_id"] == "downloads"
        && source["error"].as_str() == Some("Downloads scan exceeds 20000 entries")
}

#[cfg(test)]
mod tests {
    #[test]
    fn limit_warning_only_matches_downloads_limit() {
        use super::*;
        assert!(downloads_over_limit(
            &serde_json::json!({"source_id":"downloads", "error":"Downloads scan exceeds 20000 entries"})
        ));
        assert!(!downloads_over_limit(
            &serde_json::json!({"source_id":"sharex", "error":"Downloads scan exceeds 20000 entries"})
        ));
        assert!(!downloads_over_limit(
            &serde_json::json!({"source_id":"downloads", "error":null})
        ));
    }
    use super::*;
    #[test]
    fn thin_source_client_ingests_native_sources_through_domain_api() {
        let root = tempfile::tempdir().unwrap();
        let shots = tempfile::tempdir().unwrap();
        let downloads = tempfile::tempdir().unwrap();
        let path = shots.path().join("synthetic.png");
        image::RgbaImage::from_pixel(12, 8, image::Rgba([33, 66, 99, 255]))
            .save(&path)
            .unwrap();
        let download = downloads.path().join("synthetic.unknown");
        std::fs::write(&download, b"arbitrary bytes").unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let mut settings: katran::settings::Settings =
            serde_json::from_value(app.call(Command::GetSettings)).unwrap();
        settings.sharex_folder = Some(shots.path().into());
        settings.downloads_enabled = true;
        settings.downloads_folder = Some(downloads.path().into());
        app.call(Command::SetSettings { settings });
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.source_tick(&ctx);
            let arrivals = app.call(Command::List { limit: 100 });
            if arrivals["events"].as_array().is_some_and(|events| {
                events.iter().any(|event| event["source"] == "downloads")
                    && events.iter().any(|event| event["source"] == "sharex")
            }) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "source API client timed out: {}",
                app.message
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            app.api.is_some(),
            "source polling must retain the dispatcher client"
        );
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Screenshots,
        });
        app.refresh();
        assert_eq!(app.events.len(), 1);
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Downloads,
        });
        app.refresh();
        assert_eq!(app.events.len(), 1);
        assert_eq!(app.events[0]["name"], "synthetic.unknown");
        assert_eq!(std::fs::read(download).unwrap(), b"arbitrary bytes");
        assert!(path.is_file());
    }
}

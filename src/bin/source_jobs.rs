//! UI clients of native discovery and dispatcher commits. No source I/O here.
use super::*;
use katran::{
    folder_watch::{FolderConfig, FolderEvent, FolderSource, FolderWatcher, PreparedCandidate},
    screenshot_identity::MatchPair,
};
use std::collections::VecDeque;

type Setup = (Vec<FolderWatcher>, Vec<String>);
#[derive(Default)]
pub(super) struct Sources {
    configuration: String,
    setup: Option<mpsc::Receiver<Setup>>,
    watchers: Vec<FolderWatcher>,
    pending: VecDeque<PreparedCandidate>,
    refresh: bool,
    pub match_dirty: bool,
    match_snapshot: String,
    matching: Option<mpsc::Receiver<Vec<MatchPair>>>,
    pairs: Option<Vec<MatchPair>>,
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
        let settings = self.call(Command::GetSettings);
        let configuration = serde_json::json!([
            settings["sharex_folder"],
            settings["downloads_enabled"],
            settings["downloads_folder"]
        ])
        .to_string();
        if self.sources.configuration != configuration {
            self.sources = Sources {
                configuration,
                match_dirty: true,
                ..Default::default()
            };
            if let Ok(settings) = serde_json::from_value::<katran::settings::Settings>(settings) {
                let (tx, rx) = mpsc::channel();
                self.sources.setup = Some(rx);
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let mut configurations = Vec::new();
                    let mut errors = Vec::new();
                    if let Some(root) = settings.sharex_folder {
                        configurations.push(FolderConfig {
                            source: FolderSource::Sharex,
                            root,
                        });
                    }
                    if settings.downloads_enabled {
                        match katran::downloads::configured_folder(
                            settings.downloads_folder.as_deref(),
                        ) {
                            Ok(root) => configurations.push(FolderConfig {
                                source: FolderSource::Downloads,
                                root,
                            }),
                            Err(error) => errors.push(format!("Downloads: {}", error.message)),
                        }
                    }
                    let mut watchers = Vec::new();
                    for configuration in configurations {
                        let source = configuration.source;
                        match FolderWatcher::start(vec![configuration]) {
                            Ok(watcher) => watchers.push(watcher),
                            Err(error) => errors.push(format!("{source:?}: {}", error.message)),
                        }
                    }
                    let _ = tx.send((watchers, errors));
                    ctx.request_repaint();
                });
            }
        }
        if let Some(receiver) = self.sources.setup.as_ref() {
            match receiver.try_recv() {
                Ok((watchers, errors)) => {
                    self.sources.watchers = watchers;
                    self.sources.setup = None;
                    if !errors.is_empty() {
                        self.message = errors.join("; ");
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.sources.setup = None;
                    self.message = "Не удалось запустить наблюдение папок".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.sources.watchers.is_empty() {
            self.sources.watchers.rotate_left(1);
        }
        for watcher in &self.sources.watchers {
            while self.sources.pending.len() < 64 {
                let Some(event) = watcher.try_recv() else {
                    break;
                };
                match event {
                    FolderEvent::Candidate(candidate) => self.sources.pending.push_back(candidate),
                    FolderEvent::Refresh { source } => {
                        self.sources.refresh = true;
                        self.sources.match_dirty |= source == FolderSource::Sharex;
                    }
                    FolderEvent::Error { source, message } => {
                        self.message = format!("{source:?}: {message}")
                    }
                }
            }
        }
        if self.sources.refresh {
            self.sources.refresh = false;
            self.refresh();
        }
        // One inexpensive SQL commit per turn; discovery and content hashing
        // remain in FolderWatcher rather than taking the dispatcher from input.
        if let Some(candidate) = self.sources.pending.pop_front() {
            self.start_job(
                Command::CommitFolderCandidates {
                    items: vec![candidate],
                },
                false,
                ctx.clone(),
            );
            return;
        }
        if let Some(receiver) = self.sources.matching.as_ref() {
            match receiver.try_recv() {
                Ok(pairs) => {
                    self.sources.pairs = Some(pairs);
                    self.sources.matching = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.sources.matching = None;
                    self.message = "Не удалось сопоставить снимки Clipboard и ShareX".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(pairs) = self.sources.pairs.take()
            && !pairs.is_empty()
        {
            self.start_job(
                Command::CommitScreenshotMatches { pairs },
                false,
                ctx.clone(),
            );
            return;
        }
        if self.sources.match_dirty && self.sources.matching.is_none() {
            self.sources.match_dirty = false;
            let inputs = self.call(Command::ScreenshotMatchInputs);
            if let Ok(events) =
                serde_json::from_value::<Vec<katran::Event>>(inputs["events"].clone())
            {
                let snapshot = serde_json::to_string(&events).unwrap_or_default();
                if snapshot != self.sources.match_snapshot {
                    self.sources.match_snapshot = snapshot;
                    let (tx, rx) = mpsc::channel();
                    self.sources.matching = Some(rx);
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let pairs = katran::screenshot_identity::prepare_matches(&events);
                        let _ = tx.send(pairs);
                        ctx.request_repaint();
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_downloads_and_sharex_identity_workers_commit_through_api() {
        let root = tempfile::tempdir().unwrap();
        let shots = tempfile::tempdir().unwrap();
        let downloads = tempfile::tempdir().unwrap();
        let path = shots.path().join("synthetic.png");
        image::RgbaImage::from_pixel(12, 8, image::Rgba([33, 66, 99, 255]))
            .save(&path)
            .unwrap();
        std::fs::write(
            downloads.path().join("synthetic.unknown"),
            b"arbitrary bytes",
        )
        .unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::IngestBytes {
            name: "clipboard.png".into(),
            bytes: std::fs::read(&path).unwrap(),
            source: "clipboard_image".into(),
        });
        let mut settings: katran::settings::Settings =
            serde_json::from_value(app.call(Command::GetSettings)).unwrap();
        settings.sharex_folder = Some(shots.path().into());
        settings.downloads_enabled = true;
        settings.downloads_folder = Some(downloads.path().into());
        app.call(Command::SetSettings { settings });
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        app.refresh();
        assert_eq!(app.events.len(), 1);
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            app.finish_job();
            app.source_tick(&ctx);
            if app.api.is_some() && app.events.is_empty() {
                let arrivals = app.call(Command::List { limit: 100 });
                if arrivals["events"]
                    .as_array()
                    .is_some_and(|events| events.iter().any(|e| e["source"] == "downloads"))
                {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.api.is_some(), "worker did not complete");
        assert!(
            app.events.is_empty(),
            "Clipboard duplicate remained in Images: {}",
            app.message
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
    }
}

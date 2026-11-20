use super::*;
use katran::interaction::{ShelfTab, View};

const STARTUP_WAIT: Duration = Duration::from_millis(1250);

enum StartupVisual {
    Image(images::WarmKey, egui::ColorImage),
    Emoji(u64, String, katran::emoji_render::Rendered),
}

#[derive(Default)]
pub(super) struct StartupVisuals {
    rx: Option<mpsc::Receiver<StartupVisual>>,
    ready: Vec<StartupVisual>,
}

impl StartupVisuals {
    fn collect(&mut self) {
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(visual) => self.ready.push(visual),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.rx = None;
                        break;
                    }
                }
            }
        }
    }

    pub(super) fn upload(
        &mut self,
        ctx: &egui::Context,
        images: &mut images::Images,
        pods: &mut pods_ui::State,
    ) {
        self.collect();
        for visual in self.ready.drain(..) {
            match visual {
                StartupVisual::Image(key, pixels) => images.warm_startup(ctx, key, pixels),
                StartupVisual::Emoji(font, text, pixels) => {
                    pods.warm_startup(ctx, font, text, pixels)
                }
            }
        }
        if self.rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(24));
        }
    }
}

struct Ready {
    tab: katran::interaction::ShelfTab,
    stamp: Value,
    result: Value,
}

#[derive(Default)]
pub(super) struct State {
    rx: Option<mpsc::Receiver<Ready>>,
    stamp: Value,
    pages: std::collections::BTreeMap<katran::interaction::ShelfTab, Value>,
    pub waiting: Option<katran::interaction::ShelfTab>,
    finished: bool,
}

impl State {
    pub fn start(
        &mut self,
        root: PathBuf,
        tabs: Vec<katran::interaction::ShelfTab>,
        stamp: Value,
        ctx: Option<egui::Context>,
    ) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.stamp = stamp.clone();
        self.pages.clear();
        self.finished = false;
        std::thread::spawn(move || {
            let Ok(mut api) = Service::open_read_only(&root) else {
                return;
            };
            for tab in tabs {
                let response = api.execute(Request {
                    api_version: 1,
                    request_id: "tab-preload".into(),
                    command: Command::BrowseTabSnapshot {
                        tab,
                        query: String::new(),
                        limit: 128,
                    },
                });
                if response["ok"] == true
                    && tx
                        .send(Ready {
                            tab,
                            stamp: stamp.clone(),
                            result: response["result"].clone(),
                        })
                        .is_ok()
                {
                    if let Some(ctx) = &ctx {
                        ctx.request_repaint();
                    }
                }
            }
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }

    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(ready) => {
                        if ready.stamp == self.stamp {
                            self.pages.insert(ready.tab, ready.result);
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.finished = true;
                        break;
                    }
                }
            }
        }
    }

    pub fn take(&mut self, tab: katran::interaction::ShelfTab, stamp: &Value) -> Option<Value> {
        if self.stamp != *stamp {
            self.pages.clear();
            return None;
        }
        self.pages.get(&tab).cloned()
    }
}

impl Shelf {
    pub(super) fn start_tab_preload(&mut self, ctx: &egui::Context) {
        self.preload_tab_pages(Some(ctx.clone()));
    }

    fn preload_tab_pages(&mut self, ctx: Option<egui::Context>) {
        let Some(root) = self.launch_root.clone() else {
            return;
        };
        let tabs: Vec<katran::interaction::ShelfTab> =
            serde_json::from_value(self.call(Command::Tabs)["visible_tabs"].clone())
                .unwrap_or_default();
        let mut tabs: Vec<_> = tabs
            .into_iter()
            .filter(|tab| {
                !matches!(
                    tab,
                    katran::interaction::ShelfTab::All
                        | katran::interaction::ShelfTab::Desktop
                        | katran::interaction::ShelfTab::Pages
                )
            })
            .collect();
        if let Some(waiting) = self.tab_preload.waiting {
            tabs.sort_by_key(|tab| *tab != waiting);
        } else if let Some(current) =
            serde_json::from_value::<katran::interaction::ShelfTab>(self.state["tab"].clone()).ok()
            && let Some(index) = tabs.iter().position(|tab| *tab == current)
        {
            let order = tabs.clone();
            tabs.sort_by_key(|tab| {
                let position = order
                    .iter()
                    .position(|candidate| candidate == tab)
                    .unwrap_or(index);
                (position == index, position.abs_diff(index))
            });
        }
        let stamp = self.call(Command::CatalogueStamp);
        self.tab_preload.start(root, tabs, stamp, ctx);
    }

    /// Run while the native window is still being created. A busy disk or a
    /// missing font must never leave the launcher waiting indefinitely.
    pub(super) fn prepare_startup_visuals(&mut self, settings: &katran::settings::Settings) {
        let Some(root) = self.launch_root.clone() else {
            return;
        };
        let deadline = Instant::now() + STARTUP_WAIT;
        self.preload_tab_pages(None);
        let metadata_deadline = Instant::now() + Duration::from_millis(150);
        while !self.tab_preload.finished && Instant::now() < metadata_deadline {
            self.tab_preload.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
        let current =
            serde_json::from_value::<ShelfTab>(self.state["tab"].clone()).unwrap_or(ShelfTab::All);
        let mut image_jobs = Vec::new();
        let mut emoji_jobs = Vec::new();
        let mut image_keys = HashSet::new();
        let mut emoji_texts = HashSet::new();
        let current_events = (current, self.events.iter());
        let pages = self
            .tab_preload
            .pages
            .iter()
            .filter(|(tab, _)| **tab != current)
            .filter_map(|(&tab, page)| Some((tab, page["events"].as_array()?.iter())));
        for (tab, events) in std::iter::once(current_events).chain(pages) {
            let gallery = if tab == current {
                self.state["view"] == "gallery"
            } else {
                settings.tab_preferences(tab).view == View::Gallery
            };
            if tab == ShelfTab::Pods && gallery {
                for text in events
                    .take(72)
                    .filter_map(|event| event["text_preview"].as_str())
                {
                    if !text.is_empty() && emoji_texts.insert(text) {
                        emoji_jobs.push(text.to_owned());
                    }
                }
            } else if tab != ShelfTab::Pods {
                let edge = if gallery && tab != ShelfTab::Links {
                    512
                } else {
                    320
                };
                let visible = if gallery { 24 } else { 6 };
                for (position, (key, command)) in events
                    .take(visible)
                    .filter_map(|event| images::startup_image(event, edge))
                    .enumerate()
                {
                    if image_keys.insert(key.clone()) {
                        image_jobs.push((position, key, command));
                    }
                }
            }
        }
        if settings.tab_enabled(ShelfTab::Pods)
            && settings.tab_preferences(ShelfTab::Pods).view == View::Gallery
        {
            for text in katran::pods::catalogue_emoji().take(56) {
                if emoji_texts.insert(text) {
                    emoji_jobs.push(text.to_owned());
                }
            }
        }
        let (tx, rx) = mpsc::sync_channel(12);
        if !emoji_jobs.is_empty() {
            let tx = tx.clone();
            let (generation, font) = self.pods_ui.startup_font();
            std::thread::spawn(move || {
                for text in emoji_jobs {
                    if let Ok(pixels) = katran::emoji_render::render_with_font(
                        &text,
                        pods_ui::GALLERY_RASTER as f32,
                        font.as_deref(),
                    ) && tx
                        .send(StartupVisual::Emoji(generation, text, pixels))
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
        let mut image_lanes = [Vec::new(), Vec::new()];
        image_jobs.sort_by_key(|(position, _, _)| *position);
        for (index, (_, key, command)) in image_jobs.into_iter().enumerate() {
            image_lanes[index % 2].push((key, command));
        }
        for image_jobs in image_lanes.into_iter().filter(|lane| !lane.is_empty()) {
            let tx = tx.clone();
            let root = root.clone();
            std::thread::spawn(move || {
                let Ok(mut api) = Service::open_read_only(&root) else {
                    return;
                };
                let mut image_bytes = 0;
                for (key, command) in image_jobs {
                    let reply = api.execute(Request {
                        api_version: 1,
                        request_id: "startup-thumbnail".into(),
                        command,
                    });
                    if let Some(pixels) = images::startup_pixels(&reply, &key) {
                        image_bytes += pixels.pixels.len() * 4;
                        if image_bytes > 12 * 1024 * 1024
                            || tx.send(StartupVisual::Image(key, pixels)).is_err()
                        {
                            return;
                        }
                    }
                }
            });
        }
        drop(tx);
        self.startup_visuals.rx = Some(rx);
        while self.startup_visuals.rx.is_some() && Instant::now() < deadline {
            self.startup_visuals.collect();
            self.tab_preload.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn show_preloaded_tab(&mut self, ctx: &egui::Context) -> bool {
        if self.launch_root.is_none() {
            return false;
        }
        if !self.query.is_empty()
            || self.state["pinned_only"] == true
            || !self.state["tag_filter"].is_null()
            || !self.state["tag_slot_filter"].is_null()
        {
            return false;
        }
        let Ok(tab) =
            serde_json::from_value::<katran::interaction::ShelfTab>(self.state["tab"].clone())
        else {
            return false;
        };
        // Managed backgrounds can change independently of the catalogue stamp.
        if matches!(
            tab,
            katran::interaction::ShelfTab::All
                | katran::interaction::ShelfTab::Desktop
                | katran::interaction::ShelfTab::Pages
        ) {
            return false;
        }
        self.tab_preload.poll();
        let stamp = self.call(Command::CatalogueStamp);
        if self.tab_preload.stamp != stamp {
            self.tab_preload.waiting = Some(tab);
            self.start_tab_preload(ctx);
        }
        if let Some(page) = self.tab_preload.take(tab, &stamp) {
            self.events = page["events"].as_array().cloned().unwrap_or_default();
            self.next_cursor = None;
            self.preloaded_has_more = page["has_more"] == true;
            self.loaded_query.clear();
            self.tab_preload.waiting = None;
            self.seed_preloaded_tags(&page);
        } else {
            if self.tab_preload.finished {
                return false;
            }
            self.events.clear();
            self.next_cursor = None;
            self.preloaded_has_more = false;
            self.tab_preload.waiting = Some(tab);
        }
        ctx.request_repaint();
        true
    }

    pub(super) fn poll_preloaded_tab(&mut self, ctx: &egui::Context) {
        self.tab_preload.poll();
        let Some(tab) = self.tab_preload.waiting else {
            return;
        };
        if serde_json::from_value::<katran::interaction::ShelfTab>(self.state["tab"].clone()).ok()
            != Some(tab)
            || self.state["pinned_only"] == true
            || !self.state["tag_filter"].is_null()
            || !self.state["tag_slot_filter"].is_null()
        {
            self.tab_preload.waiting = None;
            return;
        }
        let stamp = self.call(Command::CatalogueStamp);
        if self.tab_preload.stamp != stamp {
            self.start_tab_preload(ctx);
            return;
        }
        if let Some(page) = self.tab_preload.take(tab, &stamp) {
            self.events = page["events"].as_array().cloned().unwrap_or_default();
            self.next_cursor = None;
            self.preloaded_has_more = page["has_more"] == true;
            self.tab_preload.waiting = None;
            self.seed_preloaded_tags(&page);
            ctx.request_repaint();
        } else if self.tab_preload.finished {
            self.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_switch_waits_for_background_page_without_showing_previous_tab() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let ingested = app.call(Command::IngestBytes {
            name: "synthetic.png".into(),
            bytes: vec![1, 2, 3],
            source: "synthetic".into(),
        });
        assert!(ingested["id"].is_i64());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Links,
            enabled: true,
        });
        app.launch_root = Some(root.path().into());
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let ctx = egui::Context::default();
        app.start_tab_preload(&ctx);
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Links,
        });
        let displayed = app.show_preloaded_tab(&ctx);
        let stamp = app.call(Command::CatalogueStamp);
        assert!(
            displayed,
            "state={} message={} stamp={} preload_stamp={} finished={}",
            app.state, app.message, stamp, app.tab_preload.stamp, app.tab_preload.finished
        );
        assert!(app.events.is_empty());
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.tab_preload.waiting.is_some() && Instant::now() < deadline {
            app.poll_preloaded_tab(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.tab_preload.waiting.is_none());
        assert!(app.events.is_empty());
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        assert!(app.show_preloaded_tab(&ctx));
        while app.tab_preload.waiting.is_some() && Instant::now() < deadline {
            app.poll_preloaded_tab(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.tab_preload.waiting.is_none());
        assert_eq!(app.events.len(), 1);
    }

    #[test]
    fn changed_catalogue_discards_prefetched_page() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.launch_root = Some(root.path().into());
        app.state = app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        let ctx = egui::Context::default();
        app.start_tab_preload(&ctx);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.tab_preload.finished && Instant::now() < deadline {
            app.tab_preload.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.tab_preload.finished);
        app.call(Command::IngestBytes {
            name: "new-synthetic.png".into(),
            bytes: vec![4, 5, 6],
            source: "synthetic".into(),
        });
        assert!(app.show_preloaded_tab(&ctx));
        assert!(app.events.is_empty());
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.tab_preload.waiting.is_some() && Instant::now() < deadline {
            app.poll_preloaded_tab(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.tab_preload.waiting.is_none());
        assert_eq!(app.events.len(), 1);
    }
}

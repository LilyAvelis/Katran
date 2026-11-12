use super::*;

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
        ctx: egui::Context,
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
                        limit: 1000,
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
                    ctx.request_repaint();
                }
            }
            ctx.request_repaint();
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
        let Some(root) = self.launch_root.clone() else {
            return;
        };
        let tabs: Vec<katran::interaction::ShelfTab> =
            serde_json::from_value(self.call(Command::Tabs)["visible_tabs"].clone())
                .unwrap_or_default();
        let mut tabs: Vec<_> = tabs
            .into_iter()
            .filter(|tab| *tab != katran::interaction::ShelfTab::All)
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
        self.tab_preload.start(root, tabs, stamp, ctx.clone());
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
        if tab == katran::interaction::ShelfTab::All {
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

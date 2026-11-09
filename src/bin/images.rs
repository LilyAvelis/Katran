use super::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

// A key describes the event contents, not merely its potentially reusable row ID.
type ImageKey = (i64, u32, String);
type Work = (ImageKey, egui::Context);
struct Queue {
    pending: VecDeque<Work>,
    closed: bool,
}
struct Entry {
    texture: Option<egui::TextureHandle>,
    touched: u64,
    retry_after: Instant,
}
pub(super) struct Images {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: mpsc::Receiver<(ImageKey, Value)>,
    active: HashSet<ImageKey>,
    requested: HashSet<ImageKey>,
    textures: HashMap<ImageKey, Entry>,
    clock: u64,
}
impl Images {
    pub(super) fn new(root: PathBuf) -> Self {
        let queue = Arc::new((
            Mutex::new(Queue {
                pending: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let (results, rx) = mpsc::sync_channel(4);
        // Two bounded workers let a slow, obsolete decode coexist with current work.
        for _ in 0..2 {
            let queue = Arc::clone(&queue);
            let root = root.clone();
            let results = results.clone();
            std::thread::spawn(move || {
                let Ok(mut api) = Service::open_read_only(&root) else {
                    return;
                };
                loop {
                    let (key, ctx) = {
                        let (lock, wake) = &*queue;
                        let mut state = lock.lock().unwrap();
                        while state.pending.is_empty() && !state.closed {
                            state = wake.wait(state).unwrap();
                        }
                        if state.closed {
                            return;
                        }
                        state.pending.pop_front().unwrap()
                    };
                    let reply = api.execute(Request {
                        api_version: 1,
                        request_id: "preview".into(),
                        command: Command::PreviewImage {
                            id: key.0,
                            max_edge: key.1,
                        },
                    });
                    if results.send((key, reply)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Self {
            queue,
            rx,
            active: HashSet::new(),
            requested: HashSet::new(),
            textures: HashMap::new(),
            clock: 0,
        }
    }
    pub(super) fn begin_frame(&mut self) {
        self.active.clear();
    }
    pub(super) fn end_frame(&mut self, ctx: &egui::Context) {
        while let Ok((result_key, reply)) = self.rx.try_recv() {
            self.requested.remove(&result_key);
            // A stale result cannot populate the cache for a different event.
            if !self.active.contains(&result_key)
                || (reply["ok"] == true
                    && reply["result"]["revision"].as_str() != Some(result_key.2.as_str()))
            {
                continue;
            }
            let texture = (|| {
                let p = &reply["result"];
                let w = p["width"].as_u64()? as usize;
                let h = p["height"].as_u64()? as usize;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(p["rgba_base64"].as_str()?)
                    .ok()?;
                if w.checked_mul(h)?.checked_mul(4)? != bytes.len() {
                    return None;
                }
                Some(ctx.load_texture(
                    format!("image-{result_key:?}"),
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &bytes),
                    egui::TextureOptions::LINEAR,
                ))
            })();
            ctx.request_repaint();
            self.textures.insert(
                result_key,
                Entry {
                    texture,
                    touched: self.clock,
                    retry_after: Instant::now() + Duration::from_secs(3),
                },
            );
        }
        let (lock, _) = &*self.queue;
        let mut state = lock.lock().unwrap();
        state.pending.retain(|(key, _)| self.active.contains(key));
        // Requests removed before execution must be eligible if the user returns.
        self.requested.retain(|key| self.active.contains(key));
    }
    pub(super) fn draw(&mut self, ui: &mut egui::Ui, event: &Value, size: Vec2, edge: u32) {
        let Some(id) = event["id"].as_i64() else {
            return;
        };
        let Some(revision) = event["revision"].as_str() else {
            return;
        };
        let key = (id, edge, revision.to_owned());
        self.active.insert(key.clone());
        self.clock = self.clock.wrapping_add(1);
        let retry = self
            .textures
            .get(&key)
            .is_none_or(|e| e.texture.is_none() && Instant::now() >= e.retry_after);
        if retry && self.requested.insert(key.clone()) {
            let (lock, wake) = &*self.queue;
            let mut state = lock.lock().unwrap();
            if state.pending.len() >= 64
                && let Some((old, _)) = state.pending.pop_back()
            {
                self.requested.remove(&old);
            }
            state.pending.push_front((key.clone(), ui.ctx().clone()));
            wake.notify_one();
        }
        if let Some(entry) = self.textures.get_mut(&key) {
            entry.touched = self.clock;
        }
        while self.textures.len() > 96
            || self
                .textures
                .values()
                .filter_map(|e| e.texture.as_ref())
                .map(|t| t.size()[0] * t.size()[1] * 4)
                .sum::<usize>()
                > 48 * 1024 * 1024
        {
            let Some(oldest) = self
                .textures
                .iter()
                .filter(|(k, _)| **k != key)
                .min_by_key(|(_, e)| e.touched)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.textures.remove(&oldest);
        }
        if let Some(Some(texture)) = self.textures.get(&key).map(|e| &e.texture) {
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(size)
                    .maintain_aspect_ratio(true),
            );
        } else {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 6.0, Color32::from_gray(34));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.textures.contains_key(&key) {
                    "Нет превью"
                } else {
                    "…"
                },
                egui::FontId::proportional(14.0),
                Color32::GRAY,
            );
            if self.textures.contains_key(&key) {
                ui.ctx().request_repaint_after(Duration::from_secs(3));
            }
        }
    }
}
impl Drop for Images {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap().closed = true;
        wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_discards_invisible_requests_and_bounds_backlog() {
        let queue = Arc::new((
            Mutex::new(Queue {
                pending: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let (_tx, rx) = mpsc::channel();
        let mut images = Images {
            queue: queue.clone(),
            rx,
            active: HashSet::new(),
            requested: HashSet::new(),
            textures: HashMap::new(),
            clock: 0,
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            images.begin_frame();
            for id in 0..80 {
                images.draw(
                    ui,
                    &serde_json::json!({"id": id, "revision": format!("r{id}")}),
                    Vec2::splat(10.0),
                    320,
                );
            }
            images.end_frame(&ctx);
        });
        output.textures_delta.clear();
        assert_eq!(queue.0.lock().unwrap().pending.len(), 64);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            images.begin_frame();
            images.draw(
                ui,
                &serde_json::json!({"id": 99, "revision": "new"}),
                Vec2::splat(10.0),
                320,
            );
            images.end_frame(&ctx);
        });
        output.textures_delta.clear();
        let state = queue.0.lock().unwrap();
        assert_eq!(state.pending.len(), 1);
        assert_eq!(state.pending[0].0.0, 99);
    }
}

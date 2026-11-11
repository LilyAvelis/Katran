use super::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

// A key describes the event contents, not merely its potentially reusable row ID.
type ImageKey = (i64, u32, String);
type Work = (ImageKey, Option<PathBuf>, egui::Context);
struct Queue {
    pending: VecDeque<Work>,
    closed: bool,
}
struct Entry {
    texture: Option<egui::TextureHandle>,
    icon: bool,
    touched: u64,
    retry_after: Instant,
}
fn preview_command(id: i64, edge: u32) -> Command {
    if edge <= 512 {
        Command::PreviewFile { id, max_edge: edge }
    } else {
        Command::PreviewImage { id, max_edge: edge }
    }
}
fn work_path(event: &Value, id: i64) -> Option<Option<PathBuf>> {
    if id < 0 {
        Some(Some(PathBuf::from(event["path"].as_str()?)))
    } else {
        Some(None)
    }
}
fn work_command(key: &ImageKey, path: Option<PathBuf>) -> Command {
    match path {
        Some(path) => Command::PreviewPath {
            path,
            revision: key.2.clone(),
            max_edge: key.1,
            image_only: key.1 > 512,
        },
        None => preview_command(key.0, key.1),
    }
}
// Only the latest successful domain rectangle is retained during dispatcher jobs.
#[derive(Default)]
struct PreviewGeometryCache(Option<(ImageKey, egui::Rect, egui::Rect)>);
impl PreviewGeometryCache {
    fn resolve(
        &mut self,
        key: ImageKey,
        canvas: egui::Rect,
        image: Option<egui::Rect>,
    ) -> Option<egui::Rect> {
        if let Some(image) = image {
            self.0 = Some((key, canvas, image));
            return Some(image);
        }
        if self
            .0
            .as_ref()
            .is_some_and(|(old, old_canvas, _)| old != &key || *old_canvas != canvas)
        {
            self.0 = None;
        }
        self.0.as_ref().map(|(_, _, image)| *image)
    }
}
pub(super) struct Images {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: mpsc::Receiver<(ImageKey, Value)>,
    active: HashSet<ImageKey>,
    requested: HashSet<ImageKey>,
    textures: HashMap<ImageKey, Entry>,
    clock: u64,
    preview_geometry: PreviewGeometryCache,
}
impl Images {
    pub(super) fn prefetch(&mut self, ctx: &egui::Context, event: &Value, edge: u32) {
        if matches!(
            event["kind"].as_str(),
            Some("text_fragment" | "link_fragment" | "table_fragment")
        ) || katran::file_types::is_markdown(std::path::Path::new(
            event["name"].as_str().unwrap_or(""),
        )) {
            return;
        }
        let (Some(id), Some(revision)) = (event["id"].as_i64(), event["revision"].as_str()) else {
            return;
        };
        let key = (id, edge, revision.to_owned());
        let Some(path) = work_path(event, id) else {
            return;
        };
        self.active.insert(key.clone());
        if self
            .textures
            .get(&key)
            .is_some_and(|e| e.texture.is_some() || Instant::now() < e.retry_after)
        {
            return;
        }
        if self.requested.insert(key.clone()) {
            let (lock, wake) = &*self.queue;
            let mut state = lock.lock().unwrap();
            if state.pending.len() < 64 {
                state.pending.push_back((key, path, ctx.clone()));
                wake.notify_one();
            } else {
                self.requested.remove(&key);
            }
        }
    }
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
                    let (key, path, ctx) = {
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
                        command: work_command(&key, path),
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
            preview_geometry: PreviewGeometryCache::default(),
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
                    icon: matches!(
                        reply["result"]["visual_kind"].as_str(),
                        Some("shell_icon" | "generic_icon")
                    ),
                    touched: self.clock,
                    retry_after: Instant::now() + Duration::from_secs(3),
                },
            );
        }
        let (lock, _) = &*self.queue;
        let mut state = lock.lock().unwrap();
        state
            .pending
            .retain(|(key, _, _)| self.active.contains(key));
        // Requests removed before execution must be eligible if the user returns.
        self.requested.retain(|key| self.active.contains(key));
    }
    #[cfg(test)]
    pub(super) fn draw(&mut self, ui: &mut egui::Ui, event: &Value, size: Vec2, edge: u32) {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        self.paint_at(ui, event, rect, edge);
    }
    pub(super) fn clear_preview_geometry(&mut self) {
        self.preview_geometry = PreviewGeometryCache::default();
    }
    pub(super) fn fitted_preview_size(&self, event: &Value, canvas: egui::Rect) -> Option<Vec2> {
        let key = (
            event["id"].as_i64()?,
            1400,
            event["revision"].as_str()?.to_owned(),
        );
        let texture = self.textures.get(&key)?.texture.as_ref()?;
        Some(super::gallery_card::fit_rect(canvas, texture.size_vec2()).size())
    }
    pub(super) fn paint_preview(
        &mut self,
        ui: &mut egui::Ui,
        event: &Value,
        canvas: egui::Rect,
        image: Option<egui::Rect>,
    ) {
        let image = event["id"]
            .as_i64()
            .zip(event["revision"].as_str())
            .and_then(|(id, revision)| {
                self.preview_geometry
                    .resolve((id, 1400, revision.to_owned()), canvas, image)
            });
        self.paint_image(ui, event, canvas, 1400, image);
    }
    pub(super) fn paint_at(
        &mut self,
        ui: &mut egui::Ui,
        event: &Value,
        rect: egui::Rect,
        edge: u32,
    ) {
        self.paint_image(ui, event, rect, edge, None);
    }
    fn paint_image(
        &mut self,
        ui: &mut egui::Ui,
        event: &Value,
        rect: egui::Rect,
        edge: u32,
        image: Option<egui::Rect>,
    ) {
        let Some(id) = event["id"].as_i64() else {
            return;
        };
        let Some(revision) = event["revision"].as_str() else {
            return;
        };
        let key = (id, edge, revision.to_owned());
        let Some(path) = work_path(event, id) else {
            return;
        };
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
                && let Some((old, _, _)) = state.pending.pop_back()
            {
                self.requested.remove(&old);
            }
            state
                .pending
                .push_front((key.clone(), path, ui.ctx().clone()));
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
            let image = image.unwrap_or_else(|| {
                let rect = if self.textures.get(&key).is_some_and(|entry| entry.icon) {
                    egui::Rect::from_center_size(
                        rect.center(),
                        Vec2::splat(96.0_f32.min(rect.width()).min(rect.height())),
                    )
                } else {
                    rect
                };
                super::gallery_card::fit_rect(rect, texture.size_vec2())
            });
            ui.painter_at(rect).image(
                texture.id(),
                image,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter_at(rect)
                .rect_filled(rect, 6.0, Color32::from_gray(34));
            ui.painter_at(rect).text(
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
    fn text_never_enqueues_shell_or_image_preview() {
        let root = tempfile::tempdir().unwrap();
        let mut images = Images::new(root.path().into());
        images.prefetch(
            &egui::Context::default(),
            &serde_json::json!({"id": 1, "revision":"blob:a", "kind":"text_fragment"}),
            320,
        );
        assert!(images.requested.is_empty());
        assert!(images.queue.0.lock().unwrap().pending.is_empty());
    }

    #[test]
    fn cards_use_file_visuals_and_large_preview_keeps_image_decoder() {
        assert!(matches!(
            preview_command(1, 320),
            Command::PreviewFile {
                id: 1,
                max_edge: 320
            }
        ));
        assert!(matches!(
            preview_command(1, 512),
            Command::PreviewFile {
                id: 1,
                max_edge: 512
            }
        ));
        assert!(matches!(
            preview_command(1, 1400),
            Command::PreviewImage {
                id: 1,
                max_edge: 1400
            }
        ));
    }

    #[test]
    fn folder_preview_carries_path_revision_and_size() {
        let event =
            serde_json::json!({"id": -1, "path": "folder/image.png", "revision": "file:abc"});
        let path = work_path(&event, -1).unwrap();
        let key = (-1, 320, "file:abc".to_owned());
        assert!(
            matches!(work_command(&key, path), Command::PreviewPath { path, revision, max_edge: 320, image_only: false } if path == std::path::Path::new("folder/image.png") && revision == "file:abc")
        );
        assert!(work_path(&serde_json::json!({}), -1).is_none());
    }

    #[test]
    fn busy_preview_reuses_last_domain_geometry_without_resetting_zoom() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 100).unwrap();
        let mut call = |command| {
            api.execute(Request {
                api_version: 1,
                request_id: "render-cache".into(),
                command,
            })
        };
        call(Command::IngestBytes {
            name: "geometry.png".into(),
            bytes: vec![1],
            source: "test".into(),
        });
        call(Command::SetVisible { ids: vec![1] });
        call(Command::OpenCurrent);
        use katran::preview_view::{Point, Size};
        let size = Size {
            width: 400.0,
            height: 400.0,
        };
        call(Command::ZoomPreview {
            multiplier: 2.0,
            anchor: Point { x: 300.0, y: 250.0 },
            viewport: size,
            fitted: size,
        });
        let g = call(Command::PreviewGeometry {
            viewport: size,
            fitted: size,
        });
        let g = &g["result"];
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 40.0), Vec2::splat(400.0));
        let image = egui::Rect::from_min_size(
            canvas.min
                + Vec2::new(
                    g["left"].as_f64().unwrap() as f32,
                    g["top"].as_f64().unwrap() as f32,
                ),
            Vec2::new(
                g["width"].as_f64().unwrap() as f32,
                g["height"].as_f64().unwrap() as f32,
            ),
        );
        let key = (1, 1400, "synthetic".to_owned());
        let mut cache = PreviewGeometryCache::default();
        assert_eq!(cache.resolve(key.clone(), canvas, Some(image)), Some(image));
        // A background job temporarily owns the dispatcher: no new rectangle.
        for _ in 0..3 {
            assert_eq!(cache.resolve(key.clone(), canvas, None), Some(image));
        }
        assert_ne!(image, canvas);
        assert_eq!(cache.resolve((2, 1400, "other".into()), canvas, None), None);
        assert_eq!(cache.resolve(key.clone(), canvas, None), None);
        cache.resolve(key.clone(), canvas, Some(image));
        assert_eq!(
            cache.resolve(key, canvas.translate(Vec2::splat(1.0)), None),
            None
        );
    }

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
            preview_geometry: PreviewGeometryCache::default(),
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

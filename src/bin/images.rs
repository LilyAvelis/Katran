use super::i18n::tr;
use super::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

// A key describes the event contents, not merely its potentially reusable row ID.
type ImageKey = (i64, u32, String, u64);

/// Identity of the browse context, independent of cursor movement.
pub(super) fn preview_context(state: &Value) -> String {
    serde_json::json!([
        state["tab"],
        state["folder_path"],
        state["emoji_catalogue"],
        state["view_generation"]
    ])
    .to_string()
}
/// Namespace excludes generation but still separates tabs, folders and catalogue.
fn context_namespace(context: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(context).ok()?;
    let parts = parsed.as_array()?;
    (parts.len() == 4).then(|| serde_json::json!([parts[0], parts[1], parts[2]]).to_string())
}
type Work = (ImageKey, Option<PathBuf>, bool, egui::Context);
struct Queue {
    pending: VecDeque<Work>,
    closed: bool,
}
struct Entry {
    texture: Option<egui::TextureHandle>,
    icon: bool,
    visual_status: &'static str,
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
    if id >= 0 || event["source"] == "desktop" {
        Some(None)
    } else if event["source"] == "work_folder" {
        Some(Some(PathBuf::from(event["path"].as_str()?)))
    } else {
        None
    }
}
fn work_command(key: &ImageKey, path: Option<PathBuf>, collection: bool) -> Command {
    match path {
        Some(path) => Command::PreviewPath {
            path,
            revision: key.2.clone(),
            max_edge: key.1,
            image_only: key.1 > 512,
        },
        None if collection => Command::PreviewImage {
            id: key.0,
            max_edge: key.1,
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
    context: String,
    context_epoch: u64,
}
impl Images {
    pub(super) fn prefetch(&mut self, ctx: &egui::Context, event: &Value, edge: u32) {
        if matches!(
            event["source"].as_str(),
            Some("link_collection" | "text_collection")
        ) {
            return;
        }
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
        let key = (id, edge, revision.to_owned(), self.context_epoch);
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
                state
                    .pending
                    .push_back((key, path, is_managed_collection(event), ctx.clone()));
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
                    let (key, path, collection, ctx) = {
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
                        command: work_command(&key, path, collection),
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
            context: String::new(),
            context_epoch: 0,
        }
    }
    /// A checked tag-style save changes only the view generation. Retain image
    /// work only when both interaction snapshots and our current context prove it.
    pub(super) fn accept_metadata_generation(&mut self, before: &Value, after: &Value) -> bool {
        let (Some(old), Some(new)) = (
            before["view_generation"].as_u64(),
            after["view_generation"].as_u64(),
        ) else {
            return false;
        };
        if old.wrapping_add(1) != new || self.context != preview_context(before) {
            return false;
        }
        let (Some(mut a), Some(mut b)) = (before.as_object().cloned(), after.as_object().cloned())
        else {
            return false;
        };
        a.remove("view_generation");
        b.remove("view_generation");
        if a != b {
            return false;
        }
        self.context = preview_context(after);
        true
    }
    #[cfg(test)]
    pub(super) fn test_seed_texture(
        &mut self,
        ctx: &egui::Context,
        event: &Value,
        edge: u32,
    ) -> egui::TextureId {
        let key = (
            event["id"].as_i64().unwrap(),
            edge,
            event["revision"].as_str().unwrap().to_owned(),
            self.context_epoch,
        );
        let texture = ctx.load_texture(
            format!("fixture-{key:?}"),
            egui::ColorImage::filled([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let id = texture.id();
        self.textures.insert(
            key,
            Entry {
                texture: Some(texture),
                icon: false,
                visual_status: "image",
                touched: 0,
                retry_after: Instant::now(),
            },
        );
        id
    }
    #[cfg(test)]
    pub(super) fn test_texture_id(&self, event: &Value, edge: u32) -> Option<egui::TextureId> {
        let key = (
            event["id"].as_i64()?,
            edge,
            event["revision"].as_str()?.to_owned(),
            self.context_epoch,
        );
        self.textures
            .get(&key)?
            .texture
            .as_ref()
            .map(|texture| texture.id())
    }
    #[cfg(test)]
    pub(super) fn test_is_requested(&self, event: &Value, edge: u32) -> bool {
        let (Some(id), Some(revision)) = (event["id"].as_i64(), event["revision"].as_str()) else {
            return false;
        };
        self.requested
            .contains(&(id, edge, revision.to_owned(), self.context_epoch))
    }
    pub(super) fn set_context(&mut self, state: &Value) {
        let context = preview_context(state);
        if self.context != context {
            let same_namespace = context_namespace(&self.context).is_some()
                && context_namespace(&self.context) == context_namespace(&context);
            let old_epoch = self.context_epoch;
            self.context = context;
            self.context_epoch = self.context_epoch.wrapping_add(1);
            self.active.clear();
            self.requested.clear();
            // A generation change invalidates all pending work, but decoded
            // image pixels remain valid for the same id, revision and edge.
            // Shell-derived visuals also depend on path/name and system state,
            // which are not captured by ImageKey, so they are never carried.
            let old = std::mem::take(&mut self.textures);
            if same_namespace {
                self.textures = old
                    .into_iter()
                    .filter_map(|((id, edge, revision, epoch), entry)| {
                        (epoch == old_epoch
                            && entry.visual_status == "image"
                            && entry.texture.is_some())
                        .then_some(((id, edge, revision, self.context_epoch), entry))
                    })
                    .collect();
            }
            self.preview_geometry = PreviewGeometryCache::default();
            if let Ok(mut queue) = self.queue.0.lock() {
                queue.pending.clear();
            }
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
            let visual_status = if texture.is_none() {
                "unavailable"
            } else {
                match reply["result"]["visual_kind"].as_str() {
                    Some("shell_thumbnail") => "shell_thumbnail",
                    Some("shell_icon") => "shell_icon",
                    Some("generic_icon") => "generic_icon",
                    _ => "image",
                }
            };
            ctx.request_repaint();
            self.textures.insert(
                result_key,
                Entry {
                    texture,
                    icon: matches!(
                        reply["result"]["visual_kind"].as_str(),
                        Some("shell_icon" | "generic_icon")
                    ),
                    visual_status,
                    touched: self.clock,
                    retry_after: Instant::now() + Duration::from_secs(3),
                },
            );
        }
        let (lock, _) = &*self.queue;
        let mut state = lock.lock().unwrap();
        state.pending.retain(|(key, ..)| self.active.contains(key));
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
            self.context_epoch,
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
                self.preview_geometry.resolve(
                    (id, 1400, revision.to_owned(), self.context_epoch),
                    canvas,
                    image,
                )
            });
        self.paint_image(ui, event, canvas, 1400, image);
    }
    pub(super) fn visual_status(&self, event: &Value, edge: u32) -> &'static str {
        let (Some(id), Some(revision)) = (event["id"].as_i64(), event["revision"].as_str()) else {
            return "unavailable";
        };
        if work_path(event, id).is_none() {
            return "unavailable";
        }
        let key = (id, edge, revision.to_owned(), self.context_epoch);
        self.textures
            .get(&key)
            .map_or("loading", |entry| entry.visual_status)
    }
    pub(super) fn paint_at(
        &mut self,
        ui: &mut egui::Ui,
        event: &Value,
        rect: egui::Rect,
        edge: u32,
    ) {
        if matches!(
            event["source"].as_str(),
            Some("link_collection" | "text_collection")
        ) && is_bull(event)
        {
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 6.0, Color32::from_gray(34));
            let size = egui::vec2(30.0_f32.min(rect.width()), 20.0_f32.min(rect.height()));
            paint_bull_badge(
                ui,
                egui::Rect::from_min_size(rect.right_bottom() - size, size),
            );
            return;
        }
        let cover_rect =
            if is_managed_collection(event) && rect.width() >= 48.0 && rect.height() >= 48.0 {
                let frame = rect.shrink2(egui::vec2(7.0, 6.0));
                let painter = ui.painter_at(rect);
                painter.rect_filled(
                    frame.translate(egui::vec2(5.0, 4.0)),
                    4.0,
                    Color32::from_gray(55),
                );
                painter.rect_filled(
                    frame.translate(egui::vec2(2.5, 2.0)),
                    4.0,
                    Color32::from_gray(75),
                );
                frame
            } else {
                rect
            };
        self.paint_image(ui, event, cover_rect, edge, None);
        if is_bs(event) {
            let size = egui::vec2(30.0_f32.min(rect.width()), 20.0_f32.min(rect.height()));
            paint_bs_badge(
                ui,
                egui::Rect::from_min_size(rect.right_bottom() - size, size),
            );
        } else if is_bull(event) {
            let size = egui::vec2(30.0_f32.min(rect.width()), 20.0_f32.min(rect.height()));
            let badge = egui::Rect::from_min_size(rect.right_bottom() - size, size);
            paint_bull_badge(ui, badge);
        }
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
        let key = (id, edge, revision.to_owned(), self.context_epoch);
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
                && let Some((old, _, _, _)) = state.pending.pop_back()
            {
                self.requested.remove(&old);
            }
            state.pending.push_front((
                key.clone(),
                path,
                is_managed_collection(event),
                ui.ctx().clone(),
            ));
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
            if !is_managed_collection(event) || self.textures.contains_key(&key) {
                ui.painter_at(rect).text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if self.textures.contains_key(&key) {
                        tr("Нет превью", "No preview")
                    } else {
                        "…"
                    },
                    egui::FontId::proportional(14.0),
                    Color32::GRAY,
                );
            }
            if self.textures.contains_key(&key) {
                ui.ctx().request_repaint_after(Duration::from_secs(3));
            }
        }
    }
}

fn is_managed_collection(event: &Value) -> bool {
    matches!(
        event["source"].as_str(),
        Some("image_translation_collection" | "image_collection" | "file_image_collection")
    ) && is_bull(event)
}

/// Managed collection blobs have digest paths; ordinary files use their actual path.
pub(super) fn is_bull(event: &Value) -> bool {
    fn extension_is_bull(name: &str) -> bool {
        std::path::Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("bull"))
    }
    if matches!(
        event["source"].as_str(),
        Some(
            "image_translation_collection"
                | "image_collection"
                | "file_image_collection"
                | "file_bull_collection"
                | "link_collection"
                | "text_collection"
        )
    ) {
        return event["name"].as_str().is_some_and(extension_is_bull);
    }
    event["path"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| event["name"].as_str())
        .is_some_and(extension_is_bull)
}

pub(super) fn is_bs(event: &Value) -> bool {
    let value = if event["source"] == "work_folder" {
        event["path"].as_str()
    } else if event["source"] == "bs_collection" || event["source"] == "files" {
        event["name"].as_str()
    } else {
        None
    };
    value
        .and_then(|path| std::path::Path::new(path).extension())
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bs"))
}

pub(super) fn paint_bs_badge(ui: &egui::Ui, rect: egui::Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(52, 72, 86));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "BS",
        egui::FontId::proportional((rect.height() * 0.62).clamp(8.0, 13.0)),
        Color32::WHITE,
    );
}

pub(super) fn paint_bull_badge(ui: &egui::Ui, rect: egui::Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(46, 52, 62));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "BK",
        egui::FontId::proportional((rect.height() * 0.62).clamp(8.0, 13.0)),
        Color32::WHITE,
    );
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
    fn bs_badge_requires_managed_source_and_exact_extension() {
        use serde_json::json;
        assert!(is_bs(
            &json!({"source":"bs_collection", "name":"files.BS", "path":"digest"})
        ));
        assert!(!is_bs(
            &json!({"source":"bs_collection", "name":"files.bs.png", "path":"digest"})
        ));
        assert!(!is_bs(
            &json!({"source":"manual", "name":"files.bs", "path":"digest"})
        ));
        assert!(is_bs(
            &json!({"source":"work_folder", "name":"files", "path":"C:/folder/files.BS"})
        ));
        assert!(!is_bs(
            &json!({"source":"work_folder", "name":"files.bs", "path":"C:/folder/files.bs.png"})
        ));
    }

    #[test]
    fn bull_badge_uses_exact_actual_extension() {
        use serde_json::json;
        assert!(is_bull(
            &json!({"path": "C:/media/album.bull", "name": "album"})
        ));
        assert!(is_bull(&json!({"path": "C:/media/album.BuLl"})));
        assert!(is_bull(&json!({"name": "album.bull"})));
        assert!(is_bull(
            &json!({"source": "image_translation_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "album.bull"})
        ));
        assert!(is_bull(
            &json!({"source": "image_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "album.bull"})
        ));
        assert!(is_bull(
            &json!({"source": "link_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "links.bull"})
        ));
        assert!(is_bull(
            &json!({"source": "text_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "texts.bull"})
        ));
        assert!(!is_managed_collection(
            &json!({"source": "text_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "texts.bull"})
        ));
        assert!(!is_managed_collection(
            &json!({"source": "link_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "links.bull"})
        ));
        assert!(!is_bull(
            &json!({"source": "image_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "album.bull.png"})
        ));
        assert!(!is_bull(
            &json!({"source": "image_translation_collection", "path": "C:/profile/blobs/0123456789abcdef", "name": "album.bull.png"})
        ));
        assert!(!is_bull(
            &json!({"source": "manual", "path": "C:/profile/blobs/0123456789abcdef", "name": "album.bull"})
        ));
        assert!(!is_bull(
            &json!({"path": "C:/media/album.bull.png", "name": "album.bull"})
        ));
        assert!(!is_bull(
            &json!({"path": "C:/media/bull", "name": "album.bull"})
        ));
        assert!(!is_bull(&json!({"path": "C:/media/bull/album.png"})));
        assert!(!is_bull(&json!({})));
    }

    #[test]
    fn tag_filter_generation_retains_decoded_texture() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 10_000).unwrap();
        let mut call = |command| {
            let reply = api.execute(Request {
                api_version: 1,
                request_id: "resident-filter".into(),
                command,
            });
            assert_eq!(reply["ok"], true, "{reply}");
            reply["result"].clone()
        };
        let mut images = Images::new(root.path().into());
        let before = call(Command::Interaction);
        images.set_context(&before);
        let ctx = egui::Context::default();
        let event = serde_json::json!({"id":1,"revision":"blob:synthetic","kind":"managed_image"});
        let id = images.test_seed_texture(&ctx, &event, 320);
        let filtered = call(Command::FilterTagSlot { slot: Some(1) });
        assert_ne!(filtered["view_generation"], before["view_generation"]);
        images.set_context(&filtered);
        assert_eq!(images.test_texture_id(&event, 320), Some(id));
        let cleared = call(Command::FilterTagSlot { slot: Some(1) });
        images.set_context(&cleared);
        assert_eq!(images.test_texture_id(&event, 320), Some(id));
    }

    #[test]
    fn resident_decoded_images_survive_generation_but_not_namespace_or_revision() {
        let queue = Arc::new((
            Mutex::new(Queue {
                pending: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = mpsc::channel();
        let mut images = Images {
            queue,
            rx,
            active: HashSet::new(),
            requested: HashSet::new(),
            textures: HashMap::new(),
            clock: 0,
            preview_geometry: PreviewGeometryCache::default(),
            context: String::new(),
            context_epoch: 0,
        };
        let ctx = egui::Context::default();
        let a = serde_json::json!({"tab":"images","folder_path":null,"emoji_catalogue":false,"view_generation":1});
        let b = serde_json::json!({"tab":"images","folder_path":null,"emoji_catalogue":false,"view_generation":2});
        images.set_context(&a);
        let old_epoch = images.context_epoch;
        let old = (1, 320, "blob:old".to_owned(), old_epoch);
        let pending = (2, 320, "blob:pending".to_owned(), old_epoch);
        let make_texture = |name| {
            ctx.load_texture(
                name,
                egui::ColorImage::filled([1, 1], egui::Color32::WHITE),
                egui::TextureOptions::LINEAR,
            )
        };
        let texture = make_texture("resident-image");
        let texture_id = texture.id();
        images.textures.insert(
            old.clone(),
            Entry {
                texture: Some(texture),
                icon: false,
                visual_status: "image",
                touched: 1,
                retry_after: Instant::now(),
            },
        );
        images.textures.insert(
            (3, 320, "file:shell".to_owned(), old_epoch),
            Entry {
                texture: Some(make_texture("resident-shell")),
                icon: true,
                visual_status: "shell_icon",
                touched: 1,
                retry_after: Instant::now(),
            },
        );
        images.textures.insert(
            (4, 320, "file:thumbnail".to_owned(), old_epoch),
            Entry {
                texture: Some(make_texture("resident-thumb")),
                icon: false,
                visual_status: "shell_thumbnail",
                touched: 1,
                retry_after: Instant::now(),
            },
        );
        images.requested.insert(pending.clone());
        images.queue.0.lock().unwrap().pending.push_back((
            pending.clone(),
            None,
            false,
            ctx.clone(),
        ));
        images.set_context(&b);
        let new_epoch = images.context_epoch;
        assert_ne!(old_epoch, new_epoch);
        let current = (1, 320, "blob:old".to_owned(), new_epoch);
        assert_eq!(images.textures.len(), 1);
        assert_eq!(
            images.textures[&current].texture.as_ref().unwrap().id(),
            texture_id
        );
        assert!(images.queue.0.lock().unwrap().pending.is_empty());
        assert!(images.requested.is_empty());
        images.active.insert(current.clone());
        tx.send((pending, serde_json::json!({"ok":false}))).unwrap();
        images.end_frame(&ctx);
        assert_eq!(
            images.textures.len(),
            1,
            "late old-generation work must be ignored"
        );
        let changed = serde_json::json!({"id":1,"revision":"blob:new","kind":"managed_image"});
        images.prefetch(&ctx, &changed, 320);
        assert!(
            images
                .requested
                .contains(&(1, 320, "blob:new".to_owned(), new_epoch))
        );
        let different = serde_json::json!({"tab":"images","folder_path":"other","emoji_catalogue":false,"view_generation":3});
        images.set_context(&different);
        assert!(images.textures.is_empty());
        assert!(images.requested.is_empty());
        assert!(images.queue.0.lock().unwrap().pending.is_empty());
    }

    #[test]
    fn checked_style_generation_keeps_images_and_work_but_revision_and_context_still_invalidate() {
        let queue = Arc::new((
            Mutex::new(Queue {
                pending: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = mpsc::channel();
        let mut images = Images {
            queue,
            rx,
            active: HashSet::new(),
            requested: HashSet::new(),
            textures: HashMap::new(),
            clock: 0,
            preview_geometry: PreviewGeometryCache::default(),
            context: String::new(),
            context_epoch: 0,
        };
        let ctx = egui::Context::default();
        let mut before = serde_json::json!({"tab":"images","folder_path":null,"emoji_catalogue":false,"view_generation":10,"cursor":1});
        images.set_context(&before);
        let epoch = images.context_epoch;
        let original = (1, 320, "blob:old".to_owned(), epoch);
        let texture = ctx.load_texture(
            "synthetic-cache",
            egui::ColorImage::filled([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let texture_id = texture.id();
        images.textures.insert(
            original.clone(),
            Entry {
                texture: Some(texture),
                icon: false,
                visual_status: "image",
                touched: 0,
                retry_after: Instant::now(),
            },
        );
        let pending = (2, 320, "blob:pending".to_owned(), epoch);
        images.requested.insert(pending.clone());
        images.queue.0.lock().unwrap().pending.push_back((
            pending.clone(),
            None,
            false,
            ctx.clone(),
        ));
        let event = serde_json::json!({"id":1,"revision":"blob:old","kind":"managed_image"});
        for _ in 0..4 {
            let mut after = before.clone();
            after["view_generation"] =
                serde_json::json!(before["view_generation"].as_u64().unwrap() + 1);
            assert!(images.accept_metadata_generation(&before, &after));
            images.set_context(&after);
            images.prefetch(&ctx, &event, 320);
            assert_eq!(images.context_epoch, epoch);
            assert_eq!(
                images.textures[&original].texture.as_ref().unwrap().id(),
                texture_id
            );
            assert_eq!(images.queue.0.lock().unwrap().pending.len(), 1);
            assert!(images.requested.contains(&pending));
            before = after;
        }
        // Work queued before metadata-only changes is still valid in this context.
        images.active.insert(pending.clone());
        tx.send((pending.clone(), serde_json::json!({"ok":false})))
            .unwrap();
        images.end_frame(&ctx);
        assert!(images.textures.contains_key(&pending));
        let changed = serde_json::json!({"id":1,"revision":"blob:new","kind":"managed_image"});
        images.prefetch(&ctx, &changed, 320);
        assert!(
            images
                .requested
                .contains(&(1, 320, "blob:new".to_owned(), epoch))
        );
        assert_eq!(images.queue.0.lock().unwrap().pending.len(), 2);
        let mut other = before.clone();
        other["view_generation"] = serde_json::json!(15);
        other["tab"] = serde_json::json!("desktop");
        assert!(!images.accept_metadata_generation(&before, &other));
        images.set_context(&other);
        assert_ne!(images.context_epoch, epoch);
        assert!(images.textures.is_empty());
        assert!(images.queue.0.lock().unwrap().pending.is_empty());
        assert!(images.requested.is_empty());
    }

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
    fn managed_collection_cards_request_first_page_from_image_preview() {
        let event = serde_json::json!({"id": 4, "source":"image_collection", "path":"digest", "name":"album.bull", "revision":"blob:r"});
        let key = (4, 512, "blob:r".to_owned(), 0);
        assert!(is_managed_collection(&event));
        assert!(matches!(
            work_command(&key, work_path(&event, 4).unwrap(), true),
            Command::PreviewImage {
                id: 4,
                max_edge: 512
            }
        ));
        assert!(matches!(
            work_command(&key, work_path(&event, 4).unwrap(), false),
            Command::PreviewFile {
                id: 4,
                max_edge: 512
            }
        ));
    }

    #[test]
    fn folder_preview_carries_path_revision_and_size() {
        let event = serde_json::json!({"id": -1, "source": "work_folder", "path": "folder/image.png", "revision": "file:abc"});
        let path = work_path(&event, -1).unwrap();
        let key = (-1, 320, "file:abc".to_owned(), 0);
        assert!(
            matches!(work_command(&key, path, false), Command::PreviewPath { path, revision, max_edge: 320, image_only: false } if path == std::path::Path::new("folder/image.png") && revision == "file:abc")
        );
        assert!(work_path(&serde_json::json!({}), -1).is_none());
    }

    #[test]
    fn late_image_reply_from_old_context_is_discarded() {
        let queue = Arc::new((
            Mutex::new(Queue {
                pending: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = mpsc::channel();
        let mut images = Images {
            queue,
            rx,
            active: HashSet::new(),
            requested: HashSet::new(),
            textures: HashMap::new(),
            clock: 0,
            preview_geometry: PreviewGeometryCache::default(),
            context: String::new(),
            context_epoch: 0,
        };
        let a = serde_json::json!({"tab":"folder", "folder_path":"a", "view_generation":1});
        let b = serde_json::json!({"tab":"folder", "folder_path":"b", "view_generation":2});
        images.set_context(&a);
        let old = (-1, 320, "revision".to_owned(), images.context_epoch);
        images.set_context(&b);
        images.set_context(&a);
        let current = (-1, 320, "revision".to_owned(), images.context_epoch);
        images.active.insert(current.clone());
        tx.send((old, serde_json::json!({"ok":false}))).unwrap();
        images.end_frame(&egui::Context::default());
        assert!(!images.textures.contains_key(&current));
        tx.send((current.clone(), serde_json::json!({"ok":false})))
            .unwrap();
        images.end_frame(&egui::Context::default());
        assert_eq!(images.textures[&current].visual_status, "unavailable");
    }
    #[test]
    fn desktop_uses_id_route_folder_uses_snapshot_and_context_rejects_late_reply() {
        let desktop = serde_json::json!({"id": -(1_i64 << 62) - 1, "source":"desktop", "path":"managed/extensionless", "revision":"file:r"});
        let id = desktop["id"].as_i64().unwrap();
        assert!(
            matches!(work_command(&(id, 320, "file:r".into(), 0), work_path(&desktop, id).unwrap(), false), Command::PreviewFile { id: actual, .. } if actual == id)
        );
        assert!(
            work_path(
                &serde_json::json!({"id": -1, "source":"emoji_catalogue", "path":""}),
                -1
            )
            .is_none()
        );
        let root = tempfile::tempdir().unwrap();
        let mut images = Images::new(root.path().into());
        let a = serde_json::json!({"tab":"folder", "folder_path":"A", "view_generation":1});
        let b = serde_json::json!({"tab":"folder", "folder_path":"B", "view_generation":2});
        images.set_context(&a);
        let epoch_a = images.context_epoch;
        images.set_context(&b);
        images.set_context(&a);
        assert_ne!(images.context_epoch, epoch_a);
        let old = (-1, 320, "r".to_owned(), epoch_a);
        let now = (-1, 320, "r".to_owned(), images.context_epoch);
        assert_ne!(old, now);
        assert_eq!(images.visual_status(&desktop, 320), "loading");
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
        let key = (1, 1400, "synthetic".to_owned(), 0);
        let mut cache = PreviewGeometryCache::default();
        assert_eq!(cache.resolve(key.clone(), canvas, Some(image)), Some(image));
        // A background job temporarily owns the dispatcher: no new rectangle.
        for _ in 0..3 {
            assert_eq!(cache.resolve(key.clone(), canvas, None), Some(image));
        }
        assert_ne!(image, canvas);
        assert_eq!(
            cache.resolve((2, 1400, "other".into(), 0), canvas, None),
            None
        );
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
            context: String::new(),
            context_epoch: 0,
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

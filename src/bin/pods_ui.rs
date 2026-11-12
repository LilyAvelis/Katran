//! Bounded background color rasterization for Pods cards; never accesses clipboard.
use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, SyncSender};

type Key = (String, u32, String, u64);
type Job = (Key, Option<PathBuf>, egui::Context);
type RenderResult = Result<katran::emoji_render::Rendered, String>;
struct Entry {
    texture: egui::TextureHandle,
    size: Vec2,
    touched: u64,
}
#[derive(Default)]
pub(super) struct State {
    sender: Option<SyncSender<Job>>,
    receiver: Option<Receiver<(Key, RenderResult)>>,
    pending: HashSet<Key>,
    cache: HashMap<Key, Entry>,
    failed: HashMap<Key, Instant>,
    clock: u64,
    font_id: Option<String>,
    font_path: Option<PathBuf>,
    font_error: Option<String>,
    font_generation: u64,
    description_key: Option<(i64, String)>,
    description: Option<Value>,
}
impl State {
    pub(super) fn font_selection(&self) -> Option<&str> {
        self.font_id.as_deref()
    }
    pub(super) fn configure_font(
        &mut self,
        id: String,
        path: Option<PathBuf>,
        error: Option<String>,
    ) {
        if self.font_id.as_ref() == Some(&id) && self.font_path == path && self.font_error == error
        {
            return;
        }
        self.font_id = Some(id);
        self.font_path = path;
        self.font_error = error;
        self.font_generation = self.font_generation.wrapping_add(1);
        self.cache.clear();
        self.pending.clear();
        self.failed.clear();
    }
    pub(super) fn description_needed(&self, id: i64, revision: &str) -> bool {
        self.description_key
            .as_ref()
            .is_none_or(|(old_id, old_revision)| *old_id != id || old_revision != revision)
    }
    pub(super) fn set_description(&mut self, id: i64, revision: &str, value: Value) {
        self.description_key = Some((id, revision.to_owned()));
        self.description = Some(value);
    }
    pub(super) fn description(&self) -> Option<&Value> {
        self.description.as_ref()
    }
    fn start(&mut self) {
        if self.sender.is_some() {
            return;
        }
        let (sender, jobs) = mpsc::sync_channel::<Job>(32);
        let (results, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((key, font_path, ctx)) = jobs.recv() {
                let result = std::panic::catch_unwind(|| {
                    katran::emoji_render::render_with_font(
                        &key.0,
                        key.1 as f32,
                        font_path.as_deref(),
                    )
                })
                .unwrap_or_else(|_| Err("Color renderer panicked".into()));
                if results.send((key, result)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        self.sender = Some(sender);
        self.receiver = Some(receiver);
    }
    pub(super) fn paint(&mut self, ui: &egui::Ui, event: &Value, rect: egui::Rect) {
        let text = event["text_preview"].as_str().unwrap_or("");
        let ppp = ui.ctx().pixels_per_point();
        let size = (rect.height() * 0.55 * ppp).clamp(12.0, 128.0).round() as u32;
        let key = (
            text.to_owned(),
            size,
            self.font_id.clone().unwrap_or_else(|| "system".into()),
            self.font_generation,
        );
        self.start();
        while let Ok((key, result)) = self.receiver.as_ref().unwrap().try_recv() {
            if key.3 != self.font_generation {
                continue;
            }
            self.pending.remove(&key);
            match result {
                Ok(image) => {
                    self.failed.remove(&key);
                    let texture = ui.ctx().load_texture(
                        format!("pod:{}:{}:{}:{}", key.0, key.1, key.2, key.3),
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width as usize, image.height as usize],
                            &image.rgba,
                        ),
                        egui::TextureOptions::LINEAR,
                    );
                    self.cache.insert(
                        key,
                        Entry {
                            texture,
                            size: egui::vec2(image.width as f32, image.height as f32),
                            touched: self.clock,
                        },
                    );
                }
                Err(_) => {
                    if self.failed.len() >= 64 {
                        self.failed.clear();
                    }
                    self.failed
                        .insert(key, Instant::now() + Duration::from_secs(3));
                }
            }
        }
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.cache.get_mut(&key) {
            entry.touched = self.clock;
            let natural = entry.size / ppp;
            let scale = (rect.width() / natural.x)
                .min(rect.height() / natural.y)
                .min(1.0);
            let draw = egui::Rect::from_center_size(rect.center(), natural * scale);
            ui.painter()
                .with_clip_rect(ui.clip_rect().intersect(rect))
                .image(
                    entry.texture.id(),
                    draw,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
        } else {
            let failed = self
                .failed
                .get(&key)
                .is_some_and(|until| Instant::now() < *until);
            if failed {
                let mut font_size = (size as f32 / ppp).min(rect.height() * 0.55);
                let mut galley = ui.painter().layout_no_wrap(
                    text.to_owned(),
                    egui::FontId::proportional(font_size),
                    Color32::WHITE,
                );
                let fit = (rect.width() / galley.size().x.max(1.0))
                    .min(rect.height() / galley.size().y.max(1.0))
                    .min(1.0);
                if fit < 1.0 {
                    font_size *= fit;
                    galley = ui.painter().layout_no_wrap(
                        text.to_owned(),
                        egui::FontId::proportional(font_size),
                        Color32::WHITE,
                    );
                }
                ui.painter()
                    .with_clip_rect(ui.clip_rect().intersect(rect))
                    .galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
                ui.ctx().request_repaint_after(Duration::from_secs(3));
            }
            if !text.is_empty()
                && !failed
                && !self.pending.contains(&key)
                && self.pending.len() < 64
                && self
                    .sender
                    .as_ref()
                    .unwrap()
                    .try_send((key.clone(), self.font_path.clone(), ui.ctx().clone()))
                    .is_ok()
            {
                self.pending.insert(key);
            }
        }
        while self.cache.len() > 64 {
            if let Some(old) = self
                .cache
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(key, _)| key.clone())
            {
                self.cache.remove(&old);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_raster_does_not_paint_monochrome_text() {
        let (sender, _jobs) = mpsc::sync_channel(1);
        let (_results, receiver) = mpsc::channel();
        let mut state = State {
            sender: Some(sender),
            receiver: Some(receiver),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.paint(
                ui,
                &serde_json::json!({"text_preview":"🐄"}),
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(80.0, 80.0)),
            );
        });
        let painted_text = output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, egui::Shape::Text(_)));
        output.textures_delta.clear();
        assert!(!painted_text);
        assert_eq!(state.pending.len(), 1);
    }
    #[test]
    fn transient_failure_expires_and_requeues() {
        let (sender, jobs) = mpsc::sync_channel(4);
        let (results, receiver) = mpsc::channel();
        let mut state = State {
            sender: Some(sender),
            receiver: Some(receiver),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(80.0, 80.0));
        let event = serde_json::json!({"text_preview":"🤡⚰️💀🐄🦭"});
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.paint(ui, &event, rect)
        });
        out.textures_delta.clear();
        let (key, _, _) = jobs.try_recv().unwrap();
        results
            .send((key.clone(), Err("temporary".into())))
            .unwrap();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.paint(ui, &event, rect)
        });
        out.textures_delta.clear();
        assert!(!state.pending.contains(&key));
        state
            .failed
            .insert(key.clone(), Instant::now() - Duration::from_secs(1));
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.paint(ui, &event, rect)
        });
        out.textures_delta.clear();
        assert!(state.pending.contains(&key));
        assert_eq!(jobs.try_recv().unwrap().0, key);
    }
    #[test]
    fn switching_font_discards_inflight_results_and_requeues() {
        let (sender, jobs) = mpsc::sync_channel(4);
        let (results, receiver) = mpsc::channel();
        let mut state = State {
            sender: Some(sender),
            receiver: Some(receiver),
            ..Default::default()
        };
        state.configure_font("apple".into(), Some(PathBuf::from("first.ttf")), None);
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(80.0, 80.0));
        let event = serde_json::json!({"text_preview":"🐄"});
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.paint(ui, &event, rect)
        });
        out.textures_delta.clear();
        let (old, old_path, _) = jobs.try_recv().unwrap();
        assert_eq!(old_path, Some(PathBuf::from("first.ttf")));
        state.configure_font("system".into(), None, None);
        assert!(state.pending.is_empty());
        results.send((old, Err("stale".into()))).unwrap();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.paint(ui, &event, rect)
        });
        out.textures_delta.clear();
        assert!(state.failed.is_empty());
        let (new, new_path, _) = jobs.try_recv().unwrap();
        assert_eq!(new.2, "system");
        assert_eq!(new_path, None);
        assert!(state.pending.contains(&new));
    }
    #[test]
    fn inactive_state_starts_no_worker_and_work_is_bounded() {
        let mut state = State::default();
        assert!(state.sender.is_none());
        assert!(state.cache.is_empty());
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for n in 0..100 {
                let event = serde_json::json!({"text_preview":format!("🐄{}", " ".repeat(n))});
                state.paint(
                    ui,
                    &event,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(80.0, 80.0)),
                );
            }
        });
        output.textures_delta.clear();
        assert!(state.pending.len() <= 64);
        assert!(state.cache.len() <= 64);
    }
}

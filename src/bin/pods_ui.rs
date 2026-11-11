//! Bounded background color rasterization for Pods cards; never accesses clipboard.
use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, SyncSender};

pub(super) const GRID_GAP: f32 = 12.0;
/// The same column count is used by painting and keyboard navigation.
pub(super) fn columns(width: f32, tile_size: u16) -> usize {
    ((width.max(0.0) + GRID_GAP) / (f32::from(tile_size.clamp(48, 192)) + GRID_GAP))
        .floor()
        .max(1.0) as usize
}

type Key = (String, u32);
type RenderResult = Result<katran::emoji_render::Rendered, String>;
struct Entry {
    texture: egui::TextureHandle,
    size: Vec2,
    touched: u64,
}
#[derive(Default)]
pub(super) struct State {
    sender: Option<SyncSender<(Key, egui::Context)>>,
    receiver: Option<Receiver<(Key, RenderResult)>>,
    pending: HashSet<Key>,
    cache: HashMap<Key, Entry>,
    failed: HashSet<Key>,
    clock: u64,
    description_key: Option<(i64, String)>,
    description: Option<Value>,
}
impl State {
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
        let (sender, jobs) = mpsc::sync_channel::<(Key, egui::Context)>(32);
        let (results, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((key, ctx)) = jobs.recv() {
                let result = katran::emoji_render::render(&key.0, key.1 as f32);
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
        let key = (text.to_owned(), size);
        self.start();
        while let Ok((key, result)) = self.receiver.as_ref().unwrap().try_recv() {
            self.pending.remove(&key);
            match result {
                Ok(image) => {
                    let texture = ui.ctx().load_texture(
                        format!("pod:{}:{}", key.0, key.1),
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
                    self.failed.insert(key);
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
            if self.failed.contains(&key) {
                ui.painter()
                    .with_clip_rect(ui.clip_rect().intersect(rect))
                    .text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional((size as f32 / ppp).min(rect.height() * 0.55)),
                        Color32::WHITE,
                    );
            }
            if !text.is_empty()
                && !self.failed.contains(&key)
                && !self.pending.contains(&key)
                && self.pending.len() < 64
                && self
                    .sender
                    .as_ref()
                    .unwrap()
                    .try_send((key.clone(), ui.ctx().clone()))
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
    fn compact_grid_uses_requested_size_and_never_zero_columns() {
        assert_eq!(columns(0.0, 96), 1);
        assert_eq!(columns(960.0, 96), 9);
        assert!(columns(960.0, 48) > columns(960.0, 96));
        assert!(columns(960.0, 192) < columns(960.0, 96));
    }
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

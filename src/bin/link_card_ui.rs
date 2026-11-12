//! Compact Links tiles. A worker calls the dispatcher for site identity and
//! optional favicons; painting never performs network or reads link storage.
use eframe::egui::{self, Color32, Pos2, Rect};
use katran::{Command, Request, Service};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, mpsc},
};

struct Queue {
    pending: VecDeque<(String, i64, egui::Context)>,
    closed: bool,
}
struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: mpsc::Receiver<(String, Value)>,
}
struct Visual {
    site: String,
    mark: String,
    color: Color32,
    icon: Option<egui::TextureHandle>,
}
pub(super) struct State {
    root: PathBuf,
    online: bool,
    worker: Option<Worker>,
    queued: HashSet<String>,
    visuals: HashMap<String, Visual>,
}
impl State {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            online: false,
            worker: None,
            queued: HashSet::new(),
            visuals: HashMap::new(),
        }
    }
    pub fn set_online(&mut self, online: bool) {
        if self.online != online {
            self.stop();
            self.online = online;
        }
    }
    fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let (lock, wake) = &*worker.queue;
            if let Ok(mut queue) = lock.lock() {
                queue.closed = true;
                queue.pending.clear();
                wake.notify_one();
            }
        }
        self.queued.clear();
        self.visuals.clear();
    }
    fn start(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let root = self.root.clone();
        let queue = Arc::new((
            Mutex::new(Queue {
                pending: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = mpsc::channel();
        let worker = queue.clone();
        std::thread::spawn(move || {
            let mut api = Service::open_read_only(&root);
            loop {
                let (key, id, ctx) = {
                    let (lock, wake) = &*worker;
                    let mut state = lock.lock().unwrap();
                    while state.pending.is_empty() && !state.closed {
                        state = wake.wait(state).unwrap();
                    }
                    if state.closed {
                        return;
                    }
                    state.pending.pop_front().unwrap()
                };
                let result = match &mut api {
                    Ok(api) => api.execute(Request {
                        api_version: 1,
                        request_id: "link-visual".into(),
                        command: Command::GetLinkVisual { id },
                    }),
                    Err(error) => serde_json::json!({"ok":false,"error":error}),
                };
                if tx.send((key, result)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        self.worker = Some(Worker { queue, rx });
    }
    fn drain(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        while let Ok((key, result)) = worker.rx.try_recv() {
            self.queued.remove(&key);
            let data = &result["result"];
            let host = data["host"].as_str().unwrap_or("");
            if result["ok"] != true || host != key {
                continue;
            }
            let color = data["color"]
                .as_array()
                .filter(|channels| channels.len() == 3)
                .map(|channels| {
                    Color32::from_rgb(
                        channels[0].as_u64().unwrap_or(80) as u8,
                        channels[1].as_u64().unwrap_or(90) as u8,
                        channels[2].as_u64().unwrap_or(100) as u8,
                    )
                })
                .unwrap_or(Color32::from_rgb(55, 80, 100));
            let icon = data["favicon_base64"]
                .as_str()
                .and_then(|encoded| {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .ok()
                })
                .and_then(|bytes| image::load_from_memory(&bytes).ok())
                .map(|image| {
                    let rgba = image.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    ctx.load_texture(
                        format!("link-favicon-{host}"),
                        egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                        egui::TextureOptions::LINEAR,
                    )
                });
            self.visuals.insert(
                key,
                Visual {
                    site: data["site"].as_str().unwrap_or(host).to_owned(),
                    mark: data["mark"].as_str().unwrap_or("↗").to_owned(),
                    color,
                    icon,
                },
            );
        }
    }
    fn key(event: &Value) -> Option<String> {
        let url = event["text_preview"].as_str()?;
        url::Url::parse(url)
            .ok()?
            .host_str()
            .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
    }
    pub fn paint(&mut self, ui: &egui::Ui, event: &Value, rect: Rect) {
        self.drain(ui.ctx());
        let key = Self::key(event);
        if let (Some(key), Some(id)) = (key.as_ref(), event["id"].as_i64())
            && !self.queued.contains(key)
            && !self.visuals.contains_key(key)
        {
            self.start();
            if let Some(worker) = &self.worker {
                let (lock, wake) = &*worker.queue;
                if let Ok(mut queue) = lock.lock()
                    && queue.pending.len() < 64
                {
                    queue.pending.push_back((key.clone(), id, ui.ctx().clone()));
                    self.queued.insert(key.clone());
                    wake.notify_one();
                }
            }
        }
        let visual = key.as_ref().and_then(|key| self.visuals.get(key));
        let icon_rect = Rect::from_center_size(
            rect.center() - egui::vec2(0.0, 7.0),
            egui::vec2(rect.width() * 0.54, rect.height() * 0.54),
        );
        let painter = ui.painter().with_clip_rect(rect);
        if let Some(icon) = visual.and_then(|visual| visual.icon.as_ref()) {
            painter.image(
                icon.id(),
                icon_rect,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            let color = visual.map_or(Color32::from_rgb(55, 80, 100), |v| v.color);
            painter.circle_filled(icon_rect.center(), icon_rect.width() / 2.0, color);
            painter.text(
                icon_rect.center(),
                egui::Align2::CENTER_CENTER,
                visual.map_or("↗", |v| v.mark.as_str()),
                egui::FontId::proportional((icon_rect.width() * 0.39).max(12.0)),
                Color32::WHITE,
            );
        }
        let site = visual.map_or("Ссылка", |v| v.site.as_str());
        let mut job = egui::text::LayoutJob::simple_singleline(
            site.to_owned(),
            egui::FontId::proportional(11.0),
            Color32::from_gray(230),
        );
        job.wrap.max_width = (rect.width() - 8.0).max(1.0);
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        painter.galley(
            egui::pos2(
                rect.center().x - galley.size().x / 2.0,
                rect.bottom() - galley.size().y - 5.0,
            ),
            galley,
            Color32::from_gray(230),
        );
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop();
    }
}

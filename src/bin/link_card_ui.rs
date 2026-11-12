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
    pending: VecDeque<(i64, u64, bool, egui::Context)>,
    closed: bool,
}
struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: mpsc::Receiver<(i64, u64, bool, Value)>,
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
    queued: HashSet<i64>,
    attempted: HashSet<i64>,
    visuals: HashMap<i64, Visual>,
    epoch: u64,
    refresh: Option<(usize, usize, usize)>,
}
impl State {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            online: false,
            worker: None,
            queued: HashSet::new(),
            attempted: HashSet::new(),
            visuals: HashMap::new(),
            epoch: 0,
            refresh: None,
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
        self.epoch = self.epoch.wrapping_add(1);
        self.refresh = None;
        self.queued.clear();
        self.attempted.clear();
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
            let mut host_cache: HashMap<String, Value> = HashMap::new();
            let mut cache_epoch = 0;
            loop {
                let (id, epoch, refresh, ctx) = {
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
                if epoch != cache_epoch {
                    host_cache.clear();
                    cache_epoch = epoch;
                }
                let result = match &mut api {
                    Ok(api) => {
                        let link = api.execute(Request {
                            api_version: 1,
                            request_id: "link-visual-url".into(),
                            command: Command::GetLink { id },
                        });
                        let host = link["result"]["url"]
                            .as_str()
                            .and_then(|url| url::Url::parse(url).ok())
                            .and_then(|url| url.host_str().map(str::to_owned))
                            .map(|host| host.trim_end_matches('.').to_ascii_lowercase());
                        if let Some(cached) = host.as_ref().and_then(|host| host_cache.get(host)) {
                            let mut result = cached.clone();
                            result["result"]["id"] = Value::from(id);
                            result
                        } else {
                            let result = api.execute(Request {
                                api_version: 1,
                                request_id: "link-visual".into(),
                                command: Command::GetLinkVisual { id },
                            });
                            if result["ok"] == true
                                && let Some(host) = host
                            {
                                host_cache.insert(host, result.clone());
                            }
                            result
                        }
                    }
                    Err(error) => serde_json::json!({"ok":false,"error":error}),
                };
                if tx.send((id, epoch, refresh, result)).is_err() {
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
        while let Ok((id, epoch, refresh, result)) = worker.rx.try_recv() {
            if epoch != self.epoch {
                continue;
            }
            self.queued.remove(&id);
            self.attempted.insert(id);
            if refresh && let Some((done, _, failed)) = &mut self.refresh {
                *done += 1;
                if result["ok"] != true || result["result"]["favicon_base64"].is_null() {
                    *failed += 1;
                }
            }
            let data = &result["result"];
            let host = data["host"].as_str().unwrap_or("");
            if result["ok"] != true || data["id"].as_i64() != Some(id) {
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
                id,
                Visual {
                    site: data["site"].as_str().unwrap_or(host).to_owned(),
                    mark: data["mark"].as_str().unwrap_or("↗").to_owned(),
                    color,
                    icon,
                },
            );
        }
    }
    /// Explicitly retry supplied link IDs. Results from an older queue are ignored.
    pub fn refresh_all(&mut self, ids: Vec<i64>, ctx: &egui::Context) {
        self.start();
        self.epoch = self.epoch.wrapping_add(1);
        self.queued.clear();
        let mut unique = HashSet::new();
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|id| *id > 0 && unique.insert(*id))
            .collect();
        self.refresh = Some((0, ids.len(), 0));
        if let Some(worker) = &self.worker {
            let (lock, wake) = &*worker.queue;
            if let Ok(mut queue) = lock.lock() {
                queue.pending.clear();
                for id in ids {
                    queue.pending.push_back((id, self.epoch, true, ctx.clone()));
                    self.queued.insert(id);
                }
                wake.notify_one();
            }
        }
        ctx.request_repaint();
    }

    /// Completed, total, and failures (including unavailable icons).
    pub fn progress(&mut self, ctx: &egui::Context) -> Option<(usize, usize, usize)> {
        self.drain(ctx);
        self.refresh
    }

    pub fn paint(&mut self, ui: &egui::Ui, event: &Value, rect: Rect) {
        self.drain(ui.ctx());
        // The catalogue preview may be truncated or unavailable. The dispatcher
        // resolves the full stored URL from the stable event ID.
        let id = event["id"].as_i64();
        if let Some(id) = id
            && !self.queued.contains(&id)
            && !self.attempted.contains(&id)
            && !self.visuals.contains_key(&id)
        {
            self.start();
            if let Some(worker) = &self.worker {
                let (lock, wake) = &*worker.queue;
                if let Ok(mut queue) = lock.lock()
                    && queue.pending.len() < 64
                {
                    queue
                        .pending
                        .push_back((id, self.epoch, false, ui.ctx().clone()));
                    self.queued.insert(id);
                    wake.notify_one();
                }
            }
        }
        let visual = id.and_then(|id| self.visuals.get(&id));
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
        let available = (rect.width() - 8.0).max(1.0);
        let galley = ui.fonts_mut(|fonts| {
            for size in [12.0, 11.0, 10.0, 9.0] {
                let galley = fonts.layout_no_wrap(
                    site.to_owned(),
                    egui::FontId::proportional(size),
                    Color32::from_gray(230),
                );
                if galley.size().x <= available {
                    return galley;
                }
            }
            let mut job = egui::text::LayoutJob::simple_singleline(
                site.to_owned(),
                egui::FontId::proportional(9.0),
                Color32::from_gray(230),
            );
            job.wrap.max_width = available;
            job.wrap.max_rows = 1;
            job.wrap.overflow_character = Some('…');
            fonts.layout_job(job)
        });
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

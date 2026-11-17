//! Cursor-only details. Reads use a latest-only, bounded read-only worker.
use super::{Command, PathBuf, Request, Service, Value, egui, i18n::tr};
use std::sync::{Arc, Condvar, Mutex, mpsc};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    id: i64,
    revision: String,
    context: String,
}
fn key(event: &Value, state: &Value) -> Option<Key> {
    Some(Key {
        id: event["id"].as_i64()?,
        revision: event["revision"].as_str()?.into(),
        context: serde_json::json!([
            state["tab"],
            state["emoji_catalogue"],
            state["view_generation"],
            state["folder_path"],
            event["path"]
        ])
        .to_string(),
    })
}
#[derive(Default)]
struct Queue {
    next: Option<(Key, egui::Context)>,
    closed: bool,
}
struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: mpsc::Receiver<(Key, Value)>,
}
pub(super) struct DetailsMetadata {
    root: PathBuf,
    worker: Option<Worker>,
    wanted: Option<Key>,
    description: Option<Value>,
}
impl DetailsMetadata {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            worker: None,
            wanted: None,
            description: None,
        }
    }
    pub fn reset(&mut self) {
        if let Some(worker) = self.worker.take() {
            let (lock, wake) = &*worker.queue;
            if let Ok(mut queue) = lock.lock() {
                queue.closed = true;
                queue.next = None;
                wake.notify_one();
            }
        }
        self.wanted = None;
        self.description = None;
    }
    fn start(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let root = self.root.clone();
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let shared = queue.clone();
        let (tx, rx) = mpsc::sync_channel(2);
        std::thread::spawn(move || {
            let mut api = Service::open_read_only(&root);
            loop {
                let request = {
                    let (lock, wake) = &*shared;
                    let mut queue = lock.lock().unwrap();
                    while queue.next.is_none() && !queue.closed {
                        queue = wake.wait(queue).unwrap();
                    }
                    if queue.closed {
                        return;
                    }
                    queue.next.take().unwrap()
                };
                let (key, ctx) = request;
                let result = match &mut api {
                    Ok(api) => api.execute(Request {
                        api_version: 1,
                        request_id: "details-pod".into(),
                        command: Command::DescribePod { id: key.id },
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
    fn receive(&mut self, key: Key, reply: Value) {
        if self.wanted.as_ref() != Some(&key) {
            return;
        }
        self.description = Some(
            if reply["ok"] == true && reply["result"]["revision"] == key.revision {
                reply
            } else {
                serde_json::json!({"ok":false,"error":{"message":reply["error"]["message"].as_str().unwrap_or(tr("Объект изменился или недоступен", "Item changed or is unavailable"))}})
            },
        );
    }
    pub fn show(&mut self, ui: &mut egui::Ui, event: &Value, state: &Value) {
        ui.heading(tr("Информация", "Information"));
        for (label, value) in fields(event) {
            field(ui, &label, &value);
        }
        if state["tab"] != "pods" || event["kind"] != "text_fragment" {
            self.reset();
            return;
        }
        let Some(next) = key(event, state) else {
            return;
        };
        self.start();
        if self.wanted.as_ref() != Some(&next) {
            self.wanted = Some(next.clone());
            self.description = None;
            let (lock, wake) = &*self.worker.as_ref().unwrap().queue;
            let mut queue = lock.lock().unwrap();
            queue.next = Some((next, ui.ctx().clone()));
            wake.notify_one();
        }
        while let Ok((key, reply)) = self.worker.as_ref().unwrap().rx.try_recv() {
            self.receive(key, reply);
        }
        ui.separator();
        match &self.description {
            Some(reply) if reply["ok"] == true => {
                let data = &reply["result"];
                if let Some(name) = data["name"].as_str() {
                    field(ui, "Unicode", name);
                }
                for (key, label) in [
                    ("utf8_bytes", "UTF-8"),
                    ("utf16_units", "UTF-16"),
                    ("codepoint_count", tr("Символы", "Codepoints")),
                ] {
                    if let Some(value) = data[key].as_u64() {
                        field(ui, label, &value.to_string());
                    }
                }
                if let Some(points) = data["codepoints"].as_array() {
                    for point in points.iter().take(256) {
                        field(
                            ui,
                            point["codepoint"].as_str().unwrap_or(""),
                            point["name"].as_str().unwrap_or(""),
                        );
                    }
                }
                if data["metadata_truncated"] == true {
                    ui.weak(tr("Метаданные сокращены", "Metadata truncated"));
                }
            }
            Some(reply) => {
                ui.weak(
                    reply["error"]["message"]
                        .as_str()
                        .unwrap_or(tr("Метаданные недоступны", "Metadata unavailable")),
                );
            }
            None => {
                ui.weak(tr("Загрузка метаданных…", "Loading metadata…"));
            }
        }
    }
}
impl Drop for DetailsMetadata {
    fn drop(&mut self) {
        self.reset();
    }
}
fn field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.weak(label);
    // UI content and tooltips remain bounded even for malformed external metadata.
    let text: String = value.chars().take(4096).collect();
    ui.add(egui::Label::new(text).selectable(true).wrap());
    ui.add_space(3.0);
}
fn fields(event: &Value) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut add = |label: &str, value: Option<&str>| {
        if let Some(value) = value.filter(|text| !text.is_empty()) {
            result.push((label.to_owned(), value.to_owned()));
        }
    };
    add(
        tr("Имя", "Name"),
        event["properties"]["display_name"]
            .as_str()
            .or(event["name"].as_str()),
    );
    let kind = match event["kind"].as_str() {
        Some("managed_image") => tr("Изображение", "Image"),
        Some("text_fragment") => tr("Текст", "Text"),
        Some("link_fragment") => tr("Ссылка", "Link"),
        Some("table_fragment") => tr("Таблица", "Table"),
        _ => tr("Файл или папка", "File or folder"),
    };
    add(tr("Тип", "Type"), Some(kind));
    add(tr("Путь", "Path"), event["path"].as_str());
    add(
        tr("Приложение", "Application"),
        event["origin"]["source_app"].as_str(),
    );
    add(
        tr("Домен", "Domain"),
        event["origin"]["source_domain"].as_str(),
    );
    add(tr("Источник", "Source"), event["source"].as_str());
    if event["properties"]["pinned"] == true {
        result.push((tr("Закреплён", "Pinned").into(), tr("Да", "Yes").into()));
    }
    if let Some(tags) = event["tags"].as_array() {
        let names: Vec<_> = tags
            .iter()
            .filter_map(|tag| tag["display"].as_str())
            .take(32)
            .collect();
        if !names.is_empty() {
            result.push((tr("Теги", "Tags").into(), names.join(", ")));
        }
    }
    if let Some(size) = event["size"].as_u64().filter(|size| *size > 0) {
        result.push((tr("Размер", "Size").into(), super::sorting_ui::size(size)));
    }
    for (key, label) in [
        ("detected_at_ms", tr("Добавлен", "Added")),
        ("modified_at_ms", tr("Изменён", "Modified")),
    ] {
        if let Some(ms) = event[key].as_i64().filter(|ms| *ms > 0) {
            result.push((label.into(), super::sorting_ui::date(ms)));
        }
    }
    if event["available"] == false {
        result.push((
            tr("Состояние", "Status").into(),
            tr("Недоступен", "Unavailable").into(),
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn obsolete_metadata_cannot_replace_current_cursor_or_revision() {
        let mut state = DetailsMetadata::new(PathBuf::new());
        let event = json!({"id":1,"revision":"a","path":"a"});
        let first = key(&event, &json!({"tab":"pods","view_generation":1})).unwrap();
        let current = key(&event, &json!({"tab":"pods","view_generation":2})).unwrap();
        state.wanted = Some(current.clone());
        state.receive(
            first,
            json!({"ok":true,"result":{"revision":"a","name":"OLD"}}),
        );
        assert!(state.description.is_none());
        state.receive(
            current,
            json!({"ok":true,"result":{"revision":"changed","name":"OLD"}}),
        );
        assert_eq!(state.description.as_ref().unwrap()["ok"], false);
    }
    #[test]
    fn metadata_omits_missing_zero_timestamps_and_internal_identifiers() {
        let values = fields(
            &json!({"id":-123,"revision":"internal","size":0,"detected_at_ms":0,"modified_at_ms":null,"name":"Synthetic","kind":"link_fragment","path":"","origin":{}}),
        );
        assert_eq!(values.len(), 2);
        assert!(!format!("{values:?}").contains("internal"));
    }
}

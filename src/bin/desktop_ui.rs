//! Desktop backgrounds are a client of the managed asset API, not shelf events.
use super::i18n::tr;
use eframe::egui::{self, ColorImage, TextureHandle};
use katran::{Command, Request, Service};
use serde_json::Value;
use std::{collections::HashMap, sync::mpsc};

#[derive(Default)]
pub struct State {
    items: Vec<Value>,
    selected: Option<String>,
    pending_delete: Option<String>,
    entered: bool,
    error: String,
    notice: String,
    textures: HashMap<String, TextureHandle>,
    thumbnails: Option<mpsc::Receiver<(String, ColorImage)>>,
}

fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "desktop-ui".into(),
        command,
    })
}

impl State {
    pub fn leave(&mut self) {
        self.entered = false;
    }

    fn refresh(&mut self, api: &mut Service, ctx: &egui::Context) {
        let reply = call(api, Command::ListDesktopAssets);
        if reply["ok"] != true {
            self.error = reply["error"]["message"]
                .as_str()
                .unwrap_or(tr(
                    "Не удалось загрузить фоны",
                    "Could not load backgrounds",
                ))
                .to_owned();
            return;
        }
        self.error.clear();
        self.items = reply["result"]["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if self
            .selected
            .as_ref()
            .is_some_and(|id| !self.items.iter().any(|item| item["id"] == *id))
        {
            self.selected = None;
        }
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|id| !self.items.iter().any(|item| item["id"] == *id))
        {
            self.pending_delete = None;
        }
        self.textures
            .retain(|id, _| self.items.iter().any(|item| item["id"] == *id));
        let work: Vec<_> = self
            .items
            .iter()
            .filter(|item| item["kind"] == "image")
            .filter_map(|item| {
                let id = item["id"].as_str()?;
                if self.textures.contains_key(id) {
                    return None;
                }
                Some((
                    id.to_owned(),
                    std::path::PathBuf::from(item["path"].as_str()?),
                ))
            })
            .take(96)
            .collect();
        let (tx, rx) = mpsc::channel();
        self.thumbnails = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (id, path) in work {
                let decoded = (|| {
                    if std::fs::metadata(&path).ok()?.len() > 64 * 1024 * 1024 {
                        return None;
                    }
                    let mut reader = image::ImageReader::open(path)
                        .ok()?
                        .with_guessed_format()
                        .ok()?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(8192);
                    limits.max_image_height = Some(8192);
                    limits.max_alloc = Some(128 * 1024 * 1024);
                    reader.limits(limits);
                    let image = reader.decode().ok()?.thumbnail(320, 180).to_rgba8();
                    Some(ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ))
                })();
                if let Some(image) = decoded {
                    if tx.send((id, image)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            }
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, api: &mut Service) {
        if !self.entered {
            self.entered = true;
            self.refresh(api, ui.ctx());
        }
        if let Some(rx) = &self.thumbnails {
            while let Ok((id, image)) = rx.try_recv() {
                self.textures.insert(
                    id.clone(),
                    ui.ctx().load_texture(
                        format!("desktop-{id}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    ),
                );
            }
        }
        ui.horizontal(|ui| {
            ui.heading(tr("Рабочий стол", "Desktop"));
            if ui.button(tr("Обновить", "Refresh")).clicked() {
                self.refresh(api, ui.ctx());
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
        }
        if !self.notice.is_empty() {
            ui.label(&self.notice);
        }
        if self.items.is_empty() {
            ui.weak(tr("Сохранённых фонов пока нет", "No saved backgrounds yet"));
            return;
        }
        let gallery_height = (ui.available_height() - 55.0).max(80.0);
        egui::ScrollArea::vertical()
            .max_height(gallery_height)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for item in &self.items {
                        let Some(id) = item["id"].as_str() else {
                            continue;
                        };
                        let selected = self.selected.as_deref() == Some(id);
                        ui.group(|ui| {
                            ui.set_width(170.0);
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(170.0, 100.0),
                                egui::Sense::click(),
                            );
                            ui.painter()
                                .rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
                            if let Some(texture) = self.textures.get(id) {
                                let size = texture.size_vec2();
                                let scale = (rect.width() / size.x).min(rect.height() / size.y);
                                let target =
                                    egui::Rect::from_center_size(rect.center(), size * scale);
                                ui.painter().image(
                                    texture.id(),
                                    target,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            } else {
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    if item["kind"] == "video" {
                                        "▶ Video"
                                    } else {
                                        "Image"
                                    },
                                    egui::FontId::proportional(16.0),
                                    ui.visuals().text_color(),
                                );
                            }
                            if selected {
                                ui.painter().rect_stroke(
                                    rect,
                                    4.0,
                                    egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            let label = item["name"].as_str().unwrap_or(id);
                            if response.clicked() || ui.selectable_label(selected, label).clicked()
                            {
                                self.selected = Some(id.to_owned());
                                self.pending_delete = None;
                            }
                            if item["in_use"] == true {
                                ui.weak(tr("Используется", "In use"));
                            }
                        });
                    }
                });
            });
        let Some(id) = self.selected.clone() else {
            return;
        };
        let video = self
            .items
            .iter()
            .any(|item| item["id"] == id && item["kind"] == "video");
        let mut command = None;
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Фон Katran", "Katran background")).clicked() {
                command = Some(Command::ApplyDesktopAsset {
                    id: id.clone(),
                    target: "katran".into(),
                });
            }
            if !video && ui.button(tr("Обои Windows", "Windows wallpaper")).clicked() {
                command = Some(Command::ApplyDesktopAsset {
                    id: id.clone(),
                    target: "windows".into(),
                });
            }
            if ui.button(tr("Удалить", "Delete")).clicked() {
                self.pending_delete = Some(id.clone());
            }
        });
        if self.pending_delete.as_deref() == Some(id.as_str()) {
            ui.horizontal(|ui| {
                ui.label(tr("Удалить сохранённую копию?", "Delete the saved copy?"));
                if ui.button(tr("Да, удалить", "Yes, delete")).clicked() {
                    command = Some(Command::DeleteDesktopAsset { id: id.clone() });
                    self.pending_delete = None;
                }
                if ui.button(tr("Отмена", "Cancel")).clicked() {
                    self.pending_delete = None;
                }
            });
        }
        if let Some(command) = command {
            let windows_wallpaper = matches!(&command, Command::ApplyDesktopAsset { target, .. } if target == "windows");
            let reply = call(api, command);
            if reply["ok"] == true {
                self.notice = if windows_wallpaper {
                    tr(
                        "Windows приняла запрос на смену обоев",
                        "Windows accepted the wallpaper request",
                    )
                } else {
                    tr("Готово", "Done")
                }
                .into();
                self.error.clear();
                self.refresh(api, ui.ctx());
            } else {
                self.notice.clear();
                self.error = reply["error"]["message"]
                    .as_str()
                    .unwrap_or(tr("Ошибка", "Error"))
                    .to_owned();
            }
        }
    }
}

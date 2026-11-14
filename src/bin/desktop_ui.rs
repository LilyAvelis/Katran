//! Desktop backgrounds are a client of the managed asset API, not shelf events.
use super::i18n::tr;
use eframe::egui::{self, ColorImage, TextureHandle};
use katran::{Command, Request, Service};
use serde_json::Value;
use std::{collections::HashMap, sync::mpsc};

const CARD_GAP: f32 = 12.0;
const CARD_HEIGHT: f32 = 164.0;
const CARD_TARGET_WIDTH: f32 = 190.0;

fn grid_dimensions(width: f32) -> (usize, f32) {
    let width = width.max(1.0);
    let columns = (((width + CARD_GAP) / (CARD_TARGET_WIDTH + CARD_GAP)).floor() as usize).max(1);
    let card_width = ((width - CARD_GAP * (columns - 1) as f32) / columns as f32).min(220.0);
    (columns, card_width)
}

#[derive(Default)]
pub struct State {
    items: Vec<Value>,
    selected: Option<String>,
    pending_delete: Option<String>,
    entered: bool,
    list_job: Option<katran::domain_jobs::JobId>,
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
        if let Some(job_id) = self.list_job.take() {
            call(api, Command::CancelJob { job_id });
        }
        self.error.clear();
        let reply = call(
            api,
            Command::BeginJob {
                command: Box::new(Command::ListDesktopAssets),
            },
        );
        if reply["ok"] != true {
            self.list_error(&reply["error"]);
        } else if reply["result"]["state"] == "succeeded" {
            self.accept_list(&reply["result"]["result"], ctx);
        } else if let Ok(id) = serde_json::from_value(reply["result"]["job_id"].clone()) {
            self.list_job = Some(id);
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            self.list_error(&serde_json::Value::Null);
        }
    }

    fn list_error(&mut self, error: &Value) {
        self.error = error["message"]
            .as_str()
            .unwrap_or(tr(
                "Не удалось загрузить фоны",
                "Could not load backgrounds",
            ))
            .to_owned();
    }

    fn poll_list(&mut self, api: &mut Service, ctx: &egui::Context) {
        let Some(job_id) = self.list_job else {
            return;
        };
        let reply = call(api, Command::PollJob { job_id });
        if reply["ok"] != true {
            self.list_job = None;
            self.list_error(&reply["error"]);
            return;
        }
        match reply["result"]["state"].as_str() {
            Some("queued" | "running" | "ready") => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50))
            }
            Some("succeeded") => {
                self.list_job = None;
                self.accept_list(&reply["result"]["result"], ctx);
            }
            _ => {
                self.list_job = None;
                self.list_error(&reply["result"]["error"]);
            }
        }
    }

    fn accept_list(&mut self, result: &Value, ctx: &egui::Context) {
        self.items = result["items"].as_array().cloned().unwrap_or_default();
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
        self.poll_list(api, ui.ctx());
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
        if self.list_job.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr("Загрузка фонов…", "Loading backgrounds…"));
            });
            return;
        }
        if self.items.is_empty() {
            if self.error.is_empty() {
                ui.weak(tr("Сохранённых фонов пока нет", "No saved backgrounds yet"));
            }
            return;
        }
        // Keep a separate fixed-height action region, including the delete confirmation.
        let gallery_height = (ui.available_height() - 95.0).max(50.0);
        egui::ScrollArea::vertical()
            .max_height(gallery_height)
            .show(ui, |ui| {
                let (columns, card_width) = grid_dimensions(ui.available_width());
                for (row_index, row) in self.items.chunks(columns).enumerate() {
                    let (row_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), CARD_HEIGHT),
                        egui::Sense::hover(),
                    );
                    for (column, item) in row.iter().enumerate() {
                        let Some(id) = item["id"].as_str() else {
                            continue;
                        };
                        let card = egui::Rect::from_min_size(
                            row_rect.min + egui::vec2(column as f32 * (card_width + CARD_GAP), 0.0),
                            egui::vec2(card_width, CARD_HEIGHT),
                        );
                        let response = ui.interact(
                            card,
                            ui.make_persistent_id(("desktop-card", id)),
                            egui::Sense::click(),
                        );
                        let selected = self.selected.as_deref() == Some(id);
                        ui.painter()
                            .rect_filled(card, 5.0, ui.visuals().faint_bg_color);
                        if selected {
                            ui.painter().rect_stroke(
                                card,
                                5.0,
                                egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let inner = card.shrink(8.0);
                        let image_rect =
                            egui::Rect::from_min_size(inner.min, egui::vec2(inner.width(), 100.0));
                        if let Some(texture) = self.textures.get(id) {
                            let target =
                                super::gallery_card::fit_rect(image_rect, texture.size_vec2());
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
                                image_rect.center(),
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
                        let name_rect = egui::Rect::from_min_size(
                            egui::pos2(inner.left(), image_rect.bottom() + 6.0),
                            egui::vec2(inner.width(), 20.0),
                        );
                        super::gallery_card::text(
                            ui,
                            name_rect,
                            item["name"].as_str().unwrap_or(id),
                            13.0,
                            ui.visuals().text_color(),
                        );
                        if item["in_use"] == true {
                            let badge = egui::Rect::from_min_size(
                                egui::pos2(inner.left(), name_rect.bottom() + 2.0),
                                egui::vec2(inner.width(), 18.0),
                            );
                            super::gallery_card::text(
                                ui,
                                badge,
                                tr("Используется", "In use"),
                                11.0,
                                ui.visuals().weak_text_color(),
                            );
                        }
                        if response.clicked() {
                            self.selected = Some(id.to_owned());
                            self.pending_delete = None;
                        }
                    }
                    if row_index + 1 < self.items.len().div_ceil(columns) {
                        ui.add_space(CARD_GAP);
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_grid_fits_narrow_and_wide_windows_without_overlap() {
        for width in [80.0_f32, 190.0, 400.0, 800.0, 1920.0] {
            let (columns, card_width) = grid_dimensions(width);
            assert!(columns >= 1 && card_width > 0.0 && card_width <= 220.0);
            let occupied = columns as f32 * card_width + (columns - 1) as f32 * CARD_GAP;
            assert!(
                occupied <= width + 0.01,
                "{columns} cards exceed width {width}"
            );
        }
    }
}

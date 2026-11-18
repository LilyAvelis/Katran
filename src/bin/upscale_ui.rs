//! Upscale jobs are bound by the dispatcher to the current selected image.
use super::i18n::tr;
use super::*;
use base64::Engine;
use std::time::Duration;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    job: Option<katran::domain_jobs::JobId>,
    preview_job: Option<katran::domain_jobs::JobId>,
    label: &'static str,
    status: String,
    kind: ImageJob,
    collection: Option<Collection>,
    progress: Option<(u64, u64)>,
}

struct Collection {
    id: i64,
    revision: String,
    index: usize,
    count: Option<usize>,
    pending: Option<(katran::domain_jobs::JobId, usize)>,
    texture: Option<egui::TextureHandle>,
    error: String,
}

#[derive(Clone, Copy, Default)]
enum ImageJob {
    #[default]
    Upscale,
    Translate,
}

fn label(operation: katran::UpscaleOperation) -> &'static str {
    use katran::UpscaleOperation::*;
    match operation {
        RemacriX2 => "Remacri ×2",
        RemacriX4 => "Remacri ×4",
        RealEsrganX2 => "Real-ESRGAN ×2",
        RealEsrganX4 => "Real-ESRGAN ×4",
    }
}

pub(super) fn translation_source(id: &str) -> Option<katran::api::ImageTranslationSource> {
    use katran::api::ImageTranslationSource::*;
    match id {
        "translate_image_ja" => Some(Japanese),
        "translate_image_zh" => Some(Chinese),
        "translate_image_ko" => Some(Korean),
        _ => None,
    }
}

impl State {
    pub(super) fn collection_open(&self) -> bool {
        self.collection.is_some()
    }
}

fn decode_collection_page(encoded: &str) -> Result<egui::ColorImage, String> {
    if encoded.len() > 32 * 1024 * 1024 {
        return Err("Page is too large".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid page encoding")?;
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader
        .decode()
        .map_err(|_| "Invalid collection page")?
        .into_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

impl Shelf {
    pub(super) fn open_translation_collection(
        &mut self,
        id: i64,
        revision: String,
        ctx: &egui::Context,
    ) {
        self.close_translation_collection();
        self.upscale.collection = Some(Collection {
            id,
            revision,
            index: 0,
            count: None,
            pending: None,
            texture: None,
            error: String::new(),
        });
        self.request_translation_page(0, ctx);
    }

    fn close_translation_collection(&mut self) {
        if let Some(collection) = self.upscale.collection.take() {
            if let Some((job_id, _)) = collection.pending {
                self.call(Command::CancelJob { job_id });
            }
        }
    }

    fn request_translation_page(&mut self, index: usize, ctx: &egui::Context) {
        let Some(mut collection) = self.upscale.collection.take() else {
            return;
        };
        if let Some((job_id, _)) = collection.pending.take() {
            self.call(Command::CancelJob { job_id });
        }
        collection.index = index;
        collection.texture = None;
        collection.error.clear();
        let response = self.call(Command::BeginJob {
            command: Box::new(Command::TranslationCollectionPage {
                id: collection.id,
                revision: collection.revision.clone(),
                index,
            }),
        });
        if let Some(id) = response["job_id"].as_u64() {
            collection.pending = Some((katran::domain_jobs::JobId(id), index));
            ctx.request_repaint_after(Duration::from_millis(100));
        } else {
            collection.error = self.message.clone();
        }
        self.upscale.collection = Some(collection);
    }

    fn translation_collection_ui(&mut self, ctx: &egui::Context) {
        if let Some((job_id, expected)) = self.upscale.collection.as_ref().and_then(|v| v.pending) {
            let response = self.call(Command::PollJob { job_id });
            match response["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
                _ => {
                    if let Some(collection) = self.upscale.collection.as_mut()
                        && collection.pending == Some((job_id, expected))
                        && collection.index == expected
                    {
                        collection.pending = None;
                        if response["state"] == "succeeded" {
                            let page = &response["result"];
                            let count =
                                page["count"].as_u64().and_then(|v| usize::try_from(v).ok());
                            let returned =
                                page["index"].as_u64().and_then(|v| usize::try_from(v).ok());
                            if returned == Some(expected)
                                && count.is_some_and(|n| n > expected && n <= 32)
                            {
                                collection.count = count;
                                match page["png"]
                                    .as_str()
                                    .ok_or_else(|| "Missing page image".to_owned())
                                    .and_then(decode_collection_page)
                                {
                                    Ok(image) => {
                                        collection.texture = Some(ctx.load_texture(
                                            "translated-collection-page",
                                            image,
                                            egui::TextureOptions::LINEAR,
                                        ))
                                    }
                                    Err(error) => collection.error = error,
                                }
                            } else {
                                collection.error =
                                    tr("Некорректный ответ страницы", "Invalid page response")
                                        .into();
                            }
                        } else {
                            collection.error = response["error"]["message"]
                                .as_str()
                                .unwrap_or(tr(
                                    "Не удалось загрузить страницу",
                                    "Could not load page",
                                ))
                                .into();
                        }
                    }
                }
            }
        }
        if self.upscale.collection.is_none() {
            return;
        }
        let close_key = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        let previous_key =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
        let next_key =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        let collection = self.upscale.collection.as_ref().unwrap();
        let mut previous = previous_key;
        let mut next = next_key;
        let mut close = close_key;
        let modal = egui::Modal::new(egui::Id::new("translated-collection")).show(ctx, |ui| {
            ui.heading(tr("Перевод страниц", "Translated pages"));
            ui.horizontal(|ui| {
                previous |= ui
                    .add_enabled(
                        collection.index > 0,
                        egui::Button::new(tr("Назад", "Previous")),
                    )
                    .clicked();
                ui.label(format!(
                    "{} / {}",
                    collection.index + 1,
                    collection
                        .count
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "…".into())
                ));
                next |= ui
                    .add_enabled(
                        collection.count.is_some_and(|n| collection.index + 1 < n),
                        egui::Button::new(tr("Далее", "Next")),
                    )
                    .clicked();
                close |= ui.button(tr("Закрыть", "Close")).clicked();
            });
            if collection.pending.is_some() {
                ui.spinner();
            }
            if !collection.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &collection.error);
            }
            if let Some(texture) = collection.texture.as_ref() {
                let available = ctx.content_rect().size() * 0.8;
                let original = texture.size_vec2();
                let scale = (available.x / original.x)
                    .min((available.y - 80.0).max(1.0) / original.y)
                    .min(1.0);
                ui.add(egui::Image::new((texture.id(), original * scale)));
            }
        });
        let current = collection.index;
        let count = collection.count;
        if close || modal.should_close() {
            self.close_translation_collection();
        } else if previous && current > 0 {
            self.request_translation_page(current - 1, ctx);
        } else if next && count.is_some_and(|n| current + 1 < n) {
            self.request_translation_page(current + 1, ctx);
        }
    }

    pub(super) fn start_upscale(
        &mut self,
        operation: katran::UpscaleOperation,
        ctx: &egui::Context,
    ) {
        self.start_image_job(
            Command::UpscaleSelectedImage { operation },
            label(operation),
            ImageJob::Upscale,
            ctx,
        );
    }

    pub(super) fn start_image_translation(
        &mut self,
        source: katran::api::ImageTranslationSource,
        ctx: &egui::Context,
    ) {
        use katran::api::ImageTranslationSource::*;
        let (command, label) = match source {
            Japanese => (
                Command::TranslateSelectedImage,
                tr(
                    "Перевод изображения с японского",
                    "Image translation from Japanese",
                ),
            ),
            Chinese => (
                Command::TranslateSelectedImageFrom { source },
                tr(
                    "Перевод изображения с упрощённого китайского",
                    "Image translation from Simplified Chinese",
                ),
            ),
            Korean => (
                Command::TranslateSelectedImageFrom { source },
                tr(
                    "Перевод изображения с корейского",
                    "Image translation from Korean",
                ),
            ),
        };
        self.start_image_job(command, label, ImageJob::Translate, ctx);
    }

    fn start_image_job(
        &mut self,
        command: Command,
        label: &'static str,
        kind: ImageJob,
        ctx: &egui::Context,
    ) {
        if self.upscale.preview_job.is_some() {
            self.message = tr(
                "Открытие сохранённого перевода…",
                "Opening saved translation…",
            )
            .into();
            return;
        }
        if self.upscale.job.is_some() {
            self.upscale.open = true;
            return;
        }
        let result = self.call(command);
        let job = result["job_id"].as_u64().map(katran::domain_jobs::JobId);
        self.upscale = State {
            open: true,
            job,
            preview_job: None,
            collection: None,
            progress: None,
            label,
            kind,
            status: if job.is_some() {
                String::new()
            } else {
                self.message.clone()
            },
        };
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    pub(super) fn upscale_ui(&mut self, ctx: &egui::Context) {
        if let Some(job_id) = self.upscale.preview_job {
            let response = self.call(Command::PollJob { job_id });
            match response["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
                Some("succeeded") => {
                    self.upscale.preview_job = None;
                    self.message = self.upscale.status.clone();
                }
                _ => {
                    self.upscale.preview_job = None;
                    let reason = response["error"]["message"]
                        .as_str()
                        .unwrap_or(tr("не удалось открыть просмотр", "could not open preview"));
                    self.message = format!("{} QuickLook: {reason}", self.upscale.status);
                }
            }
        }
        if let Some(job_id) = self.upscale.job {
            let response = self.call(Command::PollJob { job_id });
            match response["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    self.upscale.progress = response["progress"]["completed"]
                        .as_u64()
                        .zip(response["progress"]["total"].as_u64());
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
                Some("succeeded") => {
                    self.upscale.job = None;
                    let result = &response["result"];
                    self.upscale.status = match self.upscale.kind {
                        ImageJob::Translate => {
                            let mut status =
                                tr("PNG сохранён в Translate.", "PNG saved in Translate.")
                                    .to_owned();
                            if let Some(count) =
                                result["fallback_blocks"].as_u64().filter(|n| *n > 0)
                            {
                                let side = result["side_blocks"].as_u64().unwrap_or(0);
                                if side > 0 {
                                    status.push_str(&format!(
                                        " {} {}",
                                        side,
                                        tr(
                                            "обл. переведено на полях.",
                                            "region(s) translated in the margins."
                                        ),
                                    ));
                                }
                                if count > side {
                                    status.push_str(&format!(
                                        " {} {}",
                                        count - side,
                                        tr(
                                            "обл. переведено под страницей.",
                                            "region(s) translated below the page."
                                        ),
                                    ));
                                }
                            }
                            if let Some(count) =
                                result["skipped_blocks"].as_u64().filter(|n| *n > 0)
                            {
                                status.push_str(&format!(
                                    " {} {}",
                                    count,
                                    tr(
                                        "обл. осталось без перевода.",
                                        "region(s) left untranslated."
                                    ),
                                ));
                            }
                            status
                        }
                        ImageJob::Upscale => format!(
                            "{}: {} × {} → {} × {}. {}",
                            tr("Размер", "Size"),
                            result["input_width"],
                            result["input_height"],
                            result["width"],
                            result["height"],
                            tr("Сохранено в Images", "Saved in Images")
                        ),
                    };
                    self.refresh();
                    if matches!(self.upscale.kind, ImageJob::Translate)
                        && result["collection"] == true
                    {
                        self.upscale.open = false;
                        let event = &result["event"];
                        if let (Some(id), Some(revision)) =
                            (event["id"].as_i64(), event["revision"].as_str())
                        {
                            let total = result["total"].as_u64().unwrap_or(0);
                            let translated = result["translated"].as_u64().unwrap_or(0);
                            self.message = format!(
                                "{} {translated}/{total}",
                                tr("Сохранено страниц:", "Pages saved:")
                            );
                            self.open_translation_collection(id, revision.to_owned(), ctx);
                        } else {
                            self.message = tr(
                                "Коллекция сохранена, но её не удалось открыть.",
                                "Collection saved, but could not be opened.",
                            )
                            .into();
                        }
                    } else if matches!(self.upscale.kind, ImageJob::Translate) {
                        // Close the Katran modal before launching the external viewer,
                        // leaving focus and Space handling to QuickLook.
                        self.upscale.open = false;
                        self.message = self.upscale.status.clone();
                        let event = &result["event"];
                        if let (Some(id), Some(revision)) =
                            (event["id"].as_i64(), event["revision"].as_str())
                        {
                            let response = self.call(Command::BeginJob {
                                command: Box::new(Command::PreviewPublishedTranslation {
                                    id,
                                    revision: revision.to_owned(),
                                }),
                            });
                            if let Some(id) = response["job_id"].as_u64() {
                                self.upscale.preview_job = Some(katran::domain_jobs::JobId(id));
                                ctx.request_repaint_after(Duration::from_millis(100));
                            } else {
                                let reason = if self.message == self.upscale.status {
                                    tr("не удалось начать просмотр", "could not start preview")
                                        .to_owned()
                                } else {
                                    self.message.clone()
                                };
                                self.message =
                                    format!("{} QuickLook: {reason}", self.upscale.status);
                            }
                        } else {
                            self.message = format!(
                                "{} {}",
                                self.upscale.status,
                                tr(
                                    "Не удалось открыть QuickLook: отсутствует идентификатор результата.",
                                    "Could not open QuickLook: result identity is missing."
                                )
                            );
                        }
                    }
                }
                Some("cancelled") => {
                    self.upscale.job = None;
                    self.upscale.status = tr("Операция отменена", "Operation cancelled").into();
                }
                Some("failed") => {
                    self.upscale.job = None;
                    self.upscale.status = response["error"]["message"]
                        .as_str()
                        .unwrap_or(match self.upscale.kind {
                            ImageJob::Upscale => tr(
                                "Не удалось увеличить изображение",
                                "Could not upscale image",
                            ),
                            ImageJob::Translate => tr(
                                "Не удалось перевести изображение",
                                "Could not translate image",
                            ),
                        })
                        .into();
                }
                _ => {
                    self.upscale.job = None;
                    self.upscale.status = tr(
                        "Не удалось получить состояние операции",
                        "Could not retrieve operation status",
                    )
                    .into();
                }
            }
        }
        self.translation_collection_ui(ctx);
        if !self.upscale.open {
            return;
        }
        let mut cancel = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("image-upscale")).show(ctx, |ui| {
            ui.heading(self.upscale.label);
            if self.upscale.job.is_some() {
                ui.spinner();
                ui.label(match self.upscale.kind {
                    ImageJob::Upscale => tr("Увеличение изображения…", "Upscaling image…"),
                    ImageJob::Translate => tr("Перевод изображения…", "Translating image…"),
                });
                if let Some((completed, total)) = self.upscale.progress {
                    ui.label(format!("{completed} / {total}"));
                }
                cancel = ui.button(tr("Отменить", "Cancel")).clicked();
            } else {
                ui.label(&self.upscale.status);
                close = ui.button(tr("Закрыть", "Close")).clicked();
            }
        });
        if cancel {
            if let Some(job_id) = self.upscale.job {
                self.call(Command::CancelJob { job_id });
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.upscale.job.is_none() && (close || modal.should_close()) {
            self.upscale.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katran::actions::upscale_operation;
    #[test]
    fn translation_actions_keep_sources_separate() {
        use katran::api::ImageTranslationSource::*;
        assert_eq!(translation_source("translate_image_ja"), Some(Japanese));
        assert_eq!(translation_source("translate_image_zh"), Some(Chinese));
        assert_eq!(translation_source("translate_image_ko"), Some(Korean));
        assert_eq!(translation_source("translate_image"), None);
    }
    #[test]
    fn four_actions_map_to_distinct_operations() {
        let names = [
            "upscale_remacri_x2",
            "upscale_remacri_x4",
            "upscale_realesrgan_x2",
            "upscale_realesrgan_x4",
        ];
        let labels: Vec<_> = names
            .iter()
            .map(|id| label(upscale_operation(id).unwrap()))
            .collect();
        assert_eq!(
            labels,
            [
                "Remacri ×2",
                "Remacri ×4",
                "Real-ESRGAN ×2",
                "Real-ESRGAN ×4"
            ]
        );
        assert!(upscale_operation("unknown").is_none());
    }
}

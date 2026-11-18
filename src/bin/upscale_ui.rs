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
    ready_index: Option<usize>,
    copy_held: bool,
    zoom: katran::preview_view::State,
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

// A platform can emit both Copy and Ctrl+C for one physical press. Consume
// both, and reject key repeats while retaining Copy-only platform support.
fn take_collection_copy(
    events: &mut Vec<egui::Event>,
    focused: bool,
    key_down: bool,
    held: &mut bool,
) -> bool {
    if !focused {
        *held = false;
    }
    let mut copy_event = false;
    let mut pressed = false;
    let mut repeated = false;
    let mut released = false;
    events.retain(|event| match event {
        egui::Event::Copy => {
            copy_event = true;
            false
        }
        egui::Event::Key {
            key: egui::Key::C,
            pressed: down,
            repeat,
            modifiers,
            ..
        } if (modifiers.ctrl && !modifiers.alt && !modifiers.shift) || (!*down && *held) => {
            if *down {
                if *repeat {
                    repeated = true;
                } else {
                    pressed = true;
                }
            } else {
                released = true;
            }
            false
        }
        _ => true,
    });
    if !focused {
        return false;
    }
    if released {
        *held = false;
    }
    let fire = !*held && (pressed || (copy_event && !repeated));
    if pressed || key_down {
        *held = true;
    } else if !key_down {
        *held = false;
    }
    fire
}

fn collection_canvas_size(viewport: egui::Vec2) -> egui::Vec2 {
    egui::vec2((viewport.x * 0.8).max(1.0), (viewport.y * 0.7).max(1.0))
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
            ready_index: None,
            copy_held: false,
            zoom: katran::preview_view::State::default(),
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
                                        ));
                                        collection.ready_index = Some(expected);
                                        collection.zoom = katran::preview_view::State::default();
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
        let copy_requested = {
            let collection = self.upscale.collection.as_mut().unwrap();
            ctx.input_mut(|input| {
                let key_down = input.key_down(egui::Key::C);
                let focused = input.focused;
                take_collection_copy(
                    &mut input.events,
                    focused,
                    key_down,
                    &mut collection.copy_held,
                )
            })
        };
        if copy_requested {
            let target = self.upscale.collection.as_ref().and_then(|c| {
                c.ready_index
                    .filter(|_| c.texture.is_some())
                    .map(|index| (c.id, c.revision.clone(), index))
            });
            if let Some((id, revision, index)) = target {
                if !self.start_job(
                    Command::CopyCollectionPage {
                        id,
                        revision,
                        index,
                    },
                    false,
                    ctx.clone(),
                ) && self.job.is_some()
                {
                    self.message = tr(
                        "Дождитесь завершения текущей операции",
                        "Wait for the current operation to finish",
                    )
                    .into();
                }
            }
        }
        let close_key = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        let previous_key =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
        let next_key =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        let collection = self.upscale.collection.as_mut().unwrap();
        let mut previous = previous_key;
        let mut next = next_key;
        let mut close = close_key;
        let canvas_size = collection_canvas_size(ctx.content_rect().size());
        let modal = egui::Modal::new(egui::Id::new("translated-collection")).show(ctx, |ui| {
            ui.set_min_width(canvas_size.x);
            ui.heading(tr("Перевод страниц", "Translated pages"));
            ui.horizontal(|ui| {
                previous |= ui
                    .add_enabled(
                        collection.index > 0,
                        egui::Button::new(tr("Назад", "Previous")),
                    )
                    .clicked();
                let shown = collection
                    .ready_index
                    .map(|n| (n + 1).to_string())
                    .unwrap_or_else(|| "…".into());
                let total = collection
                    .count
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "…".into());
                ui.add_sized(
                    [90.0, ui.spacing().interact_size.y],
                    egui::Label::new(format!("{shown} / {total}")),
                );
                next |= ui
                    .add_enabled(
                        collection.count.is_some_and(|n| collection.index + 1 < n),
                        egui::Button::new(tr("Далее", "Next")),
                    )
                    .clicked();
                close |= ui.button(tr("Закрыть", "Close")).clicked();
            });
            let (canvas, response) =
                ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
            #[cfg(test)]
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("collection-test-canvas"), canvas));
            let painter = ui.painter().with_clip_rect(canvas);
            painter.rect_filled(canvas, 0.0, egui::Color32::from_gray(24));
            if let Some(texture) = collection.texture.as_ref() {
                let original = texture.size_vec2();
                let fit_scale = (canvas.width() / original.x).min(canvas.height() / original.y);
                let fitted = original * fit_scale;
                use katran::preview_view::{Point, Size};
                let viewport = Size {
                    width: canvas.width(),
                    height: canvas.height(),
                };
                let size = Size {
                    width: fitted.x,
                    height: fitted.y,
                };
                if response.hovered() {
                    let (wheel, center_zoom, cursor) = ui.input(|i| {
                        (
                            i.smooth_scroll_delta().y,
                            i.zoom_delta(),
                            i.pointer.hover_pos(),
                        )
                    });
                    if wheel.abs() > 0.01
                        && let Some(cursor) = cursor
                    {
                        let _ = collection.zoom.zoom(
                            (wheel * 0.003).exp(),
                            Point {
                                x: cursor.x - canvas.min.x,
                                y: cursor.y - canvas.min.y,
                            },
                            viewport,
                            size,
                        );
                    }
                    if (center_zoom - 1.0).abs() > 0.001 {
                        let _ = collection.zoom.zoom(
                            center_zoom,
                            Point {
                                x: viewport.width / 2.0,
                                y: viewport.height / 2.0,
                            },
                            viewport,
                            size,
                        );
                    }
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    || response.dragged_by(egui::PointerButton::Middle)
                {
                    let delta = ui.input(|i| i.pointer.delta());
                    let _ = collection.zoom.pan(
                        Point {
                            x: delta.x,
                            y: delta.y,
                        },
                        viewport,
                        size,
                    );
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                if let Ok(geometry) = collection.zoom.geometry(viewport, size) {
                    if let (Some(left), Some(top), Some(width), Some(height)) = (
                        geometry["left"].as_f64(),
                        geometry["top"].as_f64(),
                        geometry["width"].as_f64(),
                        geometry["height"].as_f64(),
                    ) {
                        let rect = egui::Rect::from_min_size(
                            canvas.min + egui::vec2(left as f32, top as f32),
                            egui::vec2(width as f32, height as f32),
                        );
                        painter.image(
                            texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                }
            }
            if !collection.error.is_empty() {
                let anchor = canvas.left_top() + egui::vec2(12.0, 12.0);
                painter.text(
                    anchor,
                    egui::Align2::LEFT_TOP,
                    &collection.error,
                    egui::FontId::proportional(15.0),
                    egui::Color32::LIGHT_RED,
                );
            }
        });
        #[cfg(test)]
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("collection-test-modal"), modal.response.rect)
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
    fn collection_copy_consumes_dual_events_and_ignores_repeat() {
        let event = |pressed, repeat| egui::Event::Key {
            key: egui::Key::C,
            physical_key: None,
            pressed,
            repeat,
            modifiers: egui::Modifiers::CTRL,
        };
        let mut held = false;
        let mut first = vec![egui::Event::Copy, event(true, false)];
        assert!(take_collection_copy(&mut first, true, true, &mut held));
        assert!(first.is_empty());
        let mut repeated = vec![egui::Event::Copy, event(true, true)];
        assert!(!take_collection_copy(&mut repeated, true, true, &mut held));
        assert!(repeated.is_empty());
        let mut released = vec![event(false, false)];
        assert!(!take_collection_copy(&mut released, true, false, &mut held));
        let mut next = vec![egui::Event::Copy];
        assert!(take_collection_copy(&mut next, true, false, &mut held));
        assert!(next.is_empty());
    }

    #[test]
    fn collection_canvas_and_modal_keep_size_when_page_arrives() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        app.upscale.collection = Some(Collection {
            id: 1,
            revision: "test".into(),
            index: 0,
            count: Some(2),
            pending: None,
            texture: None,
            ready_index: None,
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        });
        let frame = |app: &mut Shelf| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 800.0),
                    )),
                    ..Default::default()
                },
                |_| app.translation_collection_ui(&ctx),
            );
            output.textures_delta.clear();
            ctx.data(|d| {
                (
                    d.get_temp::<egui::Rect>(egui::Id::new("collection-test-canvas"))
                        .unwrap(),
                    d.get_temp::<egui::Rect>(egui::Id::new("collection-test-modal"))
                        .unwrap(),
                )
            })
        };
        frame(&mut app);
        let before = frame(&mut app);
        let texture = ctx.load_texture(
            "fixture-page",
            egui::ColorImage::from_rgba_unmultiplied([2, 1], &[255; 8]),
            egui::TextureOptions::LINEAR,
        );
        let collection = app.upscale.collection.as_mut().unwrap();
        collection.texture = Some(texture);
        collection.ready_index = Some(0);
        frame(&mut app);
        let after = frame(&mut app);
        assert_eq!(before, after);
    }

    #[test]
    fn failed_new_page_request_keeps_the_last_ready_texture() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "fixture-ready",
            egui::ColorImage::from_rgba_unmultiplied([1, 1], &[255; 4]),
            egui::TextureOptions::LINEAR,
        );
        let texture_id = texture.id();
        app.upscale.collection = Some(Collection {
            id: 99999,
            revision: "missing".into(),
            index: 0,
            count: Some(2),
            pending: None,
            texture: Some(texture),
            ready_index: Some(0),
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        });
        app.request_translation_page(1, &ctx);
        let collection = app.upscale.collection.as_ref().unwrap();
        assert_eq!(collection.index, 1);
        assert_eq!(collection.ready_index, Some(0));
        assert_eq!(collection.texture.as_ref().unwrap().id(), texture_id);
        assert!(!collection.error.is_empty());
    }

    #[test]
    fn collection_zoom_uses_shared_anchor_and_pan_limits() {
        use katran::preview_view::{Point, Size, State};
        let viewport = Size {
            width: 600.0,
            height: 400.0,
        };
        let fitted = Size {
            width: 400.0,
            height: 400.0,
        };
        let mut cursor_zoom = State::default();
        cursor_zoom
            .zoom(2.0, Point { x: 450.0, y: 200.0 }, viewport, fitted)
            .unwrap();
        assert!(cursor_zoom.offset.x < 0.0);
        let mut center_zoom = State::default();
        center_zoom
            .zoom(2.0, Point { x: 300.0, y: 200.0 }, viewport, fitted)
            .unwrap();
        assert_eq!(center_zoom.offset.x, 0.0);
        center_zoom
            .pan(
                Point {
                    x: 10000.0,
                    y: 10000.0,
                },
                viewport,
                fitted,
            )
            .unwrap();
        assert_eq!(center_zoom.offset.x, 100.0);
        assert_eq!(center_zoom.offset.y, 200.0);
    }

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

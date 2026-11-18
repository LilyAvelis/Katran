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
    collection_title: String,
    collection_is_bull: bool,
    collection_fullscreen: bool,
    link_rename: Option<LinkRename>,
    progress: Option<(u64, u64)>,
    partial_translation: Option<PartialTranslation>,
}

struct Collection {
    id: i64,
    revision: String,
    index: usize,
    count: Option<usize>,
    pending: Option<(katran::domain_jobs::JobId, usize)>,
    texture: Option<egui::TextureHandle>,
    transition_poster: Option<egui::TextureHandle>,
    opened_at: Instant,
    link: Option<LinkPage>,
    text: Option<TextPage>,
    file: Option<FilePage>,
    ready_index: Option<usize>,
    copy_held: bool,
    zoom: katran::preview_view::State,
    error: String,
}

impl Collection {
    fn video_ready(&self) -> bool {
        self.ready_index == Some(self.index) && self.file.as_ref().is_some_and(|file| file.video)
    }

    fn ready_target(&self) -> Option<(i64, String, usize)> {
        self.ready_index
            .filter(|_| {
                self.texture.is_some()
                    || self.link.is_some()
                    || self.text.is_some()
                    || self.file.is_some()
            })
            .map(|index| (self.id, self.revision.clone(), index))
    }
    fn ready_transfer_target(&self) -> Option<(i64, String, usize)> {
        if self.file.is_some() {
            None
        } else {
            self.ready_target()
        }
    }
}

struct PartialTranslation {
    event: Value,
    failures: Vec<String>,
}

fn partial_translation_failures(result: &Value) -> Vec<String> {
    result["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|page| {
            let number = page["index"].as_u64().and_then(|n| n.checked_add(1));
            let label = number
                .map(|n| format!("{} {n}", tr("Страница", "Page")))
                .unwrap_or_else(|| tr("Страница", "Page").into());
            if page["ok"] == false {
                let message: String = page["error"]["message"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(tr("Не удалось перевести", "Translation failed"))
                    .chars()
                    .take(512)
                    .collect();
                let preserved = if page["preserved_original"] == true {
                    tr(" · оригинал сохранён", " · original preserved")
                } else {
                    ""
                };
                Some(format!(
                    "{label}: {}: {message}{preserved}",
                    tr("ошибка страницы", "page failed")
                ))
            } else if page["ok"] == true {
                page["skipped_blocks"]
                    .as_u64()
                    .filter(|count| *count > 0)
                    .map(|count| {
                        format!(
                            "{label}: {} {count}",
                            tr("непереведённых областей:", "untranslated regions:")
                        )
                    })
            } else {
                None
            }
        })
        .collect()
}

fn translation_is_partial(result: &Value) -> bool {
    result["pages"].as_array().is_some_and(|pages| {
        pages.iter().any(|page| {
            page["ok"] == false
                || (page["ok"] == true && page["skipped_blocks"].as_u64().is_some_and(|n| n > 0))
        })
    }) || result["translated"]
        .as_u64()
        .zip(result["total"].as_u64())
        .is_some_and(|(done, total)| done < total)
}

fn partial_translation_status(result: &Value) -> String {
    let translated = result["translated"].as_u64().unwrap_or(0);
    let total = result["total"].as_u64().unwrap_or(0);
    let saved = result["pages"]
        .as_array()
        .map(|pages| pages.iter().filter(|page| page["page"].is_u64()).count() as u64)
        .unwrap_or(translated);
    format!(
        "{} {saved}/{total} · {} {translated}/{total} · {}",
        tr("Сохранено страниц:", "Pages saved:"),
        tr("Переведено:", "Translated:"),
        tr("BULL сохранён в Translate", "BULL saved in Translate")
    )
}

struct FilePage {
    name: String,
    size: u64,
    extension: String,
    video: bool,
}

struct TextPage {
    source: String,
    document: Option<katran::markdown::Document>,
    rendered: bool,
}

struct LinkPage {
    url: String,
    title: String,
    description: String,
    editable: bool,
}

struct LinkRename {
    title: String,
    focus: bool,
    job: Option<katran::domain_jobs::JobId>,
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
        "translate_image_zh_hant" => Some(TraditionalChinese),
        "translate_image_ko" => Some(Korean),
        _ => None,
    }
}

impl State {
    pub(super) fn collection_open(&self) -> bool {
        self.collection.is_some()
    }

    pub(super) fn collection_can_fullscreen(&self) -> bool {
        self.collection_open() && self.collection_is_bull
    }

    pub(super) fn saving_link_name(&self) -> bool {
        self.link_rename
            .as_ref()
            .is_some_and(|edit| edit.job.is_some())
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

fn take_collection_enter(events: &mut Vec<egui::Event>, focused: bool) -> bool {
    let mut pressed = false;
    events.retain(|event| match event {
        egui::Event::Key {
            key: egui::Key::Enter,
            pressed: down,
            repeat,
            modifiers,
            ..
        } if modifiers.is_none() => {
            if focused && *down && !*repeat {
                pressed = true;
            }
            false
        }
        _ => true,
    });
    pressed
}

fn collection_modal_layer() -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("translated-collection"),
    )
}

const COLLECTION_INTRO: Duration = Duration::from_millis(300);

fn collection_intro_transform(elapsed: Duration, center: egui::Pos2) -> egui::emath::TSTransform {
    let t = (elapsed.as_secs_f32() / COLLECTION_INTRO.as_secs_f32()).clamp(0.0, 1.0);
    let remaining = (1.0 - t).powi(3);
    let scale = 1.0 - 0.025 * remaining;
    egui::emath::TSTransform::new(
        center.to_vec2() * (1.0 - scale) + egui::vec2(0.0, 8.0 * remaining),
        scale,
    )
}

fn upscale_collection_status(result: &Value) -> String {
    let upscaled = result["upscaled"].as_u64().unwrap_or(0);
    let total = result["total"].as_u64().unwrap_or(0);
    format!(
        "{} {upscaled}/{total} · {}",
        tr("Увеличено страниц:", "Pages upscaled:"),
        tr("Сохранено в Images", "Saved in Images")
    )
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
    pub(super) fn collection_media_target(&self) -> Option<super::media_preview_ui::Target> {
        let collection = self
            .upscale
            .collection
            .as_ref()
            .filter(|collection| collection.video_ready())?;
        Some(super::media_preview_ui::collection_target(
            collection.id,
            &collection.revision,
            collection.index,
            &self.state,
        ))
    }

    fn open_ready_collection_entry(&mut self, ctx: &egui::Context) {
        let target = self.upscale.collection.as_ref().and_then(|c| {
            c.ready_target().and_then(|(id, revision, index)| {
                if c.file.is_some() {
                    Some(Command::OpenBsEntry {
                        id,
                        revision,
                        index,
                    })
                } else if c.link.is_some() {
                    Some(Command::OpenCollectionLink {
                        id,
                        revision,
                        index,
                    })
                } else {
                    None
                }
            })
        });
        if let Some(command) = target
            && !self.start_job(command, false, ctx.clone())
            && self.job.is_some()
        {
            self.message = tr(
                "Дождитесь завершения текущей операции",
                "Wait for the current operation to finish",
            )
            .into();
        }
    }

    pub(super) fn open_translation_collection(&mut self, event: &Value, ctx: &egui::Context) {
        let (Some(id), Some(revision)) = (event["id"].as_i64(), event["revision"].as_str()) else {
            return;
        };
        let title = self
            .events
            .iter()
            .find(|row| row["id"].as_i64() == Some(id))
            .and_then(|row| row["properties"]["display_name"].as_str())
            .filter(|name| !name.trim().is_empty())
            .or_else(|| event["name"].as_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(tr("Коллекция", "Collection"))
            .to_owned();
        self.close_translation_collection(ctx);
        self.upscale.collection_title = title;
        self.upscale.collection_is_bull = event["name"]
            .as_str()
            .and_then(|name| std::path::Path::new(name).extension())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bull"));
        self.upscale.collection = Some(Collection {
            id,
            revision: revision.into(),
            index: 0,
            count: None,
            pending: None,
            texture: None,
            transition_poster: None,
            opened_at: Instant::now(),
            link: None,
            text: None,
            file: None,
            ready_index: None,
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        });
        self.request_translation_page(0, ctx);
    }

    pub(super) fn accept_link_collection_revision(
        &mut self,
        id: i64,
        revision: &str,
        ctx: &egui::Context,
    ) {
        let Some(collection) = self.upscale.collection.as_mut() else {
            return;
        };
        if collection.id != id || collection.revision == revision {
            return;
        }
        collection.revision = revision.to_owned();
        let target = collection.index;
        // Keep the ready card visible while replacing any obsolete pending page.
        self.request_translation_page(target, ctx);
    }

    pub(super) fn toggle_collection_fullscreen(&mut self, ctx: &egui::Context) {
        if self.upscale.collection_can_fullscreen() && self.upscale.link_rename.is_none() {
            let fullscreen = !self.upscale.collection_fullscreen;
            self.upscale.collection_fullscreen = fullscreen;
            self.overlay_ui.animation.cancel(self.native_window);
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
            ctx.request_repaint();
        }
    }

    pub(super) fn exit_collection_fullscreen(&mut self, ctx: &egui::Context) -> bool {
        let was_fullscreen = std::mem::take(&mut self.upscale.collection_fullscreen);
        if was_fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.request_repaint();
        }
        was_fullscreen
    }

    fn close_translation_collection(&mut self, ctx: &egui::Context) {
        self.exit_collection_fullscreen(ctx);
        self.upscale.collection_is_bull = false;
        ctx.set_transform_layer(collection_modal_layer(), egui::emath::TSTransform::IDENTITY);
        if self.collection_media_target().is_some() {
            self.stop_media_preview();
        }
        if let Some(job_id) = self.upscale.link_rename.take().and_then(|edit| edit.job) {
            self.call(Command::CancelJob { job_id });
        }
        if let Some(collection) = self.upscale.collection.take()
            && let Some((job_id, _)) = collection.pending
        {
            self.call(Command::CancelJob { job_id });
        }
    }

    fn poll_link_rename(&mut self, ctx: &egui::Context) {
        let Some(job_id) = self.upscale.link_rename.as_ref().and_then(|edit| edit.job) else {
            return;
        };
        let status = self.call(Command::PollJob { job_id });
        if matches!(
            status["state"].as_str(),
            Some("queued" | "running" | "ready")
        ) {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        let result = &status["result"];
        if status["state"] == "succeeded"
            && let (Some(id), Some(revision), Some(index), Some(title)) = (
                result["id"].as_i64(),
                result["revision"].as_str(),
                result["index"].as_u64(),
                result["title"].as_str(),
            )
            && let Some(collection) = self.upscale.collection.as_mut()
            && collection.id == id
            && collection.ready_index == Some(index as usize)
            && let Some(link) = collection.link.as_mut()
        {
            collection.revision = revision.into();
            link.title = title.into();
            self.upscale.link_rename = None;
            self.refresh();
            return;
        }
        let edit = self.upscale.link_rename.as_mut().unwrap();
        edit.job = None;
        edit.focus = true;
        edit.error = status["error"]["message"]
            .as_str()
            .unwrap_or(tr("Не удалось сохранить имя", "Could not save link name"))
            .into();
        if status["error"]["code"] == "stale_selection"
            && let Some(id) = self
                .upscale
                .collection
                .as_ref()
                .map(|collection| collection.id)
        {
            let event = self.call(Command::Get { id });
            if let Some(revision) = event["revision"].as_str() {
                self.accept_link_collection_revision(id, revision, ctx);
            }
        }
    }

    fn submit_link_rename(&mut self, ctx: &egui::Context) {
        let Some((id, revision, index)) = self
            .upscale
            .collection
            .as_ref()
            .and_then(Collection::ready_target)
        else {
            return;
        };
        let Some(edit) = self.upscale.link_rename.as_ref() else {
            return;
        };
        let result = self.call(Command::RenameCollectionLink {
            id,
            revision,
            index,
            title: edit.title.clone(),
        });
        let edit = self.upscale.link_rename.as_mut().unwrap();
        if let Some(job_id) = result["job_id"].as_u64() {
            edit.job = Some(katran::domain_jobs::JobId(job_id));
            edit.error.clear();
            ctx.request_repaint_after(Duration::from_millis(50));
        } else {
            edit.error.clone_from(&self.message);
            edit.focus = true;
        }
    }

    fn request_translation_page(&mut self, index: usize, ctx: &egui::Context) {
        if let Some(target) = self.collection_media_target() {
            let frame = self.media_preview.frame_texture_for(&target);
            if let Some(collection) = self.upscale.collection.as_mut() {
                collection.transition_poster = frame
                    .or_else(|| collection.texture.clone())
                    .or_else(|| collection.transition_poster.take());
            }
            self.stop_media_preview();
        }
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
            ctx.request_repaint_after(Duration::from_millis(32));
        } else {
            collection.error = self.message.clone();
        }
        self.upscale.collection = Some(collection);
    }

    fn translation_collection_ui(&mut self, ctx: &egui::Context) {
        self.poll_link_rename(ctx);
        if let Some((job_id, expected)) = self.upscale.collection.as_ref().and_then(|v| v.pending) {
            let response = self.call(Command::PollJob { job_id });
            match response["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(32))
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
                                && count.is_some_and(|n| {
                                    n > expected && n <= baloh_collection::MAX_ENTRIES
                                })
                            {
                                collection.count = count;
                                if let Some(file) = page["file"].as_object() {
                                    let name =
                                        file.get("name").and_then(Value::as_str).unwrap_or("");
                                    if let Some(size) = file.get("size").and_then(Value::as_u64)
                                        && !name.is_empty()
                                        && name.len() <= 4096
                                    {
                                        let extension = file
                                            .get("extension")
                                            .and_then(Value::as_str)
                                            .unwrap_or("")
                                            .chars()
                                            .take(128)
                                            .collect();
                                        let texture = file
                                            .get("preview_png")
                                            .and_then(Value::as_str)
                                            .and_then(|encoded| {
                                                decode_collection_page(encoded).ok()
                                            })
                                            .map(|image| {
                                                ctx.load_texture(
                                                    "collection-file-preview",
                                                    image,
                                                    egui::TextureOptions::LINEAR,
                                                )
                                            });
                                        let video = file.get("video").and_then(Value::as_bool)
                                            == Some(true);
                                        if !video {
                                            collection.transition_poster = None;
                                        }
                                        collection.file = Some(FilePage {
                                            name: name.to_owned(),
                                            size,
                                            extension,
                                            video,
                                        });
                                        collection.text = None;
                                        collection.link = None;
                                        collection.texture = texture;
                                        collection.ready_index = Some(expected);
                                        collection.zoom = katran::preview_view::State::default();
                                    } else {
                                        collection.error =
                                            tr("Некорректная запись файла", "Invalid file entry")
                                                .into();
                                    }
                                } else if let Some(link) = page["link"].as_object() {
                                    let url = link.get("url").and_then(Value::as_str).unwrap_or("");
                                    if url.is_empty() || url.len() > 16384 {
                                        collection.error =
                                            tr("Некорректная ссылка", "Invalid link").into();
                                    } else {
                                        let title = link
                                            .get("title")
                                            .and_then(Value::as_str)
                                            .unwrap_or("")
                                            .chars()
                                            .take(4096)
                                            .collect();
                                        let description = link
                                            .get("description")
                                            .and_then(Value::as_str)
                                            .unwrap_or("")
                                            .chars()
                                            .take(16384)
                                            .collect();
                                        let texture = link
                                            .get("preview_png")
                                            .and_then(Value::as_str)
                                            .and_then(|encoded| {
                                                decode_collection_page(encoded).ok()
                                            })
                                            .map(|image| {
                                                ctx.load_texture(
                                                    "collection-link-preview",
                                                    image,
                                                    egui::TextureOptions::LINEAR,
                                                )
                                            });
                                        collection.text = None;
                                        collection.file = None;
                                        collection.transition_poster = None;
                                        collection.link = Some(LinkPage {
                                            url: url.to_owned(),
                                            title,
                                            description,
                                            editable: link.get("editable")
                                                == Some(&Value::Bool(true)),
                                        });
                                        collection.texture = texture;
                                        collection.ready_index = Some(expected);
                                        collection.zoom = katran::preview_view::State::default();
                                    }
                                } else if let Some(source) = page["text"]["source"].as_str() {
                                    let document = katran::markdown::parse(source).ok();
                                    collection.text = Some(TextPage {
                                        source: source.to_owned(),
                                        document,
                                        rendered: true,
                                    });
                                    collection.link = None;
                                    collection.file = None;
                                    collection.transition_poster = None;
                                    collection.texture = None;
                                    collection.ready_index = Some(expected);
                                    collection.zoom = katran::preview_view::State::default();
                                } else {
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
                                            collection.link = None;
                                            collection.text = None;
                                            collection.file = None;
                                            collection.transition_poster = None;
                                            collection.ready_index = Some(expected);
                                            collection.zoom =
                                                katran::preview_view::State::default();
                                        }
                                        Err(error) => collection.error = error,
                                    }
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
        let editing = self.upscale.link_rename.is_some();
        let accepts_input = !editing && !self.actions && self.shortcut_editor.is_none();
        if accepts_input && self.upscale.collection_can_fullscreen() && ctx.input(|i| i.focused) {
            if self.upscale.collection_fullscreen
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.exit_collection_fullscreen(ctx);
            } else if let Some(shortcut) = self
                .shortcuts
                .iter()
                .find(|shortcut| shortcut.id == "collection_fullscreen")
                && let Some(action) = shortcuts_ui::take_action_with_editing(
                    ctx,
                    std::slice::from_ref(shortcut),
                    false,
                )
            {
                self.run(action, ctx);
            }
        }
        if accepts_input {
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
            if copy_requested && !self.overlay_ui.pending {
                let target = self
                    .upscale
                    .collection
                    .as_ref()
                    .and_then(Collection::ready_transfer_target);
                if let Some((id, revision, index)) = target
                    && !self.start_job(
                        Command::CopyCollectionPage {
                            id,
                            revision,
                            index,
                        },
                        false,
                        ctx.clone(),
                    )
                    && self.job.is_some()
                {
                    self.message = tr(
                        "Дождитесь завершения текущей операции",
                        "Wait for the current operation to finish",
                    )
                    .into();
                }
            }
            let open_requested = ctx.input_mut(|input| {
                let focused = input.focused;
                let mut pressed = false;
                input.events.retain(|event| match event {
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: down,
                        repeat,
                        modifiers,
                        ..
                    } if modifiers.alt && !modifiers.ctrl && !modifiers.shift => {
                        if focused && *down && !*repeat {
                            pressed = true;
                        }
                        false
                    }
                    _ => true,
                });
                pressed
            });
            if open_requested && !self.overlay_ui.pending {
                self.open_ready_collection_entry(ctx);
            }
            let paste_requested = ctx.input_mut(|input| {
                let focused = input.focused;
                take_collection_enter(&mut input.events, focused)
            });
            if paste_requested {
                let target = self
                    .upscale
                    .collection
                    .as_ref()
                    .and_then(Collection::ready_transfer_target);
                if let Some((id, revision, index)) = target {
                    self.paste_collection_page(ctx, id, revision, index);
                }
            }
        }
        let close_key = accepts_input
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        let previous_key = accepts_input
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
        let next_key = accepts_input
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        let media_target = self
            .media_surface_visible(ctx)
            .then(|| self.collection_media_target())
            .flatten();
        let collection = self.upscale.collection.as_mut().unwrap();
        let title = &self.upscale.collection_title;
        let edit = &mut self.upscale.link_rename;
        let mut previous = previous_key;
        let mut next = next_key;
        let mut close = close_key;
        let mut save_name = false;
        let mut cancel_rename = false;
        let fullscreen = self.upscale.collection_fullscreen;
        let show_chrome = !fullscreen || collection.zoom.factor <= 1.0;
        let elapsed = collection.opened_at.elapsed();
        let viewport = ctx.content_rect();
        let canvas_size = if fullscreen {
            viewport.size()
        } else {
            collection_canvas_size(viewport.size())
        };
        let layer = collection_modal_layer();
        let transform = if fullscreen {
            egui::emath::TSTransform::IDENTITY
        } else {
            collection_intro_transform(elapsed, viewport.center())
        };
        ctx.set_transform_layer(layer, transform);
        // The dialog moves on its layer; the dimming still covers the viewport.
        let backdrop = transform.inverse() * viewport;
        egui::Painter::new(ctx.clone(), layer, backdrop).rect_filled(
            backdrop,
            0.0,
            if fullscreen {
                egui::Color32::from_gray(24)
            } else {
                egui::Color32::from_black_alpha(100)
            },
        );
        let entering = !fullscreen && elapsed < COLLECTION_INTRO;
        if entering {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let mut dialog = egui::Modal::new(layer.id).backdrop_color(egui::Color32::TRANSPARENT);
        if fullscreen {
            dialog = dialog
                .area(
                    egui::Area::new(layer.id)
                        .order(egui::Order::Foreground)
                        .fixed_pos(viewport.min)
                        .default_size(viewport.size())
                        .constrain(false),
                )
                .frame(egui::Frame::NONE);
        }
        let modal = dialog.show(ctx, |ui| {
            ui.set_min_width(canvas_size.x);
            if show_chrome {
                let row_height = ui.spacing().interact_size.y.max(30.0);
                let row_size = egui::vec2(canvas_size.x, row_height);
                let (heading, _) = ui.allocate_exact_size(row_size, egui::Sense::hover());
                let title_rect =
                    heading.shrink2(egui::vec2(row_height.min(heading.width() * 0.25), 0.0));
                ui.place(
                    title_rect,
                    egui::Label::new(egui::RichText::new(title).heading())
                        .halign(egui::Align::Center)
                        .truncate(),
                )
                .on_hover_text(title);
                let close_rect = egui::Rect::from_min_max(
                    heading.right_top() - egui::vec2(row_height.min(heading.width() * 0.25), 0.0),
                    heading.right_bottom(),
                );
                close |= ui
                    .place(
                        close_rect,
                        egui::Button::new("×").frame(false).sense(if editing {
                            egui::Sense::hover()
                        } else {
                            egui::Sense::click()
                        }),
                    )
                    .on_hover_text(tr("Закрыть · Space", "Close · Space"))
                    .clicked()
                    && !editing;

                let (pager, _) = ui.allocate_exact_size(row_size, egui::Sense::hover());
                let counter_width = (pager.width() * 0.4).min(72.0);
                let button_width = (pager.width() * 0.25).min(row_height);
                let offset = (counter_width + button_width) / 2.0
                    + ui.spacing().item_spacing.x.min(pager.width() * 0.05);
                let nav = |ui: &mut egui::Ui, x, label, hint, enabled| {
                    let rect = egui::Rect::from_center_size(
                        pager.center() + egui::vec2(x, 0.0),
                        egui::vec2(button_width, row_height),
                    );
                    ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    ))
                    .add_enabled(enabled, egui::Button::new(label))
                    .on_hover_text(hint)
                    .clicked()
                };
                previous |= nav(
                    ui,
                    -offset,
                    "←",
                    tr("Назад", "Previous"),
                    !editing && collection.index > 0,
                );
                let shown = collection
                    .ready_index
                    .map(|n| (n + 1).to_string())
                    .unwrap_or_else(|| "…".into());
                let total = collection
                    .count
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "…".into());
                let counter = egui::Rect::from_center_size(
                    pager.center(),
                    egui::vec2(counter_width, row_height),
                );
                ui.place(
                    counter,
                    egui::Label::new(format!("{shown} / {total}")).halign(egui::Align::Center),
                );
                next |= nav(
                    ui,
                    offset,
                    "→",
                    tr("Далее", "Next"),
                    !editing && collection.count.is_some_and(|n| collection.index + 1 < n),
                );
                ui.vertical_centered(|ui| {
                    if let Some(link) = collection.link.as_ref().filter(|link| link.editable)
                        && !editing
                        && ui
                            .add_enabled(
                                collection.pending.is_none()
                                    && collection.ready_index == Some(collection.index),
                                egui::Button::new(tr("Переименовать ссылку", "Rename link")),
                            )
                            .clicked()
                    {
                        *edit = Some(LinkRename {
                            title: link.title.clone(),
                            focus: true,
                            job: None,
                            error: String::new(),
                        });
                    }
                });
                if let Some(edit) = edit {
                    let idle = edit.job.is_none();
                    let can_save = idle
                        && collection.pending.is_none()
                        && collection.ready_index == Some(collection.index);
                    ui.horizontal(|ui| {
                        let name = ui.add_enabled(
                            idle,
                            egui::TextEdit::singleline(&mut edit.title)
                                .desired_width((canvas_size.x - 200.0).clamp(100.0, 480.0))
                                .hint_text(tr("Имя ссылки", "Link name")),
                        );
                        if idle && std::mem::take(&mut edit.focus) {
                            name.request_focus();
                        }
                        let enter = name.lost_focus()
                            && ui.input(|input| {
                                input.key_pressed(egui::Key::Enter) && input.modifiers.is_none()
                            });
                        save_name = ui
                            .add_enabled(
                                can_save && !edit.title.trim().is_empty(),
                                egui::Button::new(tr("Сохранить", "Save")),
                            )
                            .clicked()
                            || (can_save && enter && !edit.title.trim().is_empty());
                        cancel_rename = ui
                            .add_enabled(idle, egui::Button::new(tr("Отмена", "Cancel")))
                            .clicked()
                            || (idle
                                && ui.input_mut(|input| {
                                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                }));
                    });
                    if !edit.error.is_empty() {
                        ui.colored_label(egui::Color32::LIGHT_RED, &edit.error);
                    }
                }
            }
            let canvas_size = if fullscreen {
                egui::vec2(
                    canvas_size.x,
                    (viewport.bottom() - ui.next_widget_position().y).max(1.0),
                )
            } else {
                canvas_size
            };
            let (canvas, response) =
                ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
            #[cfg(test)]
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("collection-test-canvas"), canvas));
            let painter = ui.painter().with_clip_rect(canvas);
            painter.rect_filled(canvas, 0.0, egui::Color32::from_gray(24));
            if collection.file.as_ref().is_some_and(|file| file.video) {
                if let Some(texture) = collection
                    .transition_poster
                    .as_ref()
                    .or(collection.texture.as_ref())
                {
                    let rect = super::gallery_card::fit_rect(canvas, texture.size_vec2());
                    painter.image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            } else if let Some(file) = collection.file.as_ref() {
                let inner = canvas.shrink(18.0);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("collection-file-content")
                        .max_rect(inner),
                );
                child.set_clip_rect(inner);
                egui::ScrollArea::vertical()
                    .id_salt(("collection-file-scroll", collection.ready_index))
                    .max_height(inner.height())
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        ui.set_width(inner.width());
                        ui.add_space((inner.height() * 0.12).clamp(0.0, 80.0));
                        ui.vertical_centered(|ui| {
                            let max_size = egui::vec2(
                                inner.width().min(420.0),
                                (inner.height() * 0.55).clamp(1.0, 360.0),
                            );
                            if let Some(texture) = collection.texture.as_ref() {
                                let original = texture.size_vec2();
                                let scale = (max_size.x / original.x).min(max_size.y / original.y);
                                ui.add(egui::Image::new((texture.id(), original * scale)));
                            } else {
                                ui.add(
                                    egui_lucide::Lucide::File
                                        .size(128.0_f32.min(max_size.x).min(max_size.y))
                                        .image(),
                                );
                            }
                            ui.add_space(14.0);
                            ui.heading(&file.name);
                            let kind = if file.extension.is_empty() {
                                tr("Файл", "File")
                            } else {
                                file.extension.as_str()
                            };
                            ui.weak(format!("{kind} · {} {}", file.size, tr("байт", "bytes")));
                        });
                    });
            } else if let Some(text) = collection.text.as_mut() {
                let inner = canvas.shrink(18.0);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("collection-text-content")
                        .max_rect(inner),
                );
                child.set_clip_rect(inner);
                child.horizontal(|ui| {
                    if ui.selectable_label(text.rendered, "Markdown").clicked() {
                        text.rendered = true;
                    }
                    if ui
                        .selectable_label(!text.rendered, tr("Исходный текст", "Source text"))
                        .clicked()
                    {
                        text.rendered = false;
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt(("collection-text-scroll", collection.ready_index))
                    .max_height((inner.height() - child.spacing().interact_size.y - 8.0).max(1.0))
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        ui.set_width(inner.width());
                        if text.rendered
                            && let Some(document) = &text.document
                        {
                            super::markdown_ui::show(ui, document);
                        } else {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&text.source).monospace())
                                    .selectable(true)
                                    .wrap(),
                            );
                        }
                    });
            } else if let Some(link) = collection.link.as_ref() {
                let inner = canvas.shrink(18.0);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("collection-link-content")
                        .max_rect(inner),
                );
                child.set_clip_rect(inner);
                egui::ScrollArea::vertical()
                    .id_salt(("collection-link-scroll", collection.ready_index))
                    .max_height(inner.height())
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        ui.set_width(inner.width());
                        if let Some(texture) = collection.texture.as_ref() {
                            let original = texture.size_vec2();
                            let scale = (inner.width() / original.x)
                                .min(180.0 / original.y)
                                .min(1.0);
                            ui.add(egui::Image::new((texture.id(), original * scale)));
                            ui.add_space(10.0);
                        }
                        if !link.title.is_empty() {
                            ui.heading(&link.title);
                        }
                        if let Ok(url) = url::Url::parse(&link.url)
                            && let Some(host) = url.host_str()
                        {
                            ui.weak(host);
                        }
                        ui.add(egui::Label::new(&link.url).selectable(true).wrap());
                        if !link.description.is_empty() {
                            ui.add_space(10.0);
                            ui.add(egui::Label::new(&link.description).selectable(true).wrap());
                        }
                    });
            } else if let Some(texture) = collection.texture.as_ref() {
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
                    if fullscreen && show_chrome != (collection.zoom.factor <= 1.0) {
                        ctx.request_repaint();
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
                if let Ok(geometry) = collection.zoom.geometry(viewport, size)
                    && let (Some(left), Some(top), Some(width), Some(height)) = (
                        geometry["left"].as_f64(),
                        geometry["top"].as_f64(),
                        geometry["width"].as_f64(),
                        geometry["height"].as_f64(),
                    )
                {
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
            // Prepare the page during the entrance. Start the decoder after
            // the dialog reaches its final position.
            if !entering
                && let Some(target) = media_target.as_ref()
                && let Some(api) = self.api.as_mut()
            {
                self.media_preview
                    .show_in_canvas(ui, canvas, target, &mut |command| {
                        api.execute(Request {
                            api_version: 1,
                            request_id: "collection-media-ui".into(),
                            command,
                        })
                    });
                if self.media_preview.frame_texture_for(target).is_some() {
                    collection.transition_poster = None;
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
        if cancel_rename {
            self.upscale.link_rename = None;
        } else if save_name {
            self.submit_link_rename(ctx);
        } else if self.upscale.link_rename.is_some() {
            // The inline editor owns the preview until it closes.
        } else if close || (accepts_input && !fullscreen && modal.should_close()) {
            self.close_translation_collection(ctx);
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
            TraditionalChinese => (
                Command::TranslateSelectedImageFrom { source },
                tr(
                    "Перевод изображения с традиционного китайского",
                    "Image translation from Traditional Chinese",
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
        self.close_translation_collection(ctx);
        let result = self.call(command);
        let job = result["job_id"].as_u64().map(katran::domain_jobs::JobId);
        self.upscale = State {
            open: true,
            job,
            preview_job: None,
            collection: None,
            collection_title: String::new(),
            collection_is_bull: false,
            collection_fullscreen: false,
            link_rename: None,
            progress: None,
            partial_translation: None,
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
                        ImageJob::Upscale if result["collection"] == true => {
                            upscale_collection_status(result)
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
                    if matches!(self.upscale.kind, ImageJob::Upscale)
                        && result["collection"] == true
                    {
                        self.upscale.open = false;
                        self.message = self.upscale.status.clone();
                        let event = &result["event"];
                        if event["id"].is_i64() && event["revision"].is_string() {
                            self.open_translation_collection(event, ctx);
                            self.message = self.upscale.status.clone();
                        } else {
                            self.message.push_str(" · ");
                            self.message.push_str(tr(
                                "Не удалось открыть коллекцию",
                                "Could not open collection",
                            ));
                        }
                    } else if matches!(self.upscale.kind, ImageJob::Translate)
                        && result["collection"] == true
                        && translation_is_partial(result)
                    {
                        self.upscale.status = partial_translation_status(result);
                        self.upscale.partial_translation = Some(PartialTranslation {
                            event: result["event"].clone(),
                            failures: partial_translation_failures(result),
                        });
                        self.upscale.open = true;
                    } else if matches!(self.upscale.kind, ImageJob::Translate)
                        && result["collection"] == true
                    {
                        self.upscale.open = false;
                        let event = &result["event"];
                        if event["id"].is_i64() && event["revision"].is_string() {
                            let total = result["total"].as_u64().unwrap_or(0);
                            let translated = result["translated"].as_u64().unwrap_or(0);
                            self.message = format!(
                                "{} {translated}/{total}",
                                tr("Сохранено страниц:", "Pages saved:")
                            );
                            self.open_translation_collection(event, ctx);
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
        let mut open_partial = false;
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
                if let Some(partial) = self.upscale.partial_translation.as_ref() {
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            for failure in &partial.failures {
                                ui.label(failure);
                            }
                        });
                    let valid =
                        partial.event["id"].is_i64() && partial.event["revision"].is_string();
                    open_partial = ui
                        .add_enabled(valid, egui::Button::new(tr("Открыть BULL", "Open BULL")))
                        .clicked();
                    if !valid {
                        ui.weak(tr(
                            "Не удалось открыть коллекцию",
                            "Could not open collection",
                        ));
                    }
                }
                close = ui.button(tr("Закрыть", "Close")).clicked();
            }
        });
        if cancel {
            if let Some(job_id) = self.upscale.job {
                self.call(Command::CancelJob { job_id });
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if open_partial {
            let event = self
                .upscale
                .partial_translation
                .as_ref()
                .map(|partial| partial.event.clone());
            self.upscale.open = false;
            if let Some(event) = event {
                self.open_translation_collection(&event, ctx);
            }
        } else if self.upscale.job.is_none() && (close || modal.should_close()) {
            self.upscale.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katran::actions::upscale_operation;

    #[test]
    fn bull_fullscreen_is_contextual_and_hides_pager_when_zoomed() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 650.0));
        let frame = |app: &mut Shelf, key: Option<(egui::Key, egui::Modifiers)>| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: key
                        .into_iter()
                        .flat_map(|(key, modifiers)| {
                            [true, false].map(|pressed| egui::Event::Key {
                                key,
                                modifiers,
                                physical_key: None,
                                pressed,
                                repeat: false,
                            })
                        })
                        .collect(),
                    ..Default::default()
                },
                |ui| app.render(ui),
            )
        };
        let sent = |output: egui::FullOutput, value| {
            output.viewport_output.get(&egui::ViewportId::ROOT).unwrap().commands.iter()
                .any(|command| matches!(command, egui::ViewportCommand::Fullscreen(v) if *v == value))
        };
        assert!(!sent(
            frame(&mut app, Some((egui::Key::F11, egui::Modifiers::NONE))),
            true
        ));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(20, 30)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut bytes = Vec::new();
        baloh_collection::codec::write(&mut bytes, &[png.into_inner()]).unwrap();
        let event = app.call(Command::IngestBytes {
            name: "Fullscreen.bull".into(),
            source: "image_collection".into(),
            bytes,
        });
        app.open_translation_collection(&event, &ctx);
        assert!(sent(
            frame(&mut app, Some((egui::Key::F11, egui::Modifiers::NONE))),
            true
        ));
        let canvas = || {
            ctx.data(|data| {
                data.get_temp::<egui::Rect>(egui::Id::new("collection-test-canvas"))
                    .unwrap()
            })
        };
        assert_eq!(canvas().width(), screen.width());
        assert!(canvas().top() > screen.top());
        // The asynchronous first-page response normally resets zoom. Let it
        // settle before checking the chrome state in a subsequent frame.
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.upscale.collection.as_ref().unwrap().pending.is_some() {
            assert!(Instant::now() < deadline);
            frame(&mut app, None);
            std::thread::sleep(Duration::from_millis(5));
        }
        app.upscale.collection.as_mut().unwrap().zoom.factor = 2.0;
        frame(&mut app, None);
        assert_eq!(canvas(), screen);
        app.upscale.collection.as_mut().unwrap().zoom.factor = 1.0;
        frame(&mut app, None);
        assert!(canvas().top() > screen.top());
        assert!(sent(
            frame(&mut app, Some((egui::Key::Escape, egui::Modifiers::NONE))),
            false
        ));
        assert!(app.upscale.collection_open());

        frame(&mut app, Some((egui::Key::K, egui::Modifiers::CTRL)));
        assert!(app.actions);
        app.action_query = "fullscreen".into();
        assert!(sent(
            frame(&mut app, Some((egui::Key::Enter, egui::Modifiers::NONE))),
            true
        ));
        assert!(!app.actions);
        assert!(sent(
            frame(&mut app, Some((egui::Key::Space, egui::Modifiers::NONE))),
            false
        ));
        assert!(!app.upscale.collection_open());
    }

    #[test]
    fn collection_rename_refreshes_the_open_link_and_revision() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        let mut bytes = Vec::new();
        baloh_collection::links::write(
            &mut bytes,
            &[baloh_collection::links::Link {
                url: "http://example.test/synthetic".into(),
                title: "Before".into(),
                description: "Keep this description".into(),
                preview_png: None,
            }],
        )
        .unwrap();
        let event = app.call(Command::IngestBytes {
            name: "test.bull".into(),
            source: "link_collection".into(),
            bytes,
        });
        let revision = event["revision"].as_str().unwrap();
        app.open_translation_collection(&event, &ctx);
        assert_eq!(app.upscale.collection_title, "test.bull");
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while app.upscale.collection.as_ref().unwrap().pending.is_some() {
            assert!(std::time::Instant::now() < deadline);
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                app.translation_collection_ui(ui.ctx())
            });
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            app.upscale
                .collection
                .as_ref()
                .unwrap()
                .link
                .as_ref()
                .unwrap()
                .title,
            "Before"
        );
        app.upscale.link_rename = Some(LinkRename {
            title: "After".into(),
            focus: false,
            job: None,
            error: String::new(),
        });
        app.submit_link_rename(&ctx);
        assert!(app.upscale.saving_link_name());
        while app.upscale.saving_link_name() {
            assert!(std::time::Instant::now() < deadline);
            app.poll_link_rename(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.upscale.link_rename.is_none());
        let collection = app.upscale.collection.as_ref().unwrap();
        assert_eq!(collection.ready_index, Some(0));
        assert_ne!(collection.revision, revision);
        let link = collection.link.as_ref().unwrap();
        assert_eq!(link.title, "After");
        assert_eq!(link.description, "Keep this description");
    }

    #[test]
    fn metadata_revision_keeps_visible_link_and_pending_target() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        app.upscale.collection = Some(Collection {
            id: 7,
            revision: "old".into(),
            index: 2,
            count: Some(3),
            pending: None,
            texture: None,
            transition_poster: None,
            opened_at: Instant::now(),
            link: Some(LinkPage {
                url: "https://example.org".into(),
                title: "old title".into(),
                description: "".into(),
                editable: true,
            }),
            text: None,
            file: None,
            ready_index: Some(1),
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        });
        app.accept_link_collection_revision(7, "new", &ctx);
        let collection = app.upscale.collection.as_ref().unwrap();
        assert_eq!(collection.revision, "new");
        assert_eq!(collection.index, 2);
        assert_eq!(collection.ready_index, Some(1));
        assert_eq!(collection.link.as_ref().unwrap().title, "old title");
    }

    #[test]
    fn partial_translation_reports_original_one_based_numbers() {
        let result = serde_json::json!({"total":3,"translated":2,"pages":[
            {"index":0,"ok":true,"page":0},
            {"index":1,"ok":false,"error":{"message":"OCR failed"},"page":1,"preserved_original":true},
            {"index":2,"ok":true,"page":2}
        ]});
        assert!(translation_is_partial(&result));
        let failures = partial_translation_failures(&result);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("2"));
        assert!(failures[0].contains("OCR failed"));
        assert!(!translation_is_partial(
            &serde_json::json!({"total":1,"translated":1,"pages":[{"index":0,"ok":true}]})
        ));
    }

    #[test]
    fn batch_skipped_regions_are_reported_separately_from_failed_pages() {
        let result = serde_json::json!({"total":4,"translated":3,"pages":[
            {"index":0,"ok":true,"page":0,"skipped_blocks":0},
            {"index":1,"ok":true,"page":1,"skipped_blocks":5},
            {"index":2,"ok":false,"page":2,"preserved_original":true,"error":{"message":"OCR failed"}},
            {"index":3,"ok":true,"page":3,"skipped_blocks":0}
        ]});
        assert!(translation_is_partial(&result));
        let lines = partial_translation_failures(&result);
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].contains("2") && lines[0].contains("5") && !lines[0].contains("OCR failed")
        );
        assert!(lines[1].contains("3") && lines[1].contains("OCR failed"));
        let status = partial_translation_status(&result);
        assert!(status.contains("4/4") && status.contains("3/4"));
        assert!(translation_is_partial(
            &serde_json::json!({"total":1,"translated":1,"pages":[{"index":0,"page":0,"ok":true,"skipped_blocks":1}]})
        ));
        assert!(!translation_is_partial(
            &serde_json::json!({"total":1,"translated":1,"pages":[{"index":0,"page":0,"ok":true,"skipped_blocks":0}]})
        ));
    }

    #[test]
    fn upscale_collection_status_reports_count_and_destination() {
        let result = serde_json::json!({"collection":true,"total":3,"upscaled":2});
        let status = upscale_collection_status(&result);
        assert!(status.contains("2/3"));
        assert!(status.contains("Images"));
    }

    #[test]
    fn collection_intro_is_bounded_and_finishes_at_identity() {
        let center = egui::pos2(500.0, 400.0);
        let start = collection_intro_transform(Duration::ZERO, center);
        assert!(start.scaling > 0.95 && start.scaling < 1.0);
        assert!((start * center).y > center.y);
        assert_eq!(
            collection_intro_transform(Duration::from_millis(300), center),
            egui::emath::TSTransform::IDENTITY
        );
        assert_eq!(
            collection_intro_transform(Duration::from_secs(1), center),
            egui::emath::TSTransform::IDENTITY
        );
    }

    #[test]
    fn leaving_ready_video_retains_its_poster_when_next_page_fails() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "video-poster-fixture",
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
            transition_poster: None,
            opened_at: Instant::now(),
            link: None,
            text: None,
            file: Some(FilePage {
                name: "clip.mp4".into(),
                size: 1,
                extension: "mp4".into(),
                video: true,
            }),
            ready_index: Some(0),
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        });
        app.request_translation_page(1, &ctx);
        let collection = app.upscale.collection.as_ref().unwrap();
        assert_eq!(collection.ready_index, Some(0));
        assert_eq!(collection.index, 1);
        assert!(collection.file.as_ref().unwrap().video);
        assert_eq!(
            collection.transition_poster.as_ref().unwrap().id(),
            texture_id
        );
        assert!(app.collection_media_target().is_none());

        fn contains_filename(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Text(text) => text.galley.job.text.contains("clip.mp4"),
                egui::Shape::Vec(children) => children.iter().any(contains_filename),
                _ => false,
            }
        }
        for _ in 0..2 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| app.translation_collection_ui(ui.ctx()),
            );
            assert!(
                !output
                    .shapes
                    .iter()
                    .any(|shape| contains_filename(&shape.shape))
            );
        }
    }

    #[test]
    fn collection_video_target_requires_current_ready_page() {
        let mut collection = Collection {
            id: 12,
            revision: "rev".into(),
            index: 0,
            count: Some(2),
            pending: None,
            texture: None,
            transition_poster: None,
            opened_at: Instant::now(),
            link: None,
            text: None,
            file: Some(FilePage {
                name: "clip.mp4".into(),
                size: 1,
                extension: "mp4".into(),
                video: true,
            }),
            ready_index: Some(0),
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        };
        assert!(collection.video_ready());
        collection.index = 1;
        assert!(!collection.video_ready());
        collection.ready_index = Some(1);
        collection.file.as_mut().unwrap().video = false;
        assert!(!collection.video_ready());
    }

    #[test]
    fn bs_ready_entry_can_open_but_cannot_copy_or_paste() {
        let collection = Collection {
            id: 12,
            revision: "rev".into(),
            index: 2,
            count: Some(3),
            pending: Some((katran::domain_jobs::JobId(77), 2)),
            texture: None,
            transition_poster: None,
            opened_at: Instant::now(),
            link: None,
            text: None,
            file: Some(FilePage {
                name: "report.pdf".into(),
                size: 123,
                extension: "pdf".into(),
                video: false,
            }),
            ready_index: Some(1),
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        };
        assert_eq!(collection.ready_target(), Some((12, "rev".into(), 1)));
        assert_eq!(collection.ready_transfer_target(), None);
    }

    #[test]
    fn link_target_uses_visible_page_while_next_is_pending() {
        let collection = Collection {
            id: 7,
            revision: "rev".into(),
            index: 2,
            count: Some(3),
            pending: Some((katran::domain_jobs::JobId(42), 2)),
            texture: None,
            transition_poster: None,
            opened_at: Instant::now(),
            text: None,
            file: None,
            link: Some(LinkPage {
                url: "https://example.org".into(),
                title: "title".into(),
                description: "description".into(),
                editable: true,
            }),
            ready_index: Some(1),
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        };
        assert_eq!(collection.ready_target(), Some((7, "rev".into(), 1)));
    }

    #[test]
    fn text_page_target_keeps_last_ready_page() {
        let source = "# Example
**bold**";
        let collection = Collection {
            id: 11,
            revision: "rev".into(),
            index: 3,
            count: Some(4),
            pending: Some((katran::domain_jobs::JobId(9), 3)),
            texture: None,
            transition_poster: None,
            opened_at: Instant::now(),
            link: None,
            file: None,
            text: Some(TextPage {
                source: source.into(),
                document: katran::markdown::parse(source).ok(),
                rendered: true,
            }),
            ready_index: Some(2),
            copy_held: false,
            zoom: katran::preview_view::State::default(),
            error: String::new(),
        };
        assert_eq!(collection.ready_target(), Some((11, "rev".into(), 2)));
        assert!(collection.text.as_ref().unwrap().document.is_some());
    }

    #[test]
    fn collection_enter_consumes_plain_press_and_not_alt_or_repeat() {
        let key = |repeat, modifiers| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        };
        let mut input = vec![
            key(false, egui::Modifiers::NONE),
            key(false, egui::Modifiers::ALT),
        ];
        assert!(take_collection_enter(&mut input, true));
        assert_eq!(input.len(), 1);
        let mut repeat = vec![key(true, egui::Modifiers::NONE)];
        assert!(!take_collection_enter(&mut repeat, true));
        assert!(repeat.is_empty());
        let mut unfocused = vec![key(false, egui::Modifiers::NONE)];
        assert!(!take_collection_enter(&mut unfocused, false));
        assert!(unfocused.is_empty());
    }

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
            transition_poster: None,
            opened_at: Instant::now() - Duration::from_millis(300),
            link: None,
            text: None,
            file: None,
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
            transition_poster: None,
            opened_at: Instant::now(),
            link: None,
            text: None,
            file: None,
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
        assert_eq!(
            translation_source("translate_image_zh_hant"),
            Some(TraditionalChinese)
        );
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

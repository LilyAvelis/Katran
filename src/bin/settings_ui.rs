//! Afio's settings shell and shared section/row components; API owns preferences.
use super::ui_foundation::{SettingsRow, SettingsSection};
use eframe::egui::{self, Color32, RichText};
use katran::{Command, Request, Service, interaction::View, settings::Settings};

#[derive(Default)]
pub struct State {
    picker: Option<std::sync::mpsc::Receiver<super::theme_file_picker::PickResult>>,
    background_job: Option<katran::domain_jobs::JobId>,
    video_picker: Option<std::sync::mpsc::Receiver<super::theme_file_picker::PickResult>>,
    video_job: Option<katran::domain_jobs::JobId>,
    video_backgrounds: Option<serde_json::Value>,
    media_runtime: Option<serde_json::Value>,
    media_job: Option<katran::domain_jobs::JobId>,
    font_picker: Option<std::sync::mpsc::Receiver<super::theme_file_picker::PickResult>>,
    font_job: Option<katran::domain_jobs::JobId>,
    emoji_fonts: Option<serde_json::Value>,
    custom_backgrounds: Option<serde_json::Value>,
    restart_confirmation: bool,
    restart_requested: bool,
    updates: super::updates_ui::State,
    tags: super::tag_settings_ui::State,
    confirm_history: bool,
    query: String,
    section: usize,
    requested_section: Option<usize>,
    draft: Option<Settings>,
    error: String,
    editor: Option<super::shortcuts_ui::Editor>,
    overlay_status: String,
    overlay_retry: bool,
    shortcut_filter: String,
    sharex_folder: String,
    downloads_folder: String,
    refresh_links_requested: bool,
}
fn apply_editor_intent(
    api: &mut Service,
    editor: &mut super::shortcuts_ui::Editor,
    intent: super::shortcuts_ui::Intent,
    draft: &mut Option<Settings>,
) -> bool {
    if editor.id == "overlay_hotkey" {
        apply_global_hotkey(api, editor, intent, draft)
    } else {
        super::shortcuts_ui::apply(api, editor, intent)
    }
}
fn apply_global_hotkey(
    api: &mut Service,
    editor: &mut super::shortcuts_ui::Editor,
    intent: super::shortcuts_ui::Intent,
    draft: &mut Option<Settings>,
) -> bool {
    use super::shortcuts_ui::Intent;
    let Intent::Set { binding, .. } = intent else {
        return true;
    };
    if !binding.is_empty()
        && let Err(error) = katran::overlay::Hotkey::parse(&binding)
    {
        let message = match error.message.as_str() {
            "Use Ctrl, Alt, Shift and one letter or F key" => super::i18n::tr(
                "Укажите Ctrl, Alt, Shift и одну букву или клавишу F",
                "Use Ctrl, Alt, Shift and one letter or F key",
            ),
            "Duplicate or invalid hotkey component" => super::i18n::tr(
                "Повторный или недопустимый компонент сочетания",
                "Duplicate or invalid hotkey component",
            ),
            "Hotkey needs a modifier and a key" => super::i18n::tr(
                "В сочетании нужны модификатор и клавиша",
                "Hotkey needs a modifier and a key",
            ),
            _ => error.message.as_str(),
        };
        editor.failed(binding, message.to_owned(), false);
        return false;
    }
    let current = call(api, Command::GetSettings);
    let result = if current["ok"] == true {
        match serde_json::from_value::<Settings>(current["result"].clone()) {
            Ok(mut current) => {
                current.overlay_hotkey = binding.clone();
                call(api, Command::SetSettings { settings: current })
            }
            Err(_) => {
                serde_json::json!({"ok":false,"error":{"message":super::i18n::tr("Не удалось прочитать текущие настройки", "Could not read current settings")}})
            }
        }
    } else {
        current
    };
    if result["ok"] == true {
        if let Some(draft) = draft.as_mut() {
            draft.overlay_hotkey = binding;
        }
        true
    } else {
        editor.failed(
            binding,
            result["error"]["message"]
                .as_str()
                .unwrap_or(super::i18n::tr("Ошибка сохранения", "Save failed"))
                .into(),
            false,
        );
        false
    }
}
/// Search routes to the owning page, including settings inside named groups.
fn section_matches_query(index: usize, label: &str, query: &str) -> bool {
    let terms = match index {
        0 => {
            "general language язык запуск startup controls управление вызов katran захват capture поиск search сортировка ссылки значки favicon обновление плитки pods links галерея поток столбцы строки шрифт эмодзи папка sharex downloads кэш quota квота история folder"
        }
        1 => "shortcuts команда клавиатура сочетание shortcut hotkey",
        2 => "tabs доступные вкладки включить выключить сбор данных",
        3 => "updates версия установка обновить",
        4 => {
            "themes тема цвета акцент выделение интенсивность окно compact компактное стекло оптика gpu преломление размытие фон background лилии изображение видео components компоненты сглаживать стык декор линии прокрутка scrollbar полоса"
        }
        5 => "tags метки теги tag",
        _ => "",
    };
    let query = query.trim().to_lowercase();
    query
        .split_whitespace()
        .all(|word| label.to_lowercase().contains(word) || terms.contains(word))
}

fn localized_shortcut_label<'a>(id: &str, russian: &'a str) -> &'a str {
    if super::i18n::is_english() {
        let direct = match id {
            "pods" => "Tab: Pods",
            "files" => "Tab: Files",
            "tables" => "Tab: Tables",
            "text" => "Tab: Text",
            "links" => "Tab: Links",
            "downloads" => "Tab: Downloads",
            "all" => "Tab: Folder",
            "images" => "Tab: Images",
            "screenshots" => "Tab: Screenshots",
            "tab_toggle" => "Folder / previous tab",
            "toggle_pinned_view" => "All / pinned in category",
            "capture" => "Toggle capture",
            "commands" => "Open commands",
            _ => "",
        };
        if !direct.is_empty() {
            return direct;
        }
        let english = katran::actions::english_label(id, "");
        if english.is_empty() { russian } else { english }
    } else {
        russian
    }
}

fn localized_tab_label(label: &str) -> &str {
    match label {
        "Folder" => super::i18n::tr("Folder", "Folder"),
        "Images" => super::i18n::tr("Изображения", "Images"),
        "Screenshots" => super::i18n::tr("Снимки экрана", "Screenshots"),
        "Downloads" => super::i18n::tr("Загрузки", "Downloads"),
        "Text" => super::i18n::tr("Текст", "Text"),
        "Links" => super::i18n::tr("Ссылки", "Links"),
        "Tables" => super::i18n::tr("Таблицы", "Tables"),
        "Pods" => super::i18n::tr("Символы", "Pods"),
        "Files" => super::i18n::tr("Файлы", "Files"),
        other => other,
    }
}

fn localized_shelf_tab(tab: katran::interaction::ShelfTab) -> &'static str {
    use katran::interaction::ShelfTab;
    match tab {
        ShelfTab::All => super::i18n::tr("Folder", "Folder"),
        ShelfTab::Images => super::i18n::tr("Изображения", "Images"),
        ShelfTab::Screenshots => super::i18n::tr("Снимки экрана", "Screenshots"),
        ShelfTab::Downloads => super::i18n::tr("Загрузки", "Downloads"),
        ShelfTab::Text => super::i18n::tr("Текст", "Text"),
        ShelfTab::Links => super::i18n::tr("Ссылки", "Links"),
        ShelfTab::Tables => super::i18n::tr("Таблицы", "Tables"),
        ShelfTab::Pods => super::i18n::tr("Символы", "Pods"),
        ShelfTab::Files => super::i18n::tr("Файлы", "Files"),
    }
}

fn selection_settings(ui: &mut egui::Ui, theme: &mut katran::theme::ThemePreferences) -> bool {
    let mut changed = false;
    SettingsRow::show(
        ui,
        super::i18n::tr("Цвет выделения", "Selection color"),
        super::i18n::tr(
            "Отдельный цвет выбранных карточек. По умолчанию — цвет темы.",
            "Custom color for selected cards. Defaults to the theme color.",
        ),
        |ui| {
            let fallback = if theme.preset == katran::theme::Preset::BlueExperimental {
                theme.accent.unwrap_or([107, 174, 255])
            } else {
                let c = ui.visuals().selection.stroke.color;
                [c.r(), c.g(), c.b()]
            };
            let mut color = theme.selection.unwrap_or(fallback);
            ui.horizontal_wrapped(|ui| {
                if ui.color_edit_button_srgb(&mut color).changed() {
                    theme.selection = Some(color);
                    changed = true;
                }
                if ui
                    .button(super::i18n::tr("Цвет темы", "Theme color"))
                    .on_hover_text(super::i18n::tr(
                        "Сбросить собственный цвет выделения",
                        "Reset custom selection color",
                    ))
                    .clicked()
                {
                    theme.selection = None;
                    changed = true;
                }
            });
        },
    );
    SettingsRow::show(
        ui,
        super::i18n::tr("Интенсивность выделения", "Selection intensity"),
        super::i18n::tr(
            "Оттенок заливки и света в оптике. При 0 остаётся контур.",
            "Controls fill and optical lighting. At 0, only the outline remains.",
        ),
        |ui| {
            let available = ui.available_width();
            ui.spacing_mut().slider_width = (available - 64.0).clamp(80.0, 180.0);
            changed |= ui
                .add(egui::Slider::new(&mut theme.selection_intensity, 0..=100).suffix(" %"))
                .changed();
        },
    );
    changed
}

#[cfg(test)]
mod selection_layout_tests {
    use super::*;
    #[test]
    fn selection_controls_stay_inside_narrow_and_scaled_settings() {
        for width in [280.0, 580.0, 900.0] {
            for scale in [1.0, 1.5, 2.0] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                let mut theme = katran::theme::ThemePreferences::default();
                let mut area = egui::Rect::NOTHING;
                let mut boundary = egui::Rect::NOTHING;
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    egui::vec2(width, 700.0),
                                ))
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| {
                                ui.set_width(width);
                                ui.set_max_width(width);
                                boundary = ui.max_rect();
                                selection_settings(ui, &mut theme);
                                area = ui.min_rect();
                            },
                        );
                    },
                );
                out.textures_delta.clear();
                assert!(
                    area.right() <= boundary.right() + 1.0,
                    "width={width}, scale={scale}: {area:?} exceeds {boundary:?}"
                );
            }
        }
    }
}
fn optical_settings(ui: &mut egui::Ui, theme: &mut katran::theme::ThemePreferences) -> bool {
    let mut changed = false;
    let mut enabled = theme.optical_strength > 0;
    SettingsRow::show(
        ui,
        super::i18n::tr("Оптика элементов · GPU", "Element optics · GPU"),
        super::i18n::tr(
            "Преломление фона под карточками и меню. Нужны лилии или собственный фон.",
            "Refract the background beneath cards and menus. Requires a lily or custom background.",
        ),
        |ui| {
            if ui
                .checkbox(&mut enabled, super::i18n::tr("Включить", "Enable"))
                .changed()
            {
                theme.optical_strength = if enabled { 65 } else { 0 };
                changed = true;
            }
        },
    );
    if enabled {
        for (title, description, value, max) in [
            (
                super::i18n::tr("Сила эффекта", "Effect strength"),
                super::i18n::tr(
                    "Общая выраженность оптики.",
                    "Overall strength of the optical effect.",
                ),
                &mut theme.optical_strength,
                100,
            ),
            (
                super::i18n::tr("Преломление", "Refraction"),
                super::i18n::tr(
                    "Искажение фона у краёв элемента.",
                    "Background distortion at the element's edges.",
                ),
                &mut theme.optical_refraction,
                200,
            ),
            (
                super::i18n::tr("Размытие", "Blur"),
                super::i18n::tr(
                    "Мягкость фона внутри стекла.",
                    "Softness of the background inside the glass.",
                ),
                &mut theme.optical_blur,
                200,
            ),
            (
                super::i18n::tr("Краевые блики", "Edge highlights"),
                super::i18n::tr(
                    "Яркость направленного света на границах.",
                    "Brightness of directional light at the edges.",
                ),
                &mut theme.optical_highlight,
                200,
            ),
            (
                super::i18n::tr("Разделение цветов", "Chromatic separation"),
                super::i18n::tr(
                    "Цветовые края преломления.",
                    "Color fringing at refracted edges.",
                ),
                &mut theme.optical_chromatic,
                200,
            ),
        ] {
            SettingsRow::show(ui, title, description, |ui| {
                changed |= ui
                    .add(
                        egui::Slider::new(value, if max == 100 { 1 } else { 0 }..=max).suffix(" %"),
                    )
                    .changed();
            });
        }
    }
    changed
}
fn call(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "settings-ui".into(),
        command,
    })
}
fn merge_tab_settings(draft: &mut Settings, current: &Settings) {
    draft.tab_order = current.tab_order.clone();
    draft.hidden_tabs = current.hidden_tabs.clone();
    draft.downloads_enabled = current.downloads_enabled;
    draft.text_enabled = current.text_enabled;
    draft.links_enabled = current.links_enabled;
    draft.tables_enabled = current.tables_enabled;
    draft.pods_enabled = current.pods_enabled;
    draft.theme = current.theme.clone();
    draft.sort = current.sort;
}
fn apply_theme(
    api: &mut Service,
    draft: &mut Option<Settings>,
    theme: katran::theme::ThemePreferences,
) -> Result<(), String> {
    let response = call(api, Command::SetTheme { theme });
    if response["ok"] != true {
        return Err(response["error"]["message"]
            .as_str()
            .unwrap_or(super::i18n::tr(
                "Не удалось применить тему",
                "Could not apply theme",
            ))
            .into());
    }
    let current = serde_json::from_value(response["result"]["current"].clone()).map_err(|_| {
        super::i18n::tr(
            "Не удалось прочитать применённую тему",
            "Could not read applied theme",
        )
        .to_string()
    })?;
    if let Some(draft) = draft.as_mut() {
        draft.theme = current;
    }
    Ok(())
}
impl State {
    pub(super) fn background_busy(&self) -> bool {
        self.picker.is_some()
            || self.background_job.is_some()
            || self.video_picker.is_some()
            || self.video_job.is_some()
            || self.media_job.is_some()
            || self.font_picker.is_some()
            || self.font_job.is_some()
    }
    pub(super) fn take_restart_requested(&mut self) -> bool {
        std::mem::take(&mut self.restart_requested)
    }
    pub(super) fn take_link_refresh_requested(&mut self) -> bool {
        std::mem::take(&mut self.refresh_links_requested)
    }

    pub fn set_overlay_status(&mut self, status: String) {
        self.overlay_status = status;
    }
    pub fn take_overlay_retry(&mut self) -> bool {
        std::mem::take(&mut self.overlay_retry)
    }
    pub fn is_recording_overlay(&self) -> bool {
        self.editor.is_some()
    }
    fn tick_background(&mut self, api: &mut Service, ctx: &egui::Context) {
        if let Some(picker) = &self.picker {
            match picker.try_recv() {
                Ok(result) => {
                    self.picker = None;
                    match result {
                        Ok(Some(path)) => {
                            let response = call(
                                api,
                                Command::BeginJob {
                                    command: Box::new(Command::ImportThemeBackground { path }),
                                },
                            );
                            self.background_job = response["result"]["job_id"]
                                .as_u64()
                                .map(katran::domain_jobs::JobId);
                            if response["ok"] != true {
                                self.error = response["error"]["message"]
                                    .as_str()
                                    .unwrap_or(super::i18n::tr(
                                        "Не удалось импортировать фон",
                                        "Could not import background",
                                    ))
                                    .into();
                            }
                        }
                        Ok(None) => {}
                        Err(error) => self.error = error,
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50))
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.picker = None;
                    self.error = super::i18n::tr(
                        "Диалог выбора изображения завершился без результата",
                        "The image picker closed without a result",
                    )
                    .into();
                }
            }
        }
        if let Some(job_id) = self.background_job {
            let response = call(api, Command::PollJob { job_id });
            let status = &response["result"];
            match status["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50))
                }
                Some("succeeded") => {
                    self.background_job = None;
                    ctx.request_repaint();
                    if let Some(id) = status["result"]["id"].as_str() {
                        let current = call(api, Command::Themes);
                        if let Ok(mut theme) =
                            serde_json::from_value::<katran::theme::ThemePreferences>(
                                current["result"]["current"].clone(),
                            )
                        {
                            theme.custom_background = Some(id.to_owned());
                            theme.lily_background = None;
                            theme.video_background = None;
                            theme.video_enabled = Some(false);
                            self.error = apply_theme(api, &mut self.draft, theme)
                                .err()
                                .unwrap_or_default();
                            self.custom_backgrounds = None;
                        }
                    }
                }
                _ => {
                    self.background_job = None;
                    self.error = status["error"]["message"]
                        .as_str()
                        .or_else(|| response["error"]["message"].as_str())
                        .unwrap_or(super::i18n::tr(
                            "Импорт фона отменён или не выполнен",
                            "Background import was canceled or failed",
                        ))
                        .into();
                }
            }
        }
    }
    fn tick_video(&mut self, api: &mut Service, ctx: &egui::Context) {
        if let Some(picker) = &self.video_picker {
            match picker.try_recv() {
                Ok(result) => {
                    self.video_picker = None;
                    match result {
                        Ok(Some(path)) => {
                            let response = call(
                                api,
                                Command::BeginJob {
                                    command: Box::new(Command::ImportVideoBackground { path }),
                                },
                            );
                            self.video_job = response["result"]["job_id"]
                                .as_u64()
                                .map(katran::domain_jobs::JobId);
                            if response["ok"] != true || self.video_job.is_none() {
                                self.error = response["error"]["message"]
                                    .as_str()
                                    .unwrap_or(super::i18n::tr(
                                        "Не удалось начать импорт видео",
                                        "Could not start video import",
                                    ))
                                    .into();
                            } else {
                                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                            }
                        }
                        Ok(None) => {}
                        Err(error) => self.error = error,
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50))
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.video_picker = None;
                    self.error = super::i18n::tr(
                        "Диалог выбора видео завершился без результата",
                        "The video picker closed without a result",
                    )
                    .into();
                }
            }
        }
        if let Some(job_id) = self.video_job {
            let response = call(api, Command::PollJob { job_id });
            let status = &response["result"];
            match status["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50))
                }
                Some("succeeded") => {
                    self.video_job = None;
                    if let Some(id) = status["result"]["id"].as_str() {
                        let current = call(api, Command::Themes);
                        if let Ok(mut theme) =
                            serde_json::from_value::<katran::theme::ThemePreferences>(
                                current["result"]["current"].clone(),
                            )
                        {
                            theme.video_background = Some(id.to_owned());
                            theme.video_enabled = Some(true);
                            theme.lily_background = None;
                            theme.custom_background = None;
                            self.error = apply_theme(api, &mut self.draft, theme)
                                .err()
                                .unwrap_or_default();
                            self.video_backgrounds = None;
                        } else {
                            self.error = super::i18n::tr(
                                "Не удалось прочитать текущую тему",
                                "Could not read current theme",
                            )
                            .into();
                        }
                    } else {
                        self.error = super::i18n::tr(
                            "Импорт видео не вернул идентификатор",
                            "Video import returned no identifier",
                        )
                        .into();
                    }
                    ctx.request_repaint();
                }
                _ => {
                    self.video_job = None;
                    self.error = status["error"]["message"]
                        .as_str()
                        .or_else(|| response["error"]["message"].as_str())
                        .unwrap_or(super::i18n::tr(
                            "Импорт видео отменён или не выполнен",
                            "Video import was canceled or failed",
                        ))
                        .into();
                }
            }
        }
    }
    fn tick_media(&mut self, api: &mut Service, ctx: &egui::Context) {
        if let Some(job_id) = self.media_job {
            let response = call(api, Command::PollJob { job_id });
            let status = &response["result"];
            match status["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                }
                Some("succeeded") => {
                    self.media_job = None;
                    self.media_runtime = None;
                    self.error.clear();
                    ctx.request_repaint();
                }
                _ => {
                    self.media_job = None;
                    self.error = status["error"]["message"]
                        .as_str()
                        .or_else(|| response["error"]["message"].as_str())
                        .unwrap_or(super::i18n::tr(
                            "Не удалось подготовить компоненты видео",
                            "Could not prepare video components",
                        ))
                        .into();
                    ctx.request_repaint();
                }
            }
        }
    }
    fn tick_font(&mut self, api: &mut Service, ctx: &egui::Context) {
        if let Some(picker) = &self.font_picker {
            match picker.try_recv() {
                Ok(result) => {
                    self.font_picker = None;
                    match result {
                        Ok(Some(path)) => {
                            let response = call(
                                api,
                                Command::BeginJob {
                                    command: Box::new(Command::ImportEmojiFont { path }),
                                },
                            );
                            self.font_job = response["result"]["job_id"]
                                .as_u64()
                                .map(katran::domain_jobs::JobId);
                            if response["ok"] != true || self.font_job.is_none() {
                                self.error = response["error"]["message"]
                                    .as_str()
                                    .unwrap_or(super::i18n::tr(
                                        "Не удалось начать импорт шрифта",
                                        "Could not start font import",
                                    ))
                                    .into();
                            } else {
                                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                            }
                        }
                        Ok(None) => {}
                        Err(error) => self.error = error,
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50))
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.font_picker = None;
                    self.error = super::i18n::tr(
                        "Диалог выбора шрифта завершился без результата",
                        "The font picker closed without a result",
                    )
                    .into();
                }
            }
        }
        if let Some(job_id) = self.font_job {
            let response = call(api, Command::PollJob { job_id });
            let status = &response["result"];
            match status["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50))
                }
                Some("succeeded") => {
                    self.font_job = None;
                    if let Some(id) = status["result"]["id"].as_str() {
                        if let Some(draft) = self.draft.as_mut() {
                            draft.emoji_font = id.to_owned();
                        }
                        self.emoji_fonts = None;
                        self.error = super::i18n::tr(
                            "Шрифт добавлен. Сохраните настройки, чтобы применить его.",
                            "Font added. Save settings to apply it.",
                        )
                        .into();
                    } else {
                        self.error = super::i18n::tr(
                            "Импорт шрифта не вернул идентификатор",
                            "Font import returned no identifier",
                        )
                        .into();
                    }
                    ctx.request_repaint();
                }
                _ => {
                    self.font_job = None;
                    self.error = status["error"]["message"]
                        .as_str()
                        .or_else(|| response["error"]["message"].as_str())
                        .unwrap_or(super::i18n::tr(
                            "Импорт шрифта отменён или не выполнен",
                            "Font import was canceled or failed",
                        ))
                        .into();
                }
            }
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, api: &mut Service) -> bool {
        self.tick_background(api, ctx);
        self.tick_video(api, ctx);
        self.tick_media(api, ctx);
        self.tick_font(api, ctx);
        self.updates.tick(api, ctx);
        let interaction = call(api, Command::Interaction);
        let open = interaction["result"]["settings_open"] == true;
        if !open {
            if self.editor.is_some() {
                ctx.request_repaint();
            }
            self.tags.reset();
            self.confirm_history = false;
            self.draft = None;
            self.custom_backgrounds = None;
            self.video_backgrounds = None;
            self.media_runtime = None;
            self.emoji_fonts = None;
            self.sharex_folder.clear();
            self.downloads_folder.clear();
            self.editor = None;
            self.requested_section = None;
            return false;
        }
        let requested = interaction["result"]["settings_section"]
            .as_u64()
            .unwrap_or(0) as usize;
        if self.requested_section != Some(requested) {
            self.section = requested;
            self.requested_section = Some(requested);
        }
        if self.draft.is_none() {
            self.draft =
                serde_json::from_value(call(api, Command::GetSettings)["result"].clone()).ok();
        }
        if let Some(draft) = self.draft.as_ref()
            && self.sharex_folder.is_empty()
        {
            self.sharex_folder = draft
                .sharex_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        if let Some(draft) = self.draft.as_ref()
            && self.downloads_folder.is_empty()
        {
            self.downloads_folder = draft
                .downloads_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        let logger = api.logger();
        let mut close = false;
        let mut tabs_changed = false;
        let mut global_intent = None;
        let mut theme_intent = None;
        let root_ctx = ctx.clone();
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("katran-settings"),egui::ViewportBuilder::default().with_title(super::i18n::tr("Настройки Katran", "Katran Settings")).with_decorations(false).with_inner_size([980.0,680.0]).with_min_inner_size([680.0,460.0]),|ctx,_| {
   if self.restart_confirmation {
    let prompt=egui::Modal::new(egui::Id::new("restart-confirmation")).show(ctx,|ui|{
            ui.heading(super::i18n::tr("Перезапустить Katran?", "Restart Katran?"));
            ui.label(super::i18n::tr(
                "Настройка стекла сохранена. Новое окно откроется с теми же данными.",
                "The glass setting was saved. The new window will use the same data.",
            ));
     ui.horizontal(|ui|{
                if ui.button(super::i18n::tr("Перезапустить сейчас", "Restart now")).clicked(){self.restart_requested=true;self.restart_confirmation=false;}
                if ui.button(super::i18n::tr("Позже", "Later")).clicked(){self.restart_confirmation=false;}
     });
    });
    if prompt.should_close(){self.restart_confirmation=false;}
   }
   let updates_ctx=ctx.clone();
   super::input_log::record(ctx,logger.as_ref());
   let recording=self.editor.is_some();
   if recording {
    let key_events=ctx.input(|i|i.events.iter().filter(|e|matches!(e,egui::Event::Key{pressed:true,..})).count());
    if key_events>0 && let Some(log)=logger.as_ref() {log.record("shortcut_editor_input",serde_json::json!({"target":self.editor.as_ref().map(|e|e.id.as_str()),"key_events":key_events,"viewport":"settings"}));}
   }

   if let Some(editor)=self.editor.as_mut() && let Some(intent)=editor.input(ctx) && apply_editor_intent(api,editor,intent,&mut self.draft){self.editor=None;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
   let toggle_close=!recording&&ctx.input_mut(|i|i.consume_key(egui::Modifiers::CTRL,egui::Key::Comma));
   let escape=!recording&&ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape));
   if escape && self.confirm_history { self.confirm_history=false; } else { close=ctx.input(|i|i.viewport().close_requested())||escape||toggle_close; }
   let surface=super::theme_ui::color(ctx,super::theme_ui::Role::Surface,Color32::from_gray(69));
   let background=super::theme_ui::color(ctx,super::theme_ui::Role::Background,Color32::from_gray(65));
   egui::CentralPanel::default().frame(egui::Frame::new().fill(surface)).show(ctx,|ui| {
    let (_,drag)=ui.allocate_exact_size(egui::vec2(ui.available_width(),8.0),egui::Sense::drag());
    if drag.drag_started(){ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);}
    egui::Panel::left("settings-sidebar").exact_size(215.0).resizable(false).frame(egui::Frame::new().fill(background).inner_margin(16)).show(ui,|ui|{
     super::theme_ui::decorate(ui,ui.max_rect());
     ui.add_space(8.0);
     ui.vertical_centered(|ui|{
      if ui.add_sized([ui.available_width(),28.0],egui::Button::new(RichText::new(super::i18n::tr("Настройки", "Settings")).size(19.0).strong()).frame(false)).on_hover_text(super::i18n::tr("Закрыть настройки", "Close settings")).clicked(){close=true;}
      ui.label(RichText::new(super::i18n::tr("Ctrl+, или нажмите «Настройки», чтобы закрыть", "Press Ctrl+, or click Settings to close")).size(11.0).color(Color32::from_gray(185)));
     });
     ui.add_space(16.0);
     ui.add_sized([ui.available_width(),34.0],egui::TextEdit::singleline(&mut self.query).hint_text(super::i18n::tr("Поиск настроек…", "Search settings…")).margin(egui::vec2(10.0,9.0)));
     ui.add_space(24.0);ui.label(RichText::new(super::i18n::tr("РАЗДЕЛЫ", "WORKSPACE")).size(10.0).color(Color32::from_gray(173)));ui.add_space(8.0);
     for(index,(label_ru,label_en))in [("Основные","General"),("Горячие клавиши","Shortcuts"),("Вкладки","Tabs"),("Обновления","Updates"),("Темы","Themes"),("Теги","Tags")].iter().enumerate(){let label=super::i18n::tr(label_ru,label_en);if section_matches_query(index,label_ru,&self.query)&&ui.add_sized([ui.available_width(),38.0],egui::Button::new(label).selected(self.section==index)).clicked(){self.section=index;self.editor=None;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}}
     ui.with_layout(egui::Layout::bottom_up(egui::Align::Min),|ui|{ui.weak(crate::BUILD_LABEL).on_hover_text(super::i18n::tr("Ctrl+, — закрыть настройки", "Ctrl+, — close settings"));});
    });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(surface).inner_margin(24)).show(ui,|ui|{
     super::theme_ui::decorate(ui,ui.max_rect());
     egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui,|ui|{
      if self.section==0 {
       ui.heading(super::i18n::tr("Основные", "General"));ui.add_space(16.0);
       if let Some(draft)=self.draft.as_mut(){
        SettingsSection::group(ui,super::i18n::tr("Язык интерфейса", "Interface language"),|ui|{
         SettingsRow::show(ui,super::i18n::tr("Язык", "Language"),super::i18n::tr("Изменение вступит в силу после сохранения.", "Changes take effect after saving."),|ui|{
          egui::ComboBox::from_id_salt("settings-language").selected_text(match draft.language{katran::settings::Language::English=>"English",katran::settings::Language::Russian=>"Русский"}).show_ui(ui,|ui|{
           ui.selectable_value(&mut draft.language,katran::settings::Language::English,"English");
           ui.selectable_value(&mut draft.language,katran::settings::Language::Russian,"Русский");
          });
         });
        });
        SettingsSection::group(ui,super::i18n::tr("Запуск и управление", "Startup and controls"),|ui|{
         let binding=if draft.overlay_hotkey.is_empty(){super::i18n::tr("Не назначена", "Not assigned")}else{draft.overlay_hotkey.as_str()};
         SettingsRow::show(ui,super::i18n::tr("Вызов Katran поверх окна", "Summon Katran over the current window"),binding,|ui|{
          if ui.button(super::i18n::tr("Изменить", "Change")).clicked(){self.editor=Some(super::shortcuts_ui::Editor::new("overlay_hotkey",super::i18n::tr("Вызов Katran", "Summon Katran"),draft.overlay_hotkey.clone()).with_physical_keys());root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
          if ui.button(super::i18n::tr("Очистить", "Clear")).clicked(){self.editor=Some(super::shortcuts_ui::Editor::new("overlay_hotkey",super::i18n::tr("Вызов Katran", "Summon Katran"),draft.overlay_hotkey.clone()).with_physical_keys());global_intent=Some(super::shortcuts_ui::Intent::Set{binding:String::new(),replace:false});}
          if ui.button(super::i18n::tr("Повторить", "Retry")).clicked(){self.overlay_retry=true;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
         });
         if !self.overlay_status.is_empty(){ui.weak(&self.overlay_status);}
         if let Some(editor)=self.editor.as_mut(){ui.separator();if let Some(intent)=editor.render(ui){global_intent=Some(intent);}ui.separator();}
         SettingsRow::show(ui,super::i18n::tr("Захват при запуске", "Capture on startup"),super::i18n::tr("По умолчанию включён. Текущий захват переключается через Actions.", "On by default. Toggle the current capture from Actions."),|ui|{ui.checkbox(&mut draft.capture_on_start,super::i18n::tr("Включать", "Enable"));});
         SettingsRow::show(ui,super::i18n::tr("Подсказка сортировки", "Sort hint"),super::i18n::tr("Показывать выбранный порядок на две секунды.", "Show the selected sort order for two seconds."),|ui|{ui.checkbox(&mut draft.sort_notifications,super::i18n::tr("Показывать", "Show"));});
         SettingsRow::show(ui,super::i18n::tr("Поиск на полке", "Shelf search"),super::i18n::tr("Обычно поле доступно мышью и через Ctrl+T. При ограничении мышь перемещает окно, а ввод включает Ctrl+T.", "Usually, use the search field with the mouse or Ctrl+T. In keyboard-only mode, the mouse moves the window and Ctrl+T opens search."),|ui|{ui.checkbox(&mut draft.search_keyboard_only,super::i18n::tr("Только через Ctrl+T", "Ctrl+T only"));});
        });
        SettingsSection::group(ui,super::i18n::tr("Ссылки", "Links"),|ui|{
         SettingsRow::show(ui,super::i18n::tr("Значки ссылок", "Link icons"),super::i18n::tr("Локальные символы работают без сети. При включении Katran запрашивает значки и объявленные сайтом иконки с публичных HTTPS-адресов; если значок недоступен, остаётся локальный символ.", "Local symbols work offline. When enabled, Katran fetches favicons and site-declared icons from public HTTPS addresses. If an icon is unavailable, the local symbol remains."),|ui|{ui.checkbox(&mut draft.link_favicons_online,super::i18n::tr("Загружать favicon сайтов", "Load website favicons"));});
         SettingsRow::show(ui,super::i18n::tr("Обновление значков", "Refresh icons"),super::i18n::tr("После сохранения сетевой настройки вручную проверяет все уже сохранённые ссылки. Ход обновления виден на полке.", "After saving the network setting, manually checks all saved links. Progress appears on the shelf."),|ui|{
          let online=call(api,Command::GetSettings)["result"]["link_favicons_online"]==true;
          if ui.add_enabled(online,egui::Button::new(super::i18n::tr("Обновить значки сохранённых ссылок", "Refresh icons for saved links"))).clicked(){self.refresh_links_requested=true;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
         });
        });
        SettingsSection::group(ui,super::i18n::tr("Представление карточек", "Card appearance"),|ui|{
         let font_list=self.emoji_fonts.get_or_insert_with(||call(api,Command::EmojiFonts)).clone();
         let entries=font_list["result"]["fonts"].as_array();
         let selected_font=entries.and_then(|items|items.iter().find(|item|item["id"].as_str()==Some(draft.emoji_font.as_str())));
         let font_label=selected_font.and_then(|item|item["label"].as_str()).unwrap_or(super::i18n::tr("Шрифт недоступен", "Font unavailable"));
         let font_available=selected_font.is_some_and(|item|item["available"]==true);
         SettingsRow::show(ui,super::i18n::tr("Шрифт эмодзи", "Emoji font"),if font_available {super::i18n::tr("Применяется после сохранения настроек.", "Applied after saving settings.")} else {super::i18n::tr("Шрифт недоступен; используется системный.", "Font unavailable; using the system font.")},|ui|{
          egui::ComboBox::from_id_salt("emoji-font-picker").width(180.0).selected_text(font_label).show_ui(ui,|ui|{
           for item in entries.into_iter().flatten(){if let Some(id)=item["id"].as_str(){let label=item["label"].as_str().unwrap_or(id);let display=if item["available"]==true {label.to_owned()} else {format!("{label}{}",super::i18n::tr(" · недоступен", " · unavailable"))};ui.selectable_value(&mut draft.emoji_font,id.to_owned(),display);}}
          });
          if ui.add_enabled(self.font_picker.is_none() && self.font_job.is_none() && self.video_picker.is_none() && self.video_job.is_none() && self.picker.is_none() && self.background_job.is_none() && self.video_picker.is_none() && self.video_job.is_none() && self.font_picker.is_none() && self.font_job.is_none(),egui::Button::image_and_text(egui_lucide::Lucide::FolderOpen.size(16.0).image(),super::i18n::tr("Добавить шрифт…", "Add font…"))).clicked(){match super::theme_file_picker::start_pick_font(){Ok(receiver)=>self.font_picker=Some(receiver),Err(error)=>self.error=error}}
         });
         let tab = serde_json::from_value::<katran::interaction::ShelfTab>(call(api,Command::Interaction)["result"]["tab"].clone()).unwrap_or_default();
         if let Some(profile)=draft.tab_preferences.get_mut(&tab) {
          ui.strong(format!("{}: {}",super::i18n::tr("Текущая вкладка", "Current tab"),localized_shelf_tab(tab)));
          SettingsRow::show(ui,super::i18n::tr("Вид вкладки", "Tab view"),super::i18n::tr("Сохраняется отдельно для этой вкладки.", "Saved separately for this tab."),|ui|{ui.selectable_value(&mut profile.view,View::Gallery,super::i18n::tr("Галерея", "Gallery"));ui.selectable_value(&mut profile.view,View::List,super::i18n::tr("Поток", "Stream"));});
          SettingsRow::show(ui,super::i18n::tr("Столбцы галереи", "Gallery columns"),super::i18n::tr("Ctrl− / Ctrl+: число карточек в строке этой вкладки.", "Ctrl− / Ctrl+: cards per row on this tab."),|ui|{ui.add(egui::Slider::new(&mut profile.gallery_columns,1..=12));});
          SettingsRow::show(ui,super::i18n::tr("Строки потока", "Stream rows"),super::i18n::tr("Число видимых строк в потоке этой вкладки.", "Number of visible rows in this tab's stream."),|ui|{ui.add(egui::Slider::new(&mut profile.stream_rows,1..=12));});
          if matches!(tab,katran::interaction::ShelfTab::Pods|katran::interaction::ShelfTab::Links) {
           SettingsRow::show(ui,super::i18n::tr("Компактные плитки", "Compact tiles"),super::i18n::tr("Число карточек в строке этой вкладки.", "Number of cards per row on this tab."),|ui|{ui.add(egui::Slider::new(&mut profile.compact_columns,1..=12));});
          }
         }
        });
        SettingsSection::group(ui,super::i18n::tr("Папки и хранение", "Folders and storage"),|ui|{
         SettingsRow::show(ui,super::i18n::tr("Папка ShareX", "ShareX folder"),super::i18n::tr("Абсолютный путь к папке снимков. Пустое поле отключает импорт; изменения поступают из Windows.", "Absolute path to the screenshots folder. Leave empty to disable import. Changes are received from Windows."),|ui|{
          if ui.add(egui::TextEdit::singleline(&mut self.sharex_folder).hint_text("C:\\Screenshots")).changed(){draft.sharex_folder=if self.sharex_folder.trim().is_empty(){None}else{Some(std::path::PathBuf::from(self.sharex_folder.trim()))};}
         });
         SettingsRow::show(ui,super::i18n::tr("Папка Downloads", "Downloads folder"),super::i18n::tr("Пустое поле использует системную папку загрузок Windows. Можно указать другую абсолютную папку.", "Leave empty to use the Windows Downloads folder, or enter another absolute path."),|ui|{
          if ui.add(egui::TextEdit::singleline(&mut self.downloads_folder).hint_text(super::i18n::tr("Системная папка Downloads", "System Downloads folder"))).changed(){draft.downloads_folder=if self.downloads_folder.trim().is_empty(){None}else{Some(std::path::PathBuf::from(self.downloads_folder.trim()))};}
         });
         let mut mib=draft.quota_bytes as f64/(1024.0*1024.0);
         SettingsRow::show(ui,super::i18n::tr("Квота кэша · МиБ", "Cache quota · MiB"),super::i18n::tr("Снижение ниже текущего объёма отклоняется. Исходники не удаляются.", "Values below current usage are rejected. Source files are not deleted."),|ui|{if ui.add(egui::DragValue::new(&mut mib).range(1.0..=1048576.0)).changed(){draft.quota_bytes=(mib*1024.0*1024.0)as u64;}});
        });
       }
       ui.add_space(16.0);
       SettingsSection::group(ui,super::i18n::tr("История Folder", "Folder history"),|ui|{
       if ui.button(super::i18n::tr("Очистить историю Folder", "Clear Folder history")).clicked() {
        let r=call(api,Command::ClearFolderHistory{confirmed:false});
        if r["result"]["confirmation_required"]==true {self.confirm_history=true;} else if r["ok"]==false {self.error=r["error"]["message"].as_str().unwrap_or(super::i18n::tr("Ошибка", "Error")).into();}
       }
       if self.confirm_history {
        ui.label(RichText::new(super::i18n::tr("Действительно удалить историю?", "Clear the history?")).strong());
        ui.label(super::i18n::tr("Избранные папки и файлы сохранятся.", "Pinned folders and files will be kept."));
        ui.horizontal(|ui|{
         if ui.button(super::i18n::tr("Удалить историю", "Delete history")).clicked(){let r=call(api,Command::ClearFolderHistory{confirmed:true});self.confirm_history=false;self.error=if r["result"]["cleared"]==true {super::i18n::tr("История очищена", "History cleared").into()} else {r["error"]["message"].as_str().unwrap_or(super::i18n::tr("Ошибка", "Error")).into()};}
         if ui.button(super::i18n::tr("Отмена", "Cancel")).clicked(){self.confirm_history=false;}
        });
       }
       });
       ui.add_space(16.0);
       if ui.button(super::i18n::tr("Сохранить настройки", "Save settings")).clicked()&&let Some(mut settings)=self.draft.clone(){
        let current=call(api,Command::GetSettings);
        if current["ok"]==true {
         if let Ok(saved)=serde_json::from_value::<Settings>(current["result"].clone()) {
          merge_tab_settings(&mut settings,&saved);
          let language = settings.language;
          let r=call(api,Command::SetSettings{settings});self.error=if r["ok"]==true{super::i18n::set(language);super::i18n::tr("Настройки сохранены", "Settings saved").into()}else{r["error"]["message"].as_str().unwrap_or(super::i18n::tr("Ошибка сохранения", "Save failed")).into()};
         } else {self.error=super::i18n::tr("Не удалось прочитать текущие настройки", "Could not read current settings").into();}
        } else {self.error=current["error"]["message"].as_str().unwrap_or(super::i18n::tr("Не удалось прочитать текущие настройки", "Could not read current settings")).into();}
       }
      } else if self.section==2 {
       ui.heading(super::i18n::tr("Вкладки", "Tabs"));ui.add_space(8.0);
       ui.label(super::i18n::tr("Выключенная вкладка скрывается и останавливает сбор связанных данных. Сохранённые данные остаются на месте.", "A disabled tab is hidden and stops collecting related data. Saved data stays in place."));ui.add_space(16.0);
       let result=call(api,Command::Tabs);
       if let Some(tabs)=result["result"]["tabs"].as_array() {
        SettingsSection::group(ui,super::i18n::tr("Доступные вкладки", "Available tabs"),|ui|{
         for entry in tabs {
          let label=localized_tab_label(entry["label"].as_str().unwrap_or(""));
          let enabled=entry["enabled"]==true;
          let can_toggle=entry["can_toggle"]==true;
          if let Ok(tab)=serde_json::from_value(entry["tab"].clone()) {
           SettingsRow::show(ui,label,if can_toggle {if enabled {super::i18n::tr("Включена", "Enabled")} else {super::i18n::tr("Выключена", "Disabled")}} else {super::i18n::tr("Всегда включена", "Always enabled")},|ui|{
            if ui.add_enabled(can_toggle,egui::Button::image(if enabled {egui_lucide::Lucide::Eye.size(18.0).image()} else {egui_lucide::Lucide::EyeOff.size(18.0).image()})).on_hover_text(if enabled {super::i18n::tr("Выключить вкладку", "Disable tab")} else {super::i18n::tr("Включить вкладку", "Enable tab")}).clicked() {
             let response=call(api,Command::SetTabEnabled{tab,enabled:!enabled});
             if response["ok"]==true {
              tabs_changed=true;
              if let Ok(saved)=serde_json::from_value::<Settings>(call(api,Command::GetSettings)["result"].clone()) && let Some(draft)=self.draft.as_mut() {merge_tab_settings(draft,&saved);}
              self.error.clear();
             } else {self.error=response["error"]["message"].as_str().unwrap_or(super::i18n::tr("Ошибка", "Error")).into();}
            }
           });
          }
         }
        });
       }
      } else if self.section==3 {
       self.updates.show(ui,api,&updates_ctx);
      } else if self.section==4 {
       ui.heading(super::i18n::tr("Темы", "Themes"));ui.add_space(8.0);
       ui.label(super::i18n::tr("Тема применяется сразу. Стандартный вид остаётся доступен.", "The theme applies immediately. The standard appearance remains available."));ui.add_space(16.0);
       let response=call(api,Command::Themes);
       if response["ok"]==true {
        if let Ok(mut theme)=serde_json::from_value::<katran::theme::ThemePreferences>(response["result"]["current"].clone()) {
         SettingsSection::group(ui,super::i18n::tr("Тема и цвета", "Theme and colors"),|ui|{
          SettingsRow::show(ui,super::i18n::tr("Стандартная", "Standard"),super::i18n::tr("Привычный внешний вид Katran.", "The familiar Katran appearance."),|ui|{
           if ui.selectable_value(&mut theme.preset,katran::theme::Preset::Standard,super::i18n::tr("Выбрать", "Select")).changed(){theme_intent=Some(theme.clone());}
          });
          SettingsRow::show(ui,super::i18n::tr("Синяя", "Blue"),super::i18n::tr("Экспериментальная тема с голубыми акцентами.", "Experimental theme with blue accents."),|ui|{
           if ui.selectable_value(&mut theme.preset,katran::theme::Preset::BlueExperimental,super::i18n::tr("Выбрать", "Select")).changed(){theme_intent=Some(theme.clone());}
          });
          if selection_settings(ui,&mut theme){theme_intent=Some(theme.clone());}
          if theme.preset==katran::theme::Preset::BlueExperimental {
           SettingsRow::show(ui,super::i18n::tr("Цвет акцента", "Accent color"),super::i18n::tr("Выберите свой цвет или верните исходный.", "Choose a custom color or restore the default."),|ui|{
            let mut accent=theme.accent.unwrap_or([75,139,210]);
            if ui.color_edit_button_srgb(&mut accent).changed(){theme.accent=Some(accent);theme_intent=Some(theme.clone());}
            if ui.button(super::i18n::tr("Сбросить цвет", "Reset color")).clicked(){theme.accent=None;theme_intent=Some(theme.clone());}
           });
          }
         });
         if theme.preset==katran::theme::Preset::BlueExperimental {
          SettingsSection::group(ui,super::i18n::tr("Окно и эффекты", "Window and effects"),|ui|{
           SettingsRow::show(ui,super::i18n::tr("Компактное окно", "Compact window"),super::i18n::tr("Убрать название и крестик. Перемещение — за узкую полосу над поиском.", "Hide the title and close button. Drag the narrow strip above search to move the window."),|ui|{
            if ui.checkbox(&mut theme.compact_header,super::i18n::tr("Без шапки", "Hide title bar")).changed(){theme_intent=Some(theme.clone());}
           });
           SettingsRow::show(ui,super::i18n::tr("Стекло · эксперимент", "Glass · experimental"),super::i18n::tr("Windows 11 Acrylic. После включения полностью выйдите через трей и запустите Katran снова. При недоступности — сплошной фон.", "Windows 11 Acrylic. After enabling, quit Katran from the tray and start it again. A solid background is used if Acrylic is unavailable."),|ui|{
            if ui.checkbox(&mut theme.glass,super::i18n::tr("Включить", "Enable")).changed(){theme_intent=Some(theme.clone());}
           });
           SettingsRow::show(ui,super::i18n::tr("Стеклянные вкладки · эксперимент", "Glass tabs · experimental"),super::i18n::tr("Отдельные объёмные кнопки и движущееся выделение вашего цвета. Базовая полоска остаётся доступной; оптика может потребовать больше ресурсов.", "Separate raised buttons with a moving selection in your color. The basic tab strip remains available; optical effects may use more resources."),|ui|{
            if ui.checkbox(&mut theme.experimental_glass_tabs,super::i18n::tr("Включить", "Enable")).changed(){theme_intent=Some(theme.clone());}
           });
           if optical_settings(ui,&mut theme){theme_intent=Some(theme.clone());}
          });
          SettingsSection::group(ui,super::i18n::tr("Фон", "Background"),|ui|{
           SettingsRow::show(ui,super::i18n::tr("Фон", "Background"),super::i18n::tr("Лилии от дизайнера. Изображение загружается один раз.", "Designer lily backgrounds. The image is loaded once."),|ui|{
            if ui.selectable_label(theme.lily_background.is_none() && theme.custom_background.is_none() && theme.active_video().is_none(),super::i18n::tr("Сплошной", "Solid")).clicked(){theme.lily_background=None;theme.custom_background=None;theme.video_background=None;theme.video_enabled=Some(false);theme_intent=Some(theme.clone());}
            for i in 1..=5 {if ui.selectable_value(&mut theme.lily_background,Some(i),format!("{i}")).changed(){theme.custom_background=None;theme.video_background=None;theme.video_enabled=Some(false);theme_intent=Some(theme.clone());}}
           });
           SettingsRow::show(ui,super::i18n::tr("Собственные фоны", "Custom backgrounds"),super::i18n::tr("Копия сохраняется в Katran. Пресеты лилий остаются доступны.", "A copy is saved in Katran. Lily presets remain available."),|ui|{
            let list=self.custom_backgrounds.get_or_insert_with(||call(api,Command::ListThemeBackgrounds)).clone();
            egui::ComboBox::from_id_salt("custom-background-picker").width(180.0_f32.min(ui.available_width())).selected_text(if theme.custom_background.is_some(){super::i18n::tr("Собственный фон", "Custom background")}else{super::i18n::tr("Выбрать сохранённый", "Choose saved background")}).show_ui(ui,|ui|{
             for item in list["result"]["backgrounds"].as_array().into_iter().flatten(){
              if let Some(id)=item["id"].as_str() && ui.selectable_label(theme.custom_background.as_deref()==Some(id),item["name"].as_str().unwrap_or(super::i18n::tr("Изображение", "Image"))).clicked(){theme.custom_background=Some(id.to_owned());theme.lily_background=None;theme.video_background=None;theme.video_enabled=Some(false);theme_intent=Some(theme.clone());}
             }
            });
            if ui.add_enabled(self.picker.is_none() && self.background_job.is_none(),egui::Button::image_and_text(egui_lucide::Lucide::FolderOpen.size(16.0).image(),super::i18n::tr("Выбрать файл…", "Choose file…"))).clicked(){
                match super::theme_file_picker::start_pick_image(){Ok(receiver)=>self.picker=Some(receiver),Err(error)=>self.error=error}
            }
           });
          });
          SettingsSection::group(ui,super::i18n::tr("Видеофон", "Video background"),|ui|{
           let mut video_enabled = theme.video_is_enabled();
           SettingsRow::show(ui,super::i18n::tr("Видеофон", "Video background"),super::i18n::tr("Управляемая копия; звук не воспроизводится.", "A managed copy; audio is not played."),|ui|{
            if ui.checkbox(&mut video_enabled,super::i18n::tr("Включить", "Enable")).changed(){theme.video_enabled=Some(video_enabled);if video_enabled {theme.lily_background=None;theme.custom_background=None;}theme_intent=Some(theme.clone());}
           });
           if video_enabled {
           let media=self.media_runtime.get_or_insert_with(||call(api,Command::MediaRuntimeStatus)).clone();
           let media_available=media["ok"] == true && media["result"]["available"] == true;
           if media["ok"] == true && !media_available {
            SettingsRow::show(ui,super::i18n::tr("Компоненты видео", "Video components"),super::i18n::tr("Для работы видео требуется загрузка около 73 МиБ.", "Video playback requires a download of about 73 MiB."),|ui|{
             if ui.add_enabled(!self.background_busy(),egui::Button::new(if self.media_job.is_some(){super::i18n::tr("Подготовка…", "Preparing…")}else{super::i18n::tr("Подготовить компоненты видео", "Prepare video components")})).clicked(){
              let response=call(api,Command::BeginJob{command:Box::new(Command::EnsureMediaRuntime)});
              self.media_job=response["result"]["job_id"].as_u64().map(katran::domain_jobs::JobId);
              if response["ok"] != true || self.media_job.is_none(){self.error=response["error"]["message"].as_str().unwrap_or(super::i18n::tr("Не удалось начать подготовку компонентов видео", "Could not start preparing video components")).into();}else{self.error.clear();ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));}
             }
            });
           } else if media["ok"] != true {
           ui.weak(super::i18n::tr("Не удалось проверить компоненты видео. Откройте настройки повторно.", "Could not check video components. Reopen Settings."));
           }
           SettingsRow::show(ui,super::i18n::tr("Сглаживать стык", "Smooth loop seam"),super::i18n::tr("Короткое смешивание кадров на границе кэшированного цикла.", "Briefly blend frames at the cached loop boundary."),|ui|{
            if ui.checkbox(&mut theme.video_seam_smoothing,super::i18n::tr("Включить", "Enable")).changed(){theme_intent=Some(theme.clone());}
           });
           SettingsRow::show(ui,super::i18n::tr("Воспроизводить в фоне", "Play in background"),super::i18n::tr("Сохранять декодер активным, когда окно скрыто или свёрнуто. Увеличивает расход CPU и GPU.", "Keep the decoder active while the window is hidden or minimized. Uses more CPU and GPU."),|ui|{
            if ui.checkbox(&mut theme.video_play_hidden,super::i18n::tr("Включить", "Enable")).changed(){theme_intent=Some(theme.clone());}
           });
           ui.weak(super::i18n::tr("Короткие ролики кэшируются до 512 МиБ; длинные воспроизводятся потоком.", "Short videos are cached up to 512 MiB; longer videos stream."));
           SettingsRow::show(ui,super::i18n::tr("Файл видео", "Video file"),super::i18n::tr("Выберите сохранённое видео или добавьте файл.", "Choose a saved video or add a file."),|ui|{
            let list=self.video_backgrounds.get_or_insert_with(||call(api,Command::VideoBackgrounds)).clone();
            let selected=list["result"]["items"].as_array().and_then(|items|items.iter().find(|item|item["id"].as_str()==theme.video_background.as_deref())).and_then(|item|item["name"].as_str()).unwrap_or(super::i18n::tr("Выбрать сохранённое", "Choose saved video"));
            ui.horizontal_wrapped(|ui|{
             egui::ComboBox::from_id_salt("video-background-picker").truncate().width(180.0_f32.min(ui.available_width())).selected_text(selected).show_ui(ui,|ui|{
              for item in list["result"]["items"].as_array().into_iter().flatten(){
               if let Some(id)=item["id"].as_str() && ui.selectable_label(theme.video_background.as_deref()==Some(id),item["name"].as_str().unwrap_or(super::i18n::tr("Видео", "Video"))).clicked(){theme.video_background=Some(id.to_owned());theme.video_enabled=Some(true);theme.lily_background=None;theme.custom_background=None;theme_intent=Some(theme.clone());}
              }
             });
             if ui.add_enabled(!self.background_busy() && media_available,egui::Button::image_and_text(egui_lucide::Lucide::FolderOpen.size(16.0).image(),super::i18n::tr("Выбрать видео…", "Choose video…"))).clicked(){match super::theme_file_picker::start_pick_video(){Ok(receiver)=>self.video_picker=Some(receiver),Err(error)=>self.error=error}}
            });
           });
           }
          });
          SettingsSection::group(ui,super::i18n::tr("Декор", "Decoration"),|ui|{
           SettingsRow::show(ui,super::i18n::tr("Декоративные линии", "Decorative lines"),super::i18n::tr("Мягкий рисунок на фоне синей темы.", "A subtle pattern on the blue theme background."),|ui|{
            if ui.checkbox(&mut theme.decoration,super::i18n::tr("Показывать", "Show")).changed(){theme_intent=Some(theme.clone());}
           });
          });
         }
         SettingsSection::group(ui,super::i18n::tr("Анимации окна", "Window animations"),|ui|{
          let styles = [("none",super::i18n::tr("Без анимации", "None")),("slide_down",super::i18n::tr("Сдвиг вниз", "Slide down")),("slide_up",super::i18n::tr("Сдвиг вверх", "Slide up")),("slide_left",super::i18n::tr("Сдвиг влево", "Slide left")),("slide_right",super::i18n::tr("Сдвиг вправо", "Slide right")),("zoom",super::i18n::tr("Масштабирование", "Zoom")),("fade",super::i18n::tr("Растворение", "Fade")),("zoom_fade",super::i18n::tr("Масштабирование и растворение", "Zoom and fade"))];
          SettingsRow::show(ui,super::i18n::tr("Появление", "Show animation"),super::i18n::tr("Как окно появляется при вызове.", "How the window appears when summoned."),|ui|{
           egui::ComboBox::from_id_salt("animation-show-style")
            .selected_text(styles.iter().find(|(id,_)|*id==theme.animation_show_style).map_or(super::i18n::tr("Выбрать", "Select"),|(_,label)|*label))
            .show_ui(ui,|ui|{for (id,label) in styles {if ui.selectable_value(&mut theme.animation_show_style,id.to_owned(),label).changed(){theme_intent=Some(theme.clone());}}});
          });
          SettingsRow::show(ui,super::i18n::tr("Время появления", "Show duration"),super::i18n::tr("От 50 до 800 мс; при режиме без анимации не используется.", "From 50 to 800 ms; unused when animation is disabled."),|ui|{
           if ui.add_enabled(theme.animation_show_style!="none",egui::Slider::new(&mut theme.animation_show_ms,50..=800).suffix(super::i18n::tr(" мс", " ms"))).changed(){theme_intent=Some(theme.clone());}
          });
          SettingsRow::show(ui,super::i18n::tr("Исчезновение", "Hide animation"),super::i18n::tr("Как окно скрывается.", "How the window is hidden."),|ui|{
           egui::ComboBox::from_id_salt("animation-hide-style")
            .selected_text(styles.iter().find(|(id,_)|*id==theme.animation_hide_style).map_or(super::i18n::tr("Выбрать", "Select"),|(_,label)|*label))
            .show_ui(ui,|ui|{for (id,label) in styles {if ui.selectable_value(&mut theme.animation_hide_style,id.to_owned(),label).changed(){theme_intent=Some(theme.clone());}}});
          });
          SettingsRow::show(ui,super::i18n::tr("Время исчезновения", "Hide duration"),super::i18n::tr("От 50 до 800 мс; при режиме без анимации не используется.", "From 50 to 800 ms; unused when animation is disabled."),|ui|{
           if ui.add_enabled(theme.animation_hide_style!="none",egui::Slider::new(&mut theme.animation_hide_ms,50..=800).suffix(super::i18n::tr(" мс", " ms"))).changed(){theme_intent=Some(theme.clone());}
          });
         });
         SettingsSection::group(ui,super::i18n::tr("Прокрутка", "Scrolling"),|ui|{
          SettingsRow::show(ui,super::i18n::tr("Стеклянная полоса прокрутки", "Glass scrollbar"),super::i18n::tr("Прозрачное оформление полосы прокрутки полки.", "Transparent styling for the shelf scrollbar."),|ui|{
           if ui.checkbox(&mut theme.scrollbar_glass,super::i18n::tr("Включить", "Enable")).changed(){theme_intent=Some(theme.clone());}
          });
          SettingsRow::show(ui,super::i18n::tr("Всегда показывать полосу прокрутки", "Always show scrollbar"),super::i18n::tr("Полоса остаётся видимой, когда содержимое можно прокручивать.", "Keep the scrollbar visible whenever content can scroll."),|ui|{
           if ui.checkbox(&mut theme.scrollbar_always_visible,super::i18n::tr("Показывать", "Show")).changed(){theme_intent=Some(theme.clone());}
          });
          SettingsRow::show(ui,super::i18n::tr("Мягкая прокрутка · эксперимент", "Smooth scrolling · experimental"),super::i18n::tr("Дополнительная инерция колесика на полке. При выключении действует обычная прокрутка egui.", "Extra wheel inertia on the shelf. When disabled, standard egui scrolling is used."),|ui|{
           if ui.checkbox(&mut theme.banana_scroll,super::i18n::tr("Включить", "Enable")).changed(){theme_intent=Some(theme.clone());}
          });
          if theme.banana_scroll {
           SettingsRow::show(ui,super::i18n::tr("Скорость прокрутки", "Scroll speed"),super::i18n::tr("Длина движения от одного шага колесика.", "Distance moved per wheel step."),|ui|{
            if ui.add(egui::Slider::new(&mut theme.banana_scroll_speed,25..=300).suffix("%")).changed(){theme_intent=Some(theme.clone());}
           });
           SettingsRow::show(ui,super::i18n::tr("Инерция прокрутки", "Scroll inertia"),super::i18n::tr("Время мягкого затухания после движения.", "How long movement continues to ease out."),|ui|{
            if ui.add(egui::Slider::new(&mut theme.banana_scroll_inertia_ms,50..=1000).suffix(super::i18n::tr(" мс", " ms"))).changed(){theme_intent=Some(theme.clone());}
           });
          }
         });
        } else {ui.label(super::i18n::tr("Не удалось прочитать текущую тему.", "Could not read current theme."));}
       } else {ui.label(response["error"]["message"].as_str().unwrap_or(super::i18n::tr("Не удалось загрузить темы", "Could not load themes")));}
      } else if self.section==5 {
       if self.tags.show(ui,api){tabs_changed=true;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
      } else {
       ui.heading(super::i18n::tr("Горячие клавиши", "Shortcuts"));ui.add_space(16.0);
       if ui.add(egui::TextEdit::singleline(&mut self.shortcut_filter).hint_text(super::i18n::tr("Поиск команды…", "Search commands…")).desired_width(ui.available_width())).changed(){self.editor=None;}
       let shortcuts:Vec<katran::shortcuts::ShortcutInfo>=serde_json::from_value(call(api,Command::Shortcuts)["result"].clone()).unwrap_or_default();
       let mut edit=None;
       SettingsSection::show(ui,|ui|{
        for shortcut in shortcuts.iter().filter(|s| !s.id.starts_with("sort_")) {
         let shortcut_label = localized_shortcut_label(&shortcut.id, &shortcut.label);
         if !shortcut_label.to_lowercase().contains(&self.shortcut_filter.to_lowercase()) { continue; }
         ui.push_id(&shortcut.id,|ui|{
          SettingsRow::show(ui,shortcut_label,shortcut.binding.as_deref().unwrap_or(super::i18n::tr("Не назначено", "Not assigned")),|ui|{
           ui.add(super::action_icons::icon(&shortcut.id).size(16.0).image());
           if shortcut.rebindable {
            if ui.button(super::i18n::tr("Изменить", "Change")).clicked(){edit=Some(shortcut.clone());}
            if ui.button(super::i18n::tr("Сбросить", "Reset")).clicked(){let r=call(api,Command::SetShortcut{id:shortcut.id.clone(),binding:None,replace:false,reset:true});if r["ok"]==false{self.error=r["error"]["message"].as_str().unwrap_or(super::i18n::tr("Ошибка", "Error")).into();}}
           }else{ui.weak(super::i18n::tr("Системное сочетание", "System shortcut"));}
          });
          if self.editor.as_ref().is_some_and(|e|e.id==shortcut.id) && let Some(editor)=self.editor.as_mut() {
           ui.separator();if let Some(intent)=editor.render(ui) && apply_editor_intent(api,editor,intent,&mut self.draft){self.editor=None;}ui.separator();
          }
         });
        }
       });
       if let Some(s)=edit {let label=localized_shortcut_label(&s.id,&s.label).to_owned();self.editor=Some(super::shortcuts_ui::Editor::new(s.id,label,s.binding.unwrap_or_default()));}
      }
      if !self.error.is_empty(){super::ui_foundation::message(ui,&self.error,false);}
     });
    });
   });
  });
        if let Some(theme) = theme_intent {
            let previous = call(api, Command::Themes)["result"]["current"]["glass"]
                .as_bool()
                .unwrap_or(false);
            let changed = previous != theme.glass;
            self.error = match apply_theme(api, &mut self.draft, theme) {
                Ok(()) => {
                    if changed {
                        self.restart_confirmation = true;
                    }
                    String::new()
                }
                Err(error) => error,
            };
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if let Some(intent) = global_intent
            && let Some(editor) = self.editor.as_mut()
            && apply_editor_intent(api, editor, intent, &mut self.draft)
        {
            self.editor = None;
            ctx.request_repaint();
        }
        if close {
            call(api, Command::CloseSettings);
            self.draft = None;
            self.custom_backgrounds = None;
            self.sharex_folder.clear();
            self.downloads_folder.clear();
            self.editor = None;
            ctx.request_repaint();
            self.requested_section = None;
        }
        tabs_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn general_overlay_recording_runs_through_settings_viewport_and_persists() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 10000).unwrap();
        call(&mut api, Command::ShowSettings);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(true);
        super::super::ui_style::install(&ctx);
        let mut state = State {
            editor: Some(
                super::super::shortcuts_ui::Editor::new("overlay_hotkey", "Вызов Katran", "")
                    .with_physical_keys(),
            ),
            ..Default::default()
        };
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::R,
                    physical_key: Some(egui::Key::R),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                }],
                ..Default::default()
            },
            |ui| {
                state.show(ui.ctx(), &mut api);
            },
        );
        out.textures_delta.clear();
        assert!(state.editor.is_none());
        assert!(!state.is_recording_overlay());
        assert_eq!(
            call(&mut api, Command::GetSettings)["result"]["overlay_hotkey"],
            "Alt R"
        );
        assert_eq!(state.draft.as_ref().unwrap().overlay_hotkey, "Alt R");
    }
    use katran::interaction::ShelfTab;

    #[test]
    fn pending_native_picker_does_not_block_dispatcher_or_change_theme() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 10000).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut state = State {
            picker: Some(receiver),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let before = call(&mut api, Command::Themes);
        state.tick_background(&mut api, &ctx);
        assert!(state.picker.is_some());
        let result = call(
            &mut api,
            Command::IngestBytes {
                name: "capture.png".into(),
                bytes: vec![1, 2, 3],
                source: "synthetic".into(),
            },
        );
        assert_eq!(result["ok"], true);
        assert_eq!(call(&mut api, Command::Themes), before);
        sender.send(Ok(None)).unwrap();
        state.tick_background(&mut api, &ctx);
        assert!(state.picker.is_none());
    }
    #[test]
    fn ctrl_comma_closes_settings_and_is_consumed() {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 10000).unwrap();
        call(&mut api, Command::ShowSettings);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(true);
        super::super::ui_style::install(&ctx);
        let mut state = State::default();
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Comma,
                    physical_key: Some(egui::Key::Comma),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ui| {
                state.show(ui.ctx(), &mut api);
            },
        );
        out.textures_delta.clear();
        assert_eq!(
            call(&mut api, Command::Interaction)["result"]["settings_open"],
            false
        );
    }
    #[test]
    fn general_draft_preserves_newer_tab_changes() {
        let mut draft = Settings {
            gallery_columns: 7,
            capture_on_start: true,
            ..Settings::default()
        };
        let current = Settings {
            tab_order: vec![ShelfTab::Downloads, ShelfTab::Images, ShelfTab::Screenshots],
            hidden_tabs: vec![ShelfTab::Images, ShelfTab::Downloads],
            downloads_enabled: false,
            text_enabled: true,
            links_enabled: true,
            tables_enabled: true,
            sort: katran::sorting::SortSpec {
                mode: katran::sorting::SortMode::Origin,
                reverse: true,
                seed: 42,
            },
            ..Settings::default()
        };
        merge_tab_settings(&mut draft, &current);
        assert_eq!(draft.tab_order, current.tab_order);
        assert_eq!(draft.hidden_tabs, current.hidden_tabs);
        assert!(!draft.downloads_enabled);
        assert!(draft.text_enabled);
        assert!(draft.links_enabled);
        assert!(draft.tables_enabled);
        assert_eq!(draft.sort, current.sort);
        assert_eq!(draft.gallery_columns, 7);
        assert!(draft.capture_on_start);
    }

    #[test]
    fn theme_applies_immediately_without_saving_unrelated_draft_fields() {
        use katran::theme::{Preset, ThemePreferences};
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 1_000_000).unwrap();
        let saved: Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
        let mut draft = Some(Settings {
            gallery_columns: 7,
            ..saved.clone()
        });
        let blue = ThemePreferences {
            preset: Preset::BlueExperimental,
            accent: Some([40, 100, 220]),
            decoration: false,
            lily_background: None,
            custom_background: None,
            glass: false,
            compact_header: false,
            optical_strength: 0,
            ..Default::default()
        };
        apply_theme(&mut api, &mut draft, blue.clone()).unwrap();
        let current: Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
        assert_eq!(current.theme, blue);
        assert_eq!(current.gallery_columns, saved.gallery_columns);
        assert_eq!(draft.as_ref().unwrap().gallery_columns, 7);
        assert_eq!(draft.as_ref().unwrap().theme, blue);
        let mut stale = Settings {
            gallery_columns: 9,
            ..saved
        };
        merge_tab_settings(&mut stale, &current);
        assert_eq!(stale.theme, blue);
        assert_eq!(stale.gallery_columns, 9);
        call(&mut api, Command::SetSettings { settings: stale });
        assert_eq!(
            call(&mut api, Command::Themes)["result"]["current"]["preset"],
            "blue_experimental"
        );
        apply_theme(&mut api, &mut draft, ThemePreferences::default()).unwrap();
        assert_eq!(
            call(&mut api, Command::Themes)["result"]["current"]["preset"],
            "standard"
        );
    }

    #[test]
    fn global_recorder_saves_only_binding_and_keeps_draft_changes() {
        use super::super::shortcuts_ui::{Editor, Intent};
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 1_000_000).unwrap();
        let saved: Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
        let mut draft = Some(Settings {
            gallery_columns: 7,
            ..saved.clone()
        });
        let mut editor = Editor::new("overlay_hotkey", "Вызов Katran", "");
        assert!(!apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Set {
                binding: "A".into(),
                replace: false
            },
            &mut draft
        ));
        assert_eq!(
            call(&mut api, Command::GetSettings)["result"]["overlay_hotkey"],
            ""
        );
        assert!(apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Set {
                binding: "Alt R".into(),
                replace: false
            },
            &mut draft
        ));
        let stored = call(&mut api, Command::GetSettings);
        assert_eq!(stored["result"]["overlay_hotkey"], "Alt R");
        assert_eq!(stored["result"]["gallery_columns"], saved.gallery_columns);
        assert_eq!(draft.as_ref().unwrap().gallery_columns, 7);
        assert_eq!(draft.as_ref().unwrap().overlay_hotkey, "Alt R");
        assert!(apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Cancel,
            &mut draft
        ));
        assert_eq!(
            call(&mut api, Command::GetSettings)["result"]["overlay_hotkey"],
            "Alt R"
        );
        assert!(apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Set {
                binding: String::new(),
                replace: false
            },
            &mut draft
        ));
        assert_eq!(
            call(&mut api, Command::GetSettings)["result"]["overlay_hotkey"],
            ""
        );
        assert_eq!(draft.as_ref().unwrap().gallery_columns, 7);
        assert!(draft.unwrap().overlay_hotkey.is_empty());
    }

    #[test]
    fn global_editor_consumes_chord_and_escape_and_retry_is_one_shot() {
        use super::super::shortcuts_ui::{Editor, Intent};
        let ctx = egui::Context::default();
        let mut editor = Editor::new("overlay_hotkey", "Вызов Katran", "Alt R");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(editor.input(ui.ctx()).is_none());
        });
        output.textures_delta.clear();
        let input = egui::RawInput {
            events: vec![
                egui::Event::Key {
                    key: egui::Key::R,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                },
                egui::Event::Text("r".into()),
            ],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(matches!(editor.input(ui.ctx()), Some(Intent::Set { binding, .. }) if binding == "Alt R"));
            assert!(ui.ctx().input(|i| i.events.is_empty()));
        });
        output.textures_delta.clear();
        let ctx = egui::Context::default();
        let mut editor = Editor::new("overlay_hotkey", "Вызов Katran", "Alt R");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            editor.input(ui.ctx());
        });
        output.textures_delta.clear();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(matches!(editor.input(ui.ctx()), Some(Intent::Cancel)));
            assert!(ui.ctx().input(|i| i.events.is_empty()));
        });
        output.textures_delta.clear();
        let mut state = State {
            overlay_retry: true,
            ..Default::default()
        };
        assert!(state.take_overlay_retry());
        assert!(!state.take_overlay_retry());
    }
}
#[cfg(test)]
mod optical_layout_tests {
    use super::*;
    #[test]
    fn optical_options_fit_and_are_hidden_when_disabled() {
        for scale in [1.0, 1.5, 2.0] {
            for width in [320.0, 580.0, 900.0] {
                for enabled in [false, true] {
                    let ctx = egui::Context::default();
                    ctx.set_pixels_per_point(scale);
                    super::super::ui_style::install(&ctx);
                    let mut theme = katran::theme::ThemePreferences {
                        optical_strength: if enabled { 65 } else { 0 },
                        ..Default::default()
                    };
                    let mut allocated = egui::Rect::NOTHING;
                    let mut boundary = egui::Rect::NOTHING;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width + 32.0, 1600.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            ui.set_width(width);
                            ui.set_max_width(width);
                            boundary = ui.max_rect();
                            optical_settings(ui, &mut theme);
                            allocated = ui.min_rect();
                        },
                    );
                    output.textures_delta.clear();
                    assert!(
                        allocated.right() <= boundary.right() + 1.0,
                        "width={width}, scale={scale}: {allocated:?} exceeds {boundary:?}"
                    );
                    for title in [
                        "Сила эффекта",
                        "Преломление",
                        "Размытие",
                        "Краевые блики",
                        "Разделение цветов",
                    ] {
                        let visible = output.shapes.iter().any(
                            |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==title),
                        );
                        assert_eq!(visible, enabled, "{title}, width={width}, scale={scale}");
                    }
                }
            }
        }
    }
}

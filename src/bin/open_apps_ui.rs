//! Application picker: a client of the domain's bound discovery session.
use super::*;
use base64::Engine;
use katran::{app_handlers::AppHandler, domain_jobs::JobId};

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    job: Option<JobId>,
    token: Option<String>,
    apps: Vec<AppHandler>,
    textures: Vec<Option<egui::TextureHandle>>,
    query: String,
    selected: usize,
    focus: bool,
    scroll: bool,
    name: String,
}

impl Shelf {
    pub(super) fn open_apps_request(&mut self, ctx: &egui::Context) {
        if self.open_apps.open {
            self.close_open_apps();
            return;
        }
        self.actions = false;
        self.open_apps = open_apps_ui::State {
            open: true,
            focus: true,
            ..Default::default()
        };
        let result = self.call(Command::BeginJob {
            command: Box::new(Command::RunAction {
                id: "open_with".into(),
            }),
        });
        if result["state"] == "succeeded" {
            self.open_apps_accept(&result["result"], ctx);
        } else if let Some(id) = result["job_id"].as_u64() {
            self.open_apps.job = Some(JobId(id));
            ctx.request_repaint();
        } else {
            self.close_open_apps();
        }
    }
    fn close_open_apps(&mut self) {
        if let Some(job_id) = self.open_apps.job.take() {
            self.call(Command::CancelJob { job_id });
        }
        if let Some(token) = self.open_apps.token.take() {
            self.call(Command::ReleaseOpenApps { token });
        }
        self.open_apps = Default::default();
    }
    fn open_apps_accept(&mut self, result: &Value, ctx: &egui::Context) {
        let Some(token) = result["token"].as_str() else {
            self.message = "Не удалось получить список приложений".into();
            self.close_open_apps();
            return;
        };
        self.open_apps.token = Some(token.to_owned());
        self.open_apps.name = result["name"].as_str().unwrap_or("").to_owned();
        self.open_apps.apps = serde_json::from_value(result["apps"].clone()).unwrap_or_default();
        self.open_apps
            .apps
            .truncate(katran::app_handlers::MAX_HANDLERS);
        self.open_apps.textures = self
            .open_apps
            .apps
            .iter()
            .enumerate()
            .map(|(index, app)| {
                let icon = app.icon.as_ref()?;
                let (w, h) = (
                    usize::try_from(icon.width).ok()?,
                    usize::try_from(icon.height).ok()?,
                );
                if w == 0 || h == 0 || w > 32 || h > 32 {
                    return None;
                }
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&icon.rgba_base64)
                    .ok()?;
                if bytes.len() != w.checked_mul(h)?.checked_mul(4)? {
                    return None;
                }
                Some(ctx.load_texture(
                    format!("open-app-{token}-{index}"),
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &bytes),
                    egui::TextureOptions::LINEAR,
                ))
            })
            .collect();
    }
    pub(super) fn open_apps_tick(&mut self, ctx: &egui::Context) {
        if let Some(job_id) = self.open_apps.job {
            let result = self.call(Command::PollJob { job_id });
            match result["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(16))
                }
                Some("succeeded") => {
                    self.open_apps.job = None;
                    self.open_apps_accept(&result["result"], ctx);
                }
                _ => {
                    self.open_apps.job = None;
                    self.message = result["error"]["message"]
                        .as_str()
                        .unwrap_or("Не удалось найти приложения")
                        .into();
                    self.close_open_apps();
                }
            }
        }
    }
    pub(super) fn open_apps_panel(&mut self, ctx: &egui::Context) {
        if !self.open_apps.open {
            return;
        }
        let state = &mut self.open_apps;
        let query = state.query.to_lowercase();
        let mut matches: Vec<usize> = state
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| app.name.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();
        matches.sort_by_key(|index| !state.apps[*index].recommended);
        let fallback = "Другое приложение…".to_lowercase().contains(&query);
        let count = matches.len() + usize::from(fallback && state.token.is_some());
        state.selected = state.selected.min(count.saturating_sub(1));
        let previous = state.selected;
        let enter = ctx.input_mut(|i| {
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                state.selected = (state.selected + 1) % count;
            }
            if count > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                state.selected = (state.selected + count - 1) % count;
            }
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
        });
        state.scroll |= previous != state.selected || state.focus;
        let width = (ctx.content_rect().width() - 28.0).clamp(240.0, 420.0);
        let mut chosen: Option<Option<String>> = None;
        let result = egui::Modal::new(egui::Id::new("open-apps"))
            .area(
                egui::Modal::default_area(egui::Id::new("open-apps"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-14.0, -58.0]),
            )
            .backdrop_color(Color32::from_black_alpha(24))
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(width);
                let title = format!("Открыть с помощью · {}", state.name);
                ui.add(egui::Label::new(RichText::new(&title).strong().size(13.0)).truncate())
                    .on_hover_text(title);
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 180.0).clamp(80.0, 320.0))
                    .show(ui, |ui| {
                        if state.job.is_some() {
                            ui.label("Поиск приложений…");
                        }
                        let mut previous_group = None;
                        for (row, index) in matches.iter().copied().enumerate() {
                            let app = &state.apps[index];
                            if previous_group != Some(app.recommended) {
                                previous_group = Some(app.recommended);
                                ui.add_space(3.0);
                                ui.label(
                                    RichText::new(if app.recommended {
                                        "Рекомендуемые"
                                    } else {
                                        "Другие приложения"
                                    })
                                    .size(11.0)
                                    .color(Color32::from_gray(185)),
                                );
                            }
                            let response = crate::action_row::ActionRow::new(
                                crate::action_row::ActionPresentation {
                                    label: &app.name,
                                    icon: egui_lucide::Lucide::AppWindow,
                                    primary_shortcut: "",
                                    full_shortcut_tooltip: &app.name,
                                },
                            )
                            .with_texture(
                                state
                                    .textures
                                    .get(index)
                                    .and_then(|t| t.as_ref())
                                    .map(|t| t.id()),
                            )
                            .show(ui, row == state.selected);
                            if row == state.selected && state.scroll {
                                response.scroll_to_me(None);
                                state.scroll = false;
                            }
                            if response.clicked() {
                                chosen = Some(Some(app.id.clone()));
                            }
                        }
                        if fallback && state.token.is_some() {
                            let row = matches.len();
                            let response = crate::action_row::ActionRow::new(
                                crate::action_row::ActionPresentation {
                                    label: "Другое приложение…",
                                    icon: egui_lucide::Lucide::AppWindow,
                                    primary_shortcut: "",
                                    full_shortcut_tooltip: "Выбрать приложение в Windows",
                                },
                            )
                            .show(ui, row == state.selected);
                            if row == state.selected && state.scroll {
                                response.scroll_to_me(None);
                                state.scroll = false;
                            }
                            if response.clicked() {
                                chosen = Some(None);
                            }
                        } else if count == 0 && state.job.is_none() {
                            ui.label("Ничего не найдено");
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut state.query)
                            .hint_text("Поиск приложений…")
                            .frame(egui::Frame::NONE)
                            .desired_width(width - 65.0),
                    );
                    if state.focus {
                        r.request_focus();
                        state.focus = false;
                    }
                    if r.changed() {
                        state.selected = 0;
                        state.scroll = true;
                    }
                    ui_style::keycap(ui, "Esc");
                });
            });
        if enter && chosen.is_none() && count > 0 {
            chosen = Some(
                matches
                    .get(state.selected)
                    .map(|index| state.apps[*index].id.clone()),
            );
        }
        if let Some(app_id) = chosen {
            if let Some(token) = state.token.clone()
                && self.start_job(Command::OpenWithApp { token, app_id }, false, ctx.clone())
            {
                self.close_open_apps();
            }
        } else if result.should_close() {
            self.close_open_apps();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katran::app_handlers::{HandlerIcon, Provider};
    use std::sync::{Arc, Mutex};

    struct Fake(Arc<Mutex<Vec<String>>>);
    impl Provider for Fake {
        fn discover(&self, _: &str) -> katran::Result<Vec<AppHandler>> {
            Ok(vec![
                AppHandler {
                    id: "alpha".into(),
                    name: "Alpha".into(),
                    recommended: true,
                    icon: Some(HandlerIcon {
                        width: 1,
                        height: 1,
                        rgba_base64: "/////w==".into(),
                    }),
                },
                AppHandler {
                    id: "beta".into(),
                    name: "Beta".into(),
                    recommended: false,
                    icon: None,
                },
            ])
        }
        fn invoke(&self, _: &str, id: &str, _: &std::path::Path) -> katran::Result<()> {
            self.0.lock().unwrap().push(id.into());
            Ok(())
        }
    }
    fn frame(app: &mut Shelf, ctx: &egui::Context, key: egui::Key) {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(940.0, 680.0),
                )),
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_ui| app.open_apps_panel(ctx),
        );
        out.textures_delta.clear();
    }
    #[test]
    fn picker_filters_navigates_launches_and_releases_session() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let calls = Arc::new(Mutex::new(Vec::new()));
        app.api
            .as_mut()
            .unwrap()
            .set_app_provider(Box::new(Fake(calls.clone())));
        let id = app.call(Command::IngestBytes {
            name: "one.txt".into(),
            bytes: b"one".to_vec(),
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        app.open_apps_request(&ctx);
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.open_apps.job.is_some() && Instant::now() < deadline {
            app.open_apps_tick(&ctx);
            std::thread::yield_now();
        }
        assert_eq!(app.open_apps.apps.len(), 2);
        assert_eq!(app.open_apps.textures.iter().flatten().count(), 1);
        let token = app.open_apps.token.clone().unwrap();
        app.open_apps.query = "beta".into();
        frame(&mut app, &ctx, egui::Key::Enter);
        assert!(!app.open_apps.open);
        assert!(app.open_apps.textures.is_empty());
        while app.job.is_some() && Instant::now() < deadline {
            app.finish_job();
            std::thread::yield_now();
        }
        assert_eq!(*calls.lock().unwrap(), vec!["beta"]);
        assert_eq!(
            app.call(Command::ReleaseOpenApps { token })["released"],
            false
        );
    }
    #[test]
    fn picker_escape_releases_session_and_textures() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let calls = Arc::new(Mutex::new(Vec::new()));
        app.api
            .as_mut()
            .unwrap()
            .set_app_provider(Box::new(Fake(calls)));
        let id = app.call(Command::IngestBytes {
            name: "one.txt".into(),
            bytes: b"one".to_vec(),
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        app.open_apps_request(&ctx);
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.open_apps.job.is_some() && Instant::now() < deadline {
            app.open_apps_tick(&ctx);
            std::thread::yield_now();
        }
        let token = app.open_apps.token.clone().unwrap();
        frame(&mut app, &ctx, egui::Key::ArrowDown);
        assert_eq!(app.open_apps.selected, 1);
        frame(&mut app, &ctx, egui::Key::Escape);
        assert!(!app.open_apps.open);
        assert!(app.open_apps.textures.is_empty());
        assert_eq!(
            app.call(Command::ReleaseOpenApps { token })["released"],
            false
        );
    }
    #[test]
    fn shortcut_opens_picker_and_enter_launches_filtered_app() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let calls = Arc::new(Mutex::new(Vec::new()));
        app.api
            .as_mut()
            .unwrap()
            .set_app_provider(Box::new(Fake(calls.clone())));
        let id = app.call(Command::IngestBytes {
            name: "one.txt".into(),
            bytes: b"one".to_vec(),
            source: "test".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.refresh();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let input = |key, modifiers| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(940.0, 680.0),
            )),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(
            input(
                egui::Key::Enter,
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            ),
            |ui| app.render(ui),
        );
        out.textures_delta.clear();
        assert!(app.open_apps.open);
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.open_apps.job.is_some() && Instant::now() < deadline {
            app.open_apps_tick(&ctx);
            std::thread::yield_now();
        }
        app.open_apps.query = "beta".into();
        let mut out = ctx.run_ui(input(egui::Key::Enter, egui::Modifiers::NONE), |ui| {
            app.render(ui)
        });
        out.textures_delta.clear();
        while app.job.is_some() && Instant::now() < deadline {
            app.finish_job();
            std::thread::yield_now();
        }
        assert_eq!(*calls.lock().unwrap(), vec!["beta"]);
        assert!(!app.open_apps.open);
    }
}

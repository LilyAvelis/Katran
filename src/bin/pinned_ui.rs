use super::*;
use katran::domain_jobs::JobId;
#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    id: Option<i64>,
    tag_mode: bool,
    name: String,
    tags: String,
    opener: Option<String>,
    apps: Vec<(String, String)>,
    token: Option<String>,
    job: Option<JobId>,
    rows: Vec<(String, String)>,
    selected: usize,
    error: String,
}
impl Shelf {
    fn pinned_close(&mut self) {
        if let Some(job_id) = self.pinned_ui.job.take() {
            self.call(Command::CancelJob { job_id });
        }
        if let Some(token) = self.pinned_ui.token.take() {
            self.call(Command::ReleaseOpenApps { token });
        }
        self.pinned_ui = Default::default();
    }
    pub(super) fn pinned_input(&mut self, ctx: &egui::Context) {
        if self.pinned_ui.open
            || self.workspace.open
            || self.actions
            || self.open_apps.open
            || self.save
            || self.shortcut_editor.is_some()
            || !self.pending_delete.is_empty()
            || self.state["settings_open"] == true
            || self.state["tab"] == "folder"
            || ctx
                .memory(|m| m.focused())
                .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::E)) {
            let Some(id) = self.selected_ids().first().copied() else {
                return;
            };
            let result = self.call(Command::GetPinnedProperties { id });
            if result["properties"]["pinned"] != true {
                self.message = "Свойства доступны закреплённым объектам".into();
                return;
            }
            self.pinned_ui = State {
                open: true,
                id: Some(id),
                name: result["properties"]["display_name"]
                    .as_str()
                    .unwrap_or("")
                    .into(),
                opener: result["properties"]["opener"].as_str().map(str::to_owned),
                tags: result["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t["display"].as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                ..Default::default()
            };
            let result = self.call(Command::BeginJob {
                command: Box::new(Command::ListOpenApps { id }),
            });
            if result["state"] == "succeeded" {
                self.pinned_accept_apps(&result["result"]);
            } else if let Some(id) = result["job_id"].as_u64() {
                self.pinned_ui.job = Some(JobId(id));
                ctx.request_repaint();
            } else {
                self.pinned_ui.error = self.message.clone();
            }
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::T)) {
            if self.state["pinned_only"] != true {
                let state = self.call(Command::FilterPinnedTag { tag: None });
                if state.is_null() {
                    return;
                }
                self.state = state;
                self.refresh();
            }
            let result = self.call(Command::PinnedTags);
            if result.is_null() {
                return;
            }
            let rows = result["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| Some((v["key"].as_str()?.into(), v["display"].as_str()?.into())))
                .collect();
            self.pinned_ui = State {
                open: true,
                tag_mode: true,
                rows,
                ..Default::default()
            };
        }
    }
    fn pinned_accept_apps(&mut self, result: &Value) {
        self.pinned_ui.token = result["token"].as_str().map(str::to_owned);
        self.pinned_ui.apps = result["apps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| Some((v["id"].as_str()?.into(), v["name"].as_str()?.into())))
            .collect();
    }
    pub(super) fn pinned_tick(&mut self, ctx: &egui::Context) {
        if !self.pinned_ui.open {
            return;
        }
        if let Some(id) = self.pinned_ui.id
            && !self.selected_ids().contains(&id)
        {
            self.pinned_close();
            return;
        }
        if let Some(job_id) = self.pinned_ui.job {
            let result = self.call(Command::PollJob { job_id });
            match result["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(50))
                }
                Some("succeeded") => {
                    self.pinned_ui.job = None;
                    self.pinned_accept_apps(&result["result"]);
                }
                _ => {
                    self.pinned_ui.job = None;
                    self.pinned_ui.error = result["error"]["message"]
                        .as_str()
                        .unwrap_or("Не удалось найти приложения")
                        .into();
                }
            }
        }
    }
    pub(super) fn pinned_panel(&mut self, ctx: &egui::Context) {
        if !self.pinned_ui.open {
            return;
        }
        let state = &mut self.pinned_ui;
        let mut picked = None;
        if state.tag_mode {
            let n = state.rows.len();
            ctx.input_mut(|i| {
                if n > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    state.selected = (state.selected + 1) % n;
                }
                if n > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    state.selected = (state.selected + n - 1) % n;
                }
                if n > 0 && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    picked = Some(state.rows[state.selected].0.clone());
                }
            });
        }
        let mut save = false;
        let modal = egui::Modal::new(egui::Id::new("pinned-properties"))
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 28.0).clamp(240.0, 480.0));
                if state.tag_mode {
                    ui.heading("Теги закреплённых");
                    for (index, (key, label)) in state.rows.iter().enumerate() {
                        if ui
                            .selectable_label(index == state.selected, label)
                            .clicked()
                        {
                            picked = Some(key.clone());
                        }
                    }
                    if state.rows.is_empty() {
                        ui.weak("Тегов пока нет");
                    }
                    ui.weak("Выберите тег повторно, чтобы снять фильтр");
                } else {
                    ui.heading("Свойства закреплённого объекта");
                    ui.label("Имя");
                    ui.text_edit_singleline(&mut state.name);
                    ui.label("Теги через запятую");
                    ui.text_edit_singleline(&mut state.tags);
                    ui.label("Приложение открытия");
                    let label = state
                        .opener
                        .as_ref()
                        .and_then(|id| state.apps.iter().find(|a| &a.0 == id).map(|a| a.1.as_str()))
                        .unwrap_or(if state.opener.is_some() {
                            "Сохранённое приложение"
                        } else {
                            "По умолчанию"
                        });
                    egui::ComboBox::from_id_salt("pinned-opener")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.opener, None, "По умолчанию");
                            for (id, name) in &state.apps {
                                ui.selectable_value(&mut state.opener, Some(id.clone()), name);
                            }
                        });
                    if state.job.is_some() {
                        ui.weak("Поиск приложений…");
                    }
                    if !state.error.is_empty() {
                        ui.colored_label(Color32::LIGHT_RED, &state.error);
                    }
                    ui.weak("Имя и теги сохраняются после снятия закрепления.");
                    save = ui.button("Сохранить").clicked();
                }
            });
        if let Some(tag) = picked {
            let result = self.call(Command::FilterPinnedTag { tag: Some(tag) });
            if !result.is_null() {
                self.state = result;
                self.pinned_close();
                self.refresh();
            }
        } else if save {
            let id = self.pinned_ui.id.unwrap();
            let name = self.pinned_ui.name.trim().to_owned();
            let opener = self.pinned_ui.opener.as_ref().map(PathBuf::from);
            let tags = self
                .pinned_ui
                .tags
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let a = self.call(Command::SetPinnedProperties {
                id,
                display_name: (!name.is_empty()).then_some(name),
                opener,
            });
            if !a.is_null() {
                let b = self.call(Command::SetPinnedTags { id, tags });
                if !b.is_null() {
                    self.pinned_close();
                    self.refresh();
                } else {
                    self.pinned_ui.error = self.message.clone();
                }
            } else {
                self.pinned_ui.error = self.message.clone();
            }
        } else if modal.should_close() {
            self.pinned_close();
        }
    }
}

use super::i18n::tr;
use super::*;
use katran::domain_jobs::JobId;
#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    id: Option<i64>,
    name: String,
    opener: Option<String>,
    apps: Vec<(String, String)>,
    token: Option<String>,
    job: Option<JobId>,
    error: String,
    focus_name: bool,
    collection_revision: Option<String>,
}
impl Shelf {
    pub(super) fn pinned_close(&mut self) {
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
            || self.upscale.collection_open()
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
            let result = if id > 0 {
                self.call(Command::GetPinnedProperties { id })
            } else {
                Value::Null
            };
            if result["properties"]["pinned"] != true {
                let event = self.call(Command::Get { id });
                if super::images::is_bull(&event) || super::images::is_bs(&event) {
                    let info = self.call(Command::CollectionRenameInfo { id });
                    if let (Some(name), Some(revision)) =
                        (info["name"].as_str(), info["revision"].as_str())
                    {
                        self.pinned_ui = State {
                            open: true,
                            focus_name: true,
                            id: Some(id),
                            name: name.into(),
                            collection_revision: Some(revision.into()),
                            ..Default::default()
                        };
                    }
                    return;
                }
                self.message = tr(
                    "Ctrl+E доступен коллекциям и закреплённым объектам",
                    "Ctrl+E is available for collections and pinned items",
                )
                .into();
                return;
            }
            self.pinned_ui = State {
                open: true,
                focus_name: true,
                id: Some(id),
                name: result["properties"]["display_name"]
                    .as_str()
                    .unwrap_or("")
                    .into(),
                opener: result["properties"]["opener"].as_str().map(str::to_owned),
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
        if (self.pinned_ui.collection_revision.is_none() || self.pinned_ui.job.is_none())
            && let Some(id) = self.pinned_ui.id
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
                    if self.pinned_ui.collection_revision.is_some() {
                        self.pinned_close_modal(ctx);
                        self.state = self.call(Command::Interaction);
                        self.refresh();
                    } else {
                        self.pinned_accept_apps(&result["result"]);
                    }
                }
                _ => {
                    self.pinned_ui.job = None;
                    let fallback = if self.pinned_ui.collection_revision.is_some() {
                        self.pinned_ui.focus_name = true;
                        tr(
                            "Не удалось переименовать коллекцию",
                            "Could not rename collection",
                        )
                    } else {
                        tr("Не удалось найти приложения", "Could not find apps")
                    };
                    self.pinned_ui.error = result["error"]["message"]
                        .as_str()
                        .unwrap_or(fallback)
                        .into();
                }
            }
        }
    }

    fn pinned_close_modal(&mut self, ctx: &egui::Context) {
        self.pinned_close();
        egui::Popup::close_all(ctx);
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }
    pub(super) fn pinned_panel(&mut self, ctx: &egui::Context) {
        if !self.pinned_ui.open {
            return;
        }
        let mut save = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
        let mut cancel = false;
        let state = &mut self.pinned_ui;
        let collection = state.collection_revision.is_some();
        let saving = collection && state.job.is_some();
        let mut optical = None;
        let modal = egui::Modal::new(egui::Id::new("pinned-properties"))
            .frame(crate::action_row::popup_frame(ctx))
            .show(ctx, |ui| {
                optical = Some((ui.painter().clone(), ui.painter().add(egui::Shape::Noop)));
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(1.0, 420.0));
                ui.heading(if collection {
                    tr("Переименовать коллекцию", "Rename collection")
                } else {
                    tr("Свойства закреплённого объекта", "Pinned item properties")
                });
                ui.add_space(super::ui_foundation::tokens::SM);
                ui.label(tr("Имя", "Name"));
                let mut name =
                    super::ui_foundation::text_field(egui::TextEdit::singleline(&mut state.name))
                        .interactive(!saving)
                        .id_salt("pinned-name")
                        .desired_width(ui.available_width())
                        .show(ui);
                #[cfg(test)]
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new("pinned-name-rect"), name.response.rect)
                });
                if state.focus_name {
                    name.response.request_focus();
                    name.state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(state.name.chars().count()),
                        )));
                    name.state.store(ctx, name.response.id);
                    state.focus_name = false;
                }
                if collection
                    && name.response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none())
                {
                    save = true;
                }
                if !collection {
                    ui.add_space(super::ui_foundation::tokens::SM);
                    ui.label(tr("Приложение открытия", "Open with"));
                    let label = state
                        .opener
                        .as_ref()
                        .and_then(|id| state.apps.iter().find(|a| &a.0 == id).map(|a| a.1.as_str()))
                        .unwrap_or(if state.opener.is_some() {
                            tr("Сохранённое приложение", "Saved app")
                        } else {
                            tr("По умолчанию", "Default")
                        });
                    egui::ComboBox::from_id_salt("pinned-opener")
                        .width(ui.available_width())
                        .selected_text(label)
                        .truncate()
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.opener,
                                None,
                                tr("По умолчанию", "Default"),
                            );
                            for (id, name) in &state.apps {
                                ui.selectable_value(&mut state.opener, Some(id.clone()), name);
                            }
                        })
                        .response
                        .on_hover_text(label);
                    if state.job.is_some() {
                        ui.weak(tr("Поиск приложений…", "Searching for apps…"));
                    }
                    ui.weak(tr(
                        "Имя сохраняется после снятия закрепления.",
                        "The name is retained after unpinning.",
                    ));
                }
                if !state.error.is_empty() {
                    ui.colored_label(Color32::LIGHT_RED, &state.error);
                }
                ui.add_space(super::ui_foundation::tokens::MD);
                let show_keys = ui.available_width() > 310.0;
                ui.horizontal(|ui| {
                    cancel = ui
                        .button(tr("Отмена", "Cancel"))
                        .on_hover_text("Esc")
                        .clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        save |= ui
                            .add_enabled(
                                !saving,
                                egui::Button::new(tr("Сохранить", "Save")).selected(true),
                            )
                            .on_hover_text("Ctrl+Enter")
                            .clicked();
                        if show_keys {
                            super::ui_style::keycap(ui, "Ctrl+Enter");
                        }
                    });
                });
            });
        if let Some((painter, index)) = optical
            && let Some(shape) = super::theme_ui::optical_shape(ctx, modal.response.rect, 12.0)
        {
            painter.set(index, shape);
        }
        if cancel || modal.should_close() {
            self.pinned_close_modal(ctx);
        } else if save && !saving {
            let Some(id) = self.pinned_ui.id else {
                return;
            };
            let name = self.pinned_ui.name.trim().to_owned();
            if let Some(revision) = self.pinned_ui.collection_revision.clone() {
                let result = self.call(Command::RenameCollection { id, revision, name });
                if let Some(job_id) = result["job_id"].as_u64() {
                    self.pinned_ui.job = Some(JobId(job_id));
                    self.pinned_ui.error.clear();
                    ctx.request_repaint();
                } else {
                    self.pinned_ui.error = self.message.clone();
                    self.pinned_ui.focus_name = true;
                }
                return;
            }
            let opener = self.pinned_ui.opener.as_ref().map(PathBuf::from);
            let before = self.call(Command::Interaction);
            let a = self.call(Command::SetPinnedProperties {
                id,
                display_name: (!name.is_empty()).then_some(name),
                opener,
            });
            if !a.is_null() {
                let after = self.call(Command::Interaction);
                self.pinned_close_modal(ctx);
                self.accept_item_metadata_update(&before, after);
            } else {
                self.pinned_ui.error = self.message.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctrl_e_renames_unpinned_collection_without_loading_openers() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        let tab = katran::interaction::ShelfTab::Files;
        app.call(Command::SetTabEnabled { tab, enabled: true });
        app.call(Command::SetTab { tab });
        let event = app.call(Command::IngestBytes {
            name: "Old.bs".into(),
            source: "bs_collection".into(),
            bytes: b"synthetic fixture".to_vec(),
        });
        let id = event["id"].as_i64().unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.call(Command::Select {
            id,
            mode: katran::interaction::SelectionMode::Replace,
        });
        app.state = app.call(Command::Interaction);
        let ctx = egui::Context::default();
        let input = |key| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        let _ = ctx.run_ui(input(egui::Key::E), |ui| app.pinned_input(ui.ctx()));
        assert!(app.pinned_ui.open);
        assert_eq!(app.pinned_ui.name, "Old");
        assert!(app.pinned_ui.collection_revision.is_some());
        assert!(app.pinned_ui.job.is_none());
        app.pinned_ui.name = "New".into();
        let _ = ctx.run_ui(input(egui::Key::Enter), |ui| app.pinned_panel(ui.ctx()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.pinned_ui.open {
            assert!(
                std::time::Instant::now() < deadline,
                "{}",
                app.pinned_ui.error
            );
            app.pinned_tick(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.call(Command::Get { id })["name"], "New.bs");
        assert_eq!(
            app.call(Command::GetPinnedProperties { id })["properties"]["pinned"],
            false
        );
    }

    fn setup() -> (tempfile::TempDir, Shelf, i64) {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::tests::test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: true,
        });
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Text,
        });
        let id = app.call(Command::IngestText {
            text: "synthetic pinned text".into(),
            source: "fixture".into(),
        })["id"]
            .as_i64()
            .unwrap();
        app.call(Command::TogglePins { ids: vec![id] });
        app.state = app.call(Command::Interaction);
        app.pinned_ui = State {
            open: true,
            id: Some(id),
            name: "New alias".into(),
            focus_name: true,
            ..Default::default()
        };
        (root, app, id)
    }
    #[test]
    fn ctrl_enter_saves_through_domain_and_consumes_key() {
        let (_root, mut app, id) = setup();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 700.0),
                )),
                ..Default::default()
            },
            |ui| app.pinned_panel(ui.ctx()),
        );
        output.textures_delta.clear();
        assert!(app.pinned_ui.open);
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("pinned-name-rect")))
            .unwrap();
        assert!(rect.width() > 390.0, "full-width input: {rect:?}");
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ui| {
                app.pinned_panel(ui.ctx());
                assert!(!ui.ctx().input(|i| i.events.iter().any(|e| matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        ..
                    }
                ))));
            },
        );
        output.textures_delta.clear();
        assert!(!app.pinned_ui.open, "{}", app.message);
        assert_eq!(
            app.call(Command::GetPinnedProperties { id })["properties"]["display_name"],
            "New alias"
        );
    }
    #[test]
    fn failed_ctrl_enter_keeps_draft_and_plain_enter_does_not_save() {
        let (_root, mut app, id) = setup();
        let ctx = egui::Context::default();
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::CTRL] {
            if modifiers.ctrl {
                app.pinned_ui.id = Some(i64::MAX);
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: Some(egui::Key::Enter),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ui| app.pinned_panel(ui.ctx()),
            );
            output.textures_delta.clear();
            assert!(app.pinned_ui.open);
            assert_eq!(app.pinned_ui.name, "New alias");
        }
        assert!(!app.pinned_ui.error.is_empty());
        assert_ne!(
            app.call(Command::GetPinnedProperties { id })["properties"]["display_name"],
            "New alias"
        );
    }
}

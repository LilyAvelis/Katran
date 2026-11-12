//! Manual update controls. The dispatcher owns network and installation work.
use super::i18n::tr;
use eframe::egui;
use katran::{Command, Request, Service, domain_jobs::JobId};
use serde_json::Value;
use std::time::Duration;

#[derive(Default)]
pub struct State {
    status: Value,
    initialized: bool,
    job: Option<JobId>,
    installing: bool,
    error: String,
}

fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "settings-updates".into(),
        command,
    })
}

fn status_value(value: &Value) -> Option<Value> {
    matches!(
        value["state"].as_str(),
        Some(
            "not_checked"
                | "available"
                | "up_to_date"
                | "unavailable"
                | "unsupported"
                | "installed"
        )
    )
    .then(|| value.clone())
}

fn request_error(response: &Value, installing: bool) -> String {
    if installing {
        tr(
            "Не удалось установить обновление. Попробуйте ещё раз.",
            "Could not install the update. Try again.",
        )
        .into()
    } else if response["error"]["code"] == "update_network_error" {
        tr(
            "Не удалось проверить обновления. Проверьте подключение и попробуйте ещё раз.",
            "Could not check for updates. Check your connection and try again.",
        )
        .into()
    } else {
        tr(
            "Не удалось проверить обновления. Попробуйте ещё раз.",
            "Could not check for updates. Try again.",
        )
        .into()
    }
}

impl State {
    fn read_status(&mut self, api: &mut Service) {
        self.initialized = true;
        let response = call(api, Command::GetUpdateStatus);
        if response["ok"] == true {
            self.status = response["result"].clone();
        } else {
            self.error = tr(
                "Не удалось прочитать состояние обновлений.",
                "Could not read update status.",
            )
            .into();
        }
    }

    fn begin(&mut self, api: &mut Service, ctx: &egui::Context, installing: bool) {
        self.error.clear();
        let command = if installing {
            Command::InstallUpdate
        } else {
            Command::CheckUpdates
        };
        let response = call(
            api,
            Command::BeginJob {
                command: Box::new(command),
            },
        );
        if response["ok"] != true {
            self.error = request_error(&response, installing);
            return;
        }
        let result = &response["result"];
        if let Some(id) = result["job_id"].as_u64() {
            self.job = Some(JobId(id));
            self.installing = installing;
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }

        // Keep the client defensive if the dispatcher completes an operation
        // inline in a future API version.
        if let Some(status) = status_value(result).or_else(|| status_value(&result["result"]))
            && (!installing || status["state"] == "installed")
        {
            self.status = status;
            self.error.clear();
            return;
        }
        self.error = if installing {
            tr(
                "Не удалось запустить установку обновления.",
                "Could not start installing the update.",
            )
            .into()
        } else {
            tr(
                "Не удалось запустить проверку обновлений.",
                "Could not start checking for updates.",
            )
            .into()
        };
    }

    /// Polls an owned job even while Settings is closed.
    pub fn tick(&mut self, api: &mut Service, ctx: &egui::Context) {
        let Some(job_id) = self.job else {
            return;
        };
        let response = call(api, Command::PollJob { job_id });
        if response["ok"] != true {
            self.error = if self.installing {
                tr(
                    "Не удалось получить результат установки.",
                    "Could not retrieve the installation result.",
                )
                .into()
            } else {
                tr(
                    "Не удалось получить результат проверки.",
                    "Could not retrieve the update check result.",
                )
                .into()
            };
            self.job = None;
            return;
        }
        let result = &response["result"];
        match result["state"].as_str() {
            Some("queued" | "running" | "ready") => {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Some("succeeded") => {
                if let Some(status) = status_value(&result["result"]) {
                    self.status = status;
                    self.error.clear();
                } else {
                    self.error = tr(
                        "Операция завершилась без состояния обновлений.",
                        "The operation finished without an update status.",
                    )
                    .into();
                }
                self.job = None;
                self.installing = false;
            }
            Some("cancelled") => {
                self.error = tr("Операция отменена.", "Operation cancelled.").into();
                self.job = None;
                self.installing = false;
            }
            Some("failed") => {
                self.error = if self.installing {
                    tr(
                        "Не удалось установить обновление. Попробуйте ещё раз.",
                        "Could not install the update. Try again.",
                    )
                    .into()
                } else {
                    tr(
                        "Не удалось проверить обновления. Попробуйте ещё раз.",
                        "Could not check for updates. Try again.",
                    )
                    .into()
                };
                self.job = None;
                self.installing = false;
            }
            _ => {
                self.error = tr(
                    "Не удалось прочитать состояние операции обновления.",
                    "Could not read the update operation status.",
                )
                .into();
                self.job = None;
                self.installing = false;
            }
        }
    }

    fn cancel(&mut self, api: &mut Service, ctx: &egui::Context) {
        let Some(job_id) = self.job else {
            return;
        };
        let response = call(api, Command::CancelJob { job_id });
        if response["ok"] != true {
            self.error = tr(
                "Операция уже выполняется и не была отменена.",
                "The operation is already running and was not cancelled.",
            )
            .into();
        } else {
            self.error.clear();
        }
        // Poll after cancellation so a race with the install commit is reported
        // by its actual terminal status rather than as a claimed rollback.
        self.tick(api, ctx);
    }

    fn status_label(&self) -> String {
        match self.status["state"].as_str().unwrap_or("") {
            "not_checked" => tr(
                "Проверка обновлений ещё не выполнялась.",
                "Updates have not been checked yet.",
            )
            .into(),
            "available" => format!(
                "{} {}.",
                tr("Доступна версия", "Available version:"),
                self.status["latest_version"]
                    .as_str()
                    .unwrap_or(tr("новая", "new"))
            ),
            "up_to_date" => format!(
                "{} {}.",
                tr(
                    "Установлена актуальная версия",
                    "Up to date; installed version:"
                ),
                self.status["current_version"].as_str().unwrap_or("")
            ),
            "unavailable" => tr(
                "Сейчас нет доступной публичной версии.",
                "No public version is currently available.",
            )
            .into(),
            "unsupported" => tr(
                "Эта установка не поддерживает обновление.",
                "This installation does not support updates.",
            )
            .into(),
            "installed" => {
                let version = self.status["installed"]["version"]
                    .as_str()
                    .or_else(|| self.status["current_version"].as_str())
                    .unwrap_or("");
                if self.status["installed"]["restart_required"] == true {
                    if super::i18n::is_english() {
                        format!("Version {version} installed. Restart Katran manually.")
                    } else {
                        format!("Версия {version} установлена. Перезапустите Katran вручную.")
                    }
                } else {
                    if super::i18n::is_english() {
                        format!("Version {version} installed.")
                    } else {
                        format!("Версия {version} установлена.")
                    }
                }
            }
            _ => tr(
                "Состояние обновлений пока неизвестно.",
                "Update status is not yet known.",
            )
            .into(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, api: &mut Service, ctx: &egui::Context) {
        if !self.initialized {
            self.read_status(api);
        }
        ui.heading(tr("Обновления", "Updates"));
        ui.label(self.status_label());

        let busy = self.job.is_some();
        let installed = self.status["state"] == "installed";
        if busy {
            ui.horizontal(|ui| {
                ui.label(if self.installing {
                    tr("Скачивание и установка…", "Downloading and installing…")
                } else {
                    tr("Проверка обновлений…", "Checking for updates…")
                });
                if ui.button(tr("Отменить", "Cancel")).clicked() {
                    self.cancel(api, ctx);
                }
            });
        } else {
            if !installed
                && ui
                    .button(tr("Проверить обновления", "Check for updates"))
                    .clicked()
            {
                self.begin(api, ctx, false);
            }
            if self.status["state"] == "available"
                && ui
                    .button(tr("Скачать и установить", "Download and install"))
                    .clicked()
            {
                self.begin(api, ctx, true);
            }
        }

        if !self.error.is_empty() {
            ui.add_space(8.0);
            ui.weak(&self.error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katran::{
        Result,
        updater::{HttpResponse, Transport, Updater},
    };
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone, Default)]
    struct CountingTransport(Arc<AtomicUsize>);
    impl Transport for CountingTransport {
        fn get(&self, _url: &str, _max_bytes: usize) -> Result<HttpResponse> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(HttpResponse {
                status: 404,
                body: Vec::new(),
            })
        }
    }

    fn output_text(output: &egui::FullOutput) -> String {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn render(state: &mut State, api: &mut Service, ctx: &egui::Context) -> String {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 500.0),
                )),
                ..Default::default()
            },
            |ui| state.show(ui, api, ctx),
        );
        let text = output_text(&output);
        output.textures_delta.clear();
        text
    }

    fn fixture() -> (tempfile::TempDir, Service, CountingTransport) {
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), katran::DEFAULT_QUOTA).unwrap();
        let transport = CountingTransport::default();
        api.set_updater(
            Updater::new(None, Arc::new(transport.clone()), env!("CARGO_PKG_VERSION")).unwrap(),
        );
        (root, api, transport)
    }

    #[test]
    fn first_show_is_manual_and_non_installable_states_have_no_install_button() {
        let (_root, mut api, transport) = fixture();
        let ctx = egui::Context::default();
        let mut state = State::default();
        let text = render(&mut state, &mut api, &ctx);
        assert!(text.contains("Проверить обновления"));
        assert!(!text.contains("Скачать и установить"));
        assert_eq!(state.status["state"], "not_checked");
        assert_eq!(transport.0.load(Ordering::Relaxed), 0);
        assert!(state.job.is_none());

        for status in ["unsupported", "up_to_date"] {
            state.status = json!({"state":status,"current_version":"1.5.0"});
            let text = render(&mut state, &mut api, &ctx);
            assert!(!text.contains("Скачать и установить"));
        }
        assert_eq!(transport.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn installed_status_asks_for_a_manual_restart() {
        let (_root, mut api, transport) = fixture();
        let ctx = egui::Context::default();
        let mut state = State {
            status: json!({
                "state":"installed",
                "installed":{"version":"2.0.0","restart_required":true}
            }),
            initialized: true,
            ..Default::default()
        };
        let text = render(&mut state, &mut api, &ctx);
        assert!(text.contains("Перезапустите Katran вручную"));
        assert!(!text.contains("Скачать и установить"));
        assert_eq!(transport.0.load(Ordering::Relaxed), 0);
    }
}

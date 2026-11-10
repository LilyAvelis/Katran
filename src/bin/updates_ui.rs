//! Manual update controls. The dispatcher owns network and installation work.
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
        "Не удалось установить обновление. Попробуйте ещё раз.".into()
    } else if response["error"]["code"] == "update_network_error" {
        "Не удалось проверить обновления. Проверьте подключение и попробуйте ещё раз.".into()
    } else {
        "Не удалось проверить обновления. Попробуйте ещё раз.".into()
    }
}

impl State {
    fn read_status(&mut self, api: &mut Service) {
        self.initialized = true;
        let response = call(api, Command::GetUpdateStatus);
        if response["ok"] == true {
            self.status = response["result"].clone();
        } else {
            self.error = "Не удалось прочитать состояние обновлений.".into();
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
            "Не удалось запустить установку обновления.".into()
        } else {
            "Не удалось запустить проверку обновлений.".into()
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
                "Не удалось получить результат установки.".into()
            } else {
                "Не удалось получить результат проверки.".into()
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
                    self.error = "Операция завершилась без состояния обновлений.".into();
                }
                self.job = None;
                self.installing = false;
            }
            Some("cancelled") => {
                self.error = "Операция отменена.".into();
                self.job = None;
                self.installing = false;
            }
            Some("failed") => {
                self.error = if self.installing {
                    "Не удалось установить обновление. Попробуйте ещё раз.".into()
                } else {
                    "Не удалось проверить обновления. Попробуйте ещё раз.".into()
                };
                self.job = None;
                self.installing = false;
            }
            _ => {
                self.error = "Не удалось прочитать состояние операции обновления.".into();
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
            self.error = "Операция уже выполняется и не была отменена.".into();
        } else {
            self.error.clear();
        }
        // Poll after cancellation so a race with the install commit is reported
        // by its actual terminal status rather than as a claimed rollback.
        self.tick(api, ctx);
    }

    fn status_label(&self) -> String {
        match self.status["state"].as_str().unwrap_or("") {
            "not_checked" => "Проверка обновлений ещё не выполнялась.".into(),
            "available" => format!(
                "Доступна версия {}.",
                self.status["latest_version"].as_str().unwrap_or("новая")
            ),
            "up_to_date" => format!(
                "Установлена актуальная версия {}.",
                self.status["current_version"].as_str().unwrap_or("")
            ),
            "unavailable" => "Сейчас нет доступной публичной версии.".into(),
            "unsupported" => "Эта установка не поддерживает обновление.".into(),
            "installed" => {
                let version = self.status["installed"]["version"]
                    .as_str()
                    .or_else(|| self.status["current_version"].as_str())
                    .unwrap_or("");
                if self.status["installed"]["restart_required"] == true {
                    format!("Версия {version} установлена. Перезапустите Katran вручную.")
                } else {
                    format!("Версия {version} установлена.")
                }
            }
            _ => "Состояние обновлений пока неизвестно.".into(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, api: &mut Service, ctx: &egui::Context) {
        if !self.initialized {
            self.read_status(api);
        }
        ui.heading("Обновления");
        ui.label(self.status_label());

        let busy = self.job.is_some();
        let installed = self.status["state"] == "installed";
        if busy {
            ui.horizontal(|ui| {
                ui.label(if self.installing {
                    "Скачивание и установка…"
                } else {
                    "Проверка обновлений…"
                });
                if ui.button("Отменить").clicked() {
                    self.cancel(api, ctx);
                }
            });
        } else {
            if !installed && ui.button("Проверить обновления").clicked() {
                self.begin(api, ctx, false);
            }
            if self.status["state"] == "available" && ui.button("Скачать и установить").clicked()
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

//! Single-writer dispatcher jobs and UI result handling.
use super::*;
impl Shelf {
    pub(super) fn start_drag(&mut self, ids: Vec<i64>, ctx: &egui::Context) {
        if self.native_drag.is_some() || self.job.is_some() {
            return;
        }
        let items: Option<Vec<katran::SelectionRef>> = ids
            .into_iter()
            .map(|id| {
                let event = self.events.iter().find(|e| e["id"].as_i64() == Some(id))?;
                Some(katran::SelectionRef {
                    id,
                    revision: event["revision"].as_str()?.to_owned(),
                })
            })
            .collect();
        let Some(items) = items else {
            self.message = "Выбор устарел; обновите полку".into();
            return;
        };
        if self.start_job(Command::DragEventsChecked { items }, false, ctx.clone()) {
            self.message = "Перетаскивание файлов…".into();
        }
    }
    pub(super) fn call(&mut self, command: Command) -> Value {
        let Some(api) = self.api.as_mut() else {
            return match command {
                Command::Interaction | Command::SetVisible { .. } => self.state.clone(),
                _ => Value::Null,
            };
        };
        let r = api.execute(Request {
            api_version: 1,
            request_id: "ui".into(),
            command,
        });
        if r["ok"] == false {
            self.message = r["error"]["message"].as_str().unwrap_or("Ошибка").into();
        }
        r["result"].clone()
    }
    pub(super) fn start_job(
        &mut self,
        command: Command,
        capture: bool,
        ctx: egui::Context,
    ) -> bool {
        let Some(mut api) = self.api.take() else {
            return false;
        };
        self.busy_text_editing = ctx.egui_wants_keyboard_input();
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let reply = api.execute(Request {
                api_version: 1,
                request_id: "ui-job".into(),
                command,
            });
            let _ = tx.send((api, reply, capture));
            ctx.request_repaint();
        });
        true
    }
    pub(super) fn finish_job(&mut self) {
        let Some(job) = self.job.as_ref() else {
            return;
        };
        let (api, reply, capture) = match job.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                self.pending_copy = None;
                self.message =
                    "Фоновая операция прервалась; копирование не выполнено. Перезапустите Katran."
                        .into();
                return;
            }
        };
        self.job = None;
        self.api = Some(api);
        if reply["ok"] == false {
            self.message = reply["error"]["message"]
                .as_str()
                .unwrap_or("Ошибка")
                .into();
            return;
        }
        let result = &reply["result"];
        #[cfg(feature = "clipboard-events")]
        if capture {
            self.clipboard_events.completed(result, Instant::now());
            if result["capture_schedule"]["retry_after_ms"].is_null() {
                if let Some(error) = result["capture_error"]["message"].as_str() {
                    self.message = format!("Не удалось захватить Clipboard: {error}");
                } else if result["capture_schedule"]["exhausted"] == true {
                    self.message = "Clipboard оставался занят или менялся; повторы исчерпаны. Скопируйте объект ещё раз.".into();
                }
            }
        }
        if let Some(token) = result["drag_token"].as_str() {
            self.native_drag = Some(token.to_owned());
            self.message = "Файлы готовы к перетаскиванию…".into();
            return;
        }
        if capture {
            let errors = item_failures(&result["items"]);
            if !errors.is_empty() {
                self.message = format!("Не удалось захватить: {errors}");
            }
            if clipboard_changed(result) {
                self.refresh();
            }
        } else {
            self.action_result(result.clone());
            self.refresh();
        }
    }
    pub(super) fn refresh(&mut self) {
        let target = if self.query == self.loaded_query {
            self.events.len().max(1000)
        } else {
            1000
        };
        let r = self.call(Command::SearchPage {
            query: self.query.clone(),
            limit: 1000,
            after_id: None,
        });
        if let Some(events) = r["events"].as_array() {
            self.events = events.clone();
            self.next_cursor = r["next_cursor"].as_i64();
            self.loaded_query = self.query.clone();
            while self.events.len() < target && self.next_cursor.is_some() {
                let previous = self.events.len();
                self.load_more();
                if self.events.len() == previous {
                    break;
                }
            }
        }
        self.capture = self.call(Command::Status)["capture_enabled"] == true;
    }
    pub(super) fn load_more(&mut self) {
        let Some(cursor) = self.next_cursor else {
            return;
        };
        let r = self.call(Command::SearchPage {
            query: self.query.clone(),
            limit: 1000,
            after_id: Some(cursor),
        });
        if let Some(events) = r["events"].as_array() {
            self.events.extend(events.iter().cloned());
            self.next_cursor = r["next_cursor"].as_i64();
        }
    }
    pub(super) fn selection_snapshot(&self) -> Option<Vec<katran::SelectionRef>> {
        let ids: Vec<i64> = self.state["selected"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .collect();
        let ids = if ids.is_empty() {
            self.state["cursor"].as_i64().into_iter().collect()
        } else {
            ids
        };
        let items: Option<Vec<katran::SelectionRef>> = ids
            .into_iter()
            .map(|id| {
                let event = self
                    .events
                    .iter()
                    .find(|event| event["id"].as_i64() == Some(id))?;
                Some(katran::SelectionRef {
                    id,
                    revision: event["revision"].as_str()?.to_owned(),
                })
            })
            .collect();
        items
    }
    pub(super) fn copy_request(&mut self, ctx: &egui::Context) {
        if self.pending_copy.is_some() {
            self.message = "Копирование уже в очереди".into();
            return;
        }
        let Some(items) = self.selection_snapshot() else {
            self.message = "Выбор устарел. Обновите полку и выберите объекты заново.".into();
            return;
        };
        if self.api.is_some() {
            if self.start_job(Command::CopyEventsChecked { items }, false, ctx.clone()) {
                self.message = "Копирование выполняется…".into();
                self.actions = false;
            }
        } else if self.job.is_some() {
            self.pending_copy = Some(items);
            self.message = "Копирование в очереди".into();
        } else {
            self.message = "Не скопировано. Перезапустите Katran.".into();
        }
    }
    pub(super) fn take_queued_copy(&mut self) -> Option<Command> {
        self.pending_copy
            .take()
            .map(|items| Command::CopyEventsChecked { items })
    }
    pub(super) fn run(&mut self, id: String, ctx: &egui::Context) {
        if id == "copy" {
            self.copy_request(ctx);
            return;
        }
        if id == "cleanup" {
            if self.start_job(Command::RunAction { id }, false, ctx.clone()) {
                self.actions = false;
            }
            return;
        }
        if matches!(id.as_str(), "open" | "preview")
            && let Some(focus) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(focus));
        }
        let r = self.call(Command::RunAction { id });
        self.action_result(r);
        self.refresh();
    }
    pub(super) fn action_result(&mut self, r: Value) {
        if r["cancelled"] == true {
            self.message = "Перетаскивание отменено".into();
        }
        if let Some(count) = r["dragged"].as_u64() {
            self.message = format!("Перетаскивание завершено: {count}");
            let errors = item_failures(&r["failures"]);
            if !errors.is_empty() {
                self.message.push_str(&format!(" · не передано: {errors}"));
            }
            if let Some(warnings) = r["warnings"].as_array() {
                for warning in warnings.iter().filter_map(Value::as_str) {
                    self.message.push_str(&format!(" · {warning}"));
                }
            }
        }
        if let Some(bytes) = r["reclaimed_blob_bytes"].as_u64() {
            self.message = format!("Очищено временных данных: {bytes} байт");
        }
        if r["confirmation_required"] == true {
            self.pending_delete = r["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
        }
        if let Some(count) = r["deleted"].as_u64()
            && count > 0
        {
            self.message = format!("Убрано с полки: {count}{}", cleanup_message(&r));
        }
        if let Some(count) = r["copied"].as_u64() {
            self.message = format!("Скопировано: {count}");
            if let Some(failures) = r["failures"].as_array()
                && !failures.is_empty()
            {
                self.message.push_str(&format!(
                    " · не скопировано: {}",
                    item_failures(&Value::Array(failures.clone()))
                ));
            }
            if let Some(warnings) = r["warnings"].as_array()
                && !warnings.is_empty()
            {
                self.message.push_str(&format!(
                    " · {}",
                    warnings
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }
        if r["intent"] == "choose_destination" {
            self.save = true;
        }
        self.actions = false;
        if r["intent"] == "show_commands" {
            self.actions = true;
            self.focus_actions = true;
            self.action_query.clear();
            self.action_selected = 0;
        }
    }
}

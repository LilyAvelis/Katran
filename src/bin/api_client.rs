//! Single-writer dispatcher jobs and UI result handling.
use super::*;
impl Shelf {
    pub(super) fn direct_transfer_input(&mut self, ctx: &egui::Context) {
        if self.api.is_none()
            || self.modal_owns_input()
            || ctx
                .memory(|m| m.focused())
                .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
        {
            return;
        }
        let catalogue: Vec<_> = self
            .shortcuts
            .iter()
            .filter(|shortcut| {
                matches!(shortcut.id.as_str(), "transfer_selection" | "transfer_copy")
            })
            .cloned()
            .collect();
        if let Some(action) = shortcuts_ui::take_action_with_editing(ctx, &catalogue, false) {
            if self.job.is_some() {
                self.message = "Дождитесь завершения текущей операции".into();
                return;
            }
            if let Some(id) = ctx.memory(|memory| memory.focused()) {
                ctx.memory_mut(|memory| memory.surrender_focus(id));
            }
            self.run(action, ctx);
        }
    }
    pub(super) fn start_drag(&mut self, ids: Vec<i64>, ctx: &egui::Context) {
        if self.native_drag.is_some() || self.job.is_some() {
            return;
        }
        let items = self.snapshot_ids(ids);
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
        if self.api.is_none()
            || (capture && self.capture_job.is_some())
            || (!capture && self.job.is_some())
        {
            return false;
        }
        self.busy_text_editing = ctx.egui_wants_keyboard_input();
        let result = self.call(Command::BeginJob {
            command: Box::new(command),
        });
        if result["state"] == "succeeded" {
            self.apply_job_reply(
                serde_json::json!({"ok":true,"result":result["result"]}),
                capture,
            );
            return true;
        }
        let Some(id) = result["job_id"].as_u64() else {
            return false;
        };
        if capture {
            self.capture_job = Some(katran::domain_jobs::JobId(id));
        } else {
            self.job = Some(UiJob::Domain {
                job_id: katran::domain_jobs::JobId(id),
                capture: false,
            });
        }
        ctx.request_repaint();
        true
    }
    pub(super) fn preview_warmup_tick(&mut self) {
        let Some(job_id) = self.preview_warmup else {
            return;
        };
        let status = self.call(Command::PollJob { job_id });
        if matches!(
            status["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            self.preview_warmup = None;
            if status["state"] == "failed" {
                self.message = format!(
                    "QuickLook: {}",
                    status["error"]["message"]
                        .as_str()
                        .unwrap_or("Не удалось подготовить просмотр")
                );
            }
        }
    }
    fn finish_capture_job(&mut self) {
        let Some(job_id) = self.capture_job else {
            return;
        };
        let status = self.call(Command::PollJob { job_id });
        let reply = match status["state"].as_str() {
            Some("queued" | "running" | "ready") => return,
            Some("succeeded") => serde_json::json!({"ok":true,"result":status["result"]}),
            _ => serde_json::json!({"ok":false,"error":status["error"]}),
        };
        self.capture_job = None;
        self.apply_job_reply(reply, true);
    }
    pub(super) fn finish_job(&mut self) {
        self.finish_capture_job();
        let Some(job) = self.job.as_ref() else {
            return;
        };
        let (reply, capture) = match job {
            UiJob::Domain { job_id, capture } => {
                let job_id = *job_id;
                let capture = *capture;
                let status = self.call(Command::PollJob { job_id });
                match status["state"].as_str() {
                    Some("queued" | "running" | "ready") => return,
                    Some("succeeded") => (
                        serde_json::json!({"ok":true,"result":status["result"]}),
                        capture,
                    ),
                    Some("failed" | "cancelled") => (
                        serde_json::json!({"ok":false,"error":status["error"]}),
                        capture,
                    ),
                    _ => (
                        serde_json::json!({"ok":false,"error":{"message":"Потерян статус фоновой операции"}}),
                        capture,
                    ),
                }
            }
            #[cfg(test)]
            UiJob::UnavailableTransport(rx) => match rx.try_recv() {
                Ok((api, reply, capture)) => {
                    self.api = Some(api);
                    (reply, capture)
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => (
                    serde_json::json!({"ok":false,"error":{"message":"Тестовый транспорт отключён"}}),
                    false,
                ),
            },
        };
        self.job = None;
        self.apply_job_reply(reply, capture);
    }
    fn apply_job_reply(&mut self, reply: Value, capture: bool) {
        if reply["ok"] == false {
            self.message = reply["error"]["message"]
                .as_str()
                .unwrap_or("Ошибка")
                .into();
            return;
        }
        let result = &reply["result"];
        if result["backend"] == "quicklook" {
            self.action_result(result.clone());
            self.state = self.call(Command::Interaction);
            return;
        }
        if result.get("preview").is_some() && result.get("view").is_some() {
            self.state = result.clone();
            return;
        }
        if result.get("classification_changed").is_some() {
            if result["classification_changed"] == true {
                self.refresh();
            }
            return;
        }
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
        if result["scanned"] == true {
            let errors = item_failures(&result["items"]);
            if !errors.is_empty() {
                self.message = format!("Папки: {errors}");
            }
            if result["items"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["ok"] == true))
            {
                self.sources.match_dirty |= result["items"].as_array().is_some_and(|items| {
                    items.iter().any(|item| item["event"]["source"] == "sharex")
                });
                self.refresh();
            }
            return;
        }
        if capture {
            let errors = item_failures(&result["items"]);
            if !errors.is_empty() {
                self.message = format!("Не удалось захватить: {errors}");
            }
            if clipboard_changed(result) {
                self.sources.match_dirty = true;
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
        let r = self.call(Command::BrowseShelfSorted {
            query: self.query.clone(),
            limit: 1000,
            cursor: None,
        });
        if let Some(events) = r["events"].as_array() {
            self.events = events.clone();
            self.next_cursor = r["next_cursor"].as_str().map(str::to_owned);
            self.loaded_query = self.query.clone();
            while self.events.len() < target && self.next_cursor.is_some() {
                let previous = self.events.len();
                self.load_more();
                if self.events.len() == previous {
                    break;
                }
            }
        } else {
            self.events.clear();
            self.next_cursor = None;
        }
        self.capture = self.call(Command::Status)["capture_enabled"] == true;
    }
    pub(super) fn load_more(&mut self) {
        let Some(cursor) = self.next_cursor.clone() else {
            return;
        };
        let r = self.call(Command::BrowseShelfSorted {
            query: self.query.clone(),
            limit: 1000,
            cursor: Some(cursor),
        });
        if let Some(events) = r["events"].as_array() {
            self.events.extend(events.iter().cloned());
            self.next_cursor = r["next_cursor"].as_str().map(str::to_owned);
        }
    }
    pub(super) fn selected_ids(&self) -> Vec<i64> {
        let ids: Vec<i64> = self.state["selected"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .collect();
        if ids.is_empty() {
            self.state["cursor"].as_i64().into_iter().collect()
        } else {
            ids
        }
    }
    fn snapshot_ids(&mut self, ids: Vec<i64>) -> Option<Vec<katran::SelectionRef>> {
        ids.into_iter()
            .map(|id| {
                let event = self
                    .events
                    .iter()
                    .find(|event| event["id"].as_i64() == Some(id))
                    .cloned()
                    .unwrap_or_else(|| self.call(Command::Get { id }));
                Some(katran::SelectionRef {
                    id,
                    revision: event["revision"].as_str()?.to_owned(),
                })
            })
            .collect()
    }
    pub(super) fn selection_snapshot(&mut self) -> Option<Vec<katran::SelectionRef>> {
        self.snapshot_ids(self.selected_ids())
    }
    pub(super) fn copy_request(&mut self, ctx: &egui::Context) {
        if self.pending_copy.is_some() || self.pending_copy_ids.is_some() {
            self.message = "Копирование уже в очереди".into();
            return;
        }
        if self.api.is_none()
            && self.job.is_some()
            && self.selected_ids().iter().any(|id| {
                !self
                    .events
                    .iter()
                    .any(|event| event["id"].as_i64() == Some(*id))
            })
        {
            self.pending_copy_ids = Some(self.selected_ids());
            self.message = "Копирование в очереди".into();
            return;
        }
        let Some(items) = self.selection_snapshot() else {
            self.message = "Выбор устарел. Обновите полку и выберите объекты заново.".into();
            return;
        };
        if self.api.is_some() {
            let staged = self.call(Command::StageTransfer {
                items: items.clone(),
                cut: false,
            });
            if staged.is_null() {
                return;
            }
        }
        if self.job.is_some() {
            self.pending_copy = Some(items);
            self.message = "Копирование в очереди".into();
        } else if self.api.is_some() {
            if self.start_job(Command::CopySelectedObjects, false, ctx.clone()) {
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
        let items = if let Some(ids) = self.pending_copy_ids.take() {
            match self.snapshot_ids(ids) {
                Some(items) => items,
                None => {
                    self.message = "Выбор устарел; копирование не выполнено".into();
                    return None;
                }
            }
        } else {
            self.pending_copy.take()?
        };
        if self
            .call(Command::StageTransfer {
                items: items.clone(),
                cut: false,
            })
            .is_null()
        {
            return None;
        }
        Some(Command::CopySmart { items })
    }
    pub(super) fn run(&mut self, id: String, ctx: &egui::Context) {
        if id == "quit" {
            self.quit_application(ctx);
            return;
        }
        if id == "paste_previous" {
            self.paste_previous(ctx);
            return;
        }
        if matches!(id.as_str(), "transfer_selection" | "transfer_copy") {
            let copying = id == "transfer_copy";
            let result = self.call(if copying {
                Command::CopySelection
            } else {
                Command::TransferSelection
            });
            self.actions = false;
            if result.is_null() {
                return;
            }
            if result["interaction"].is_object() {
                self.state = result["interaction"].clone();
            }
            let items = result["items"].as_array().cloned().unwrap_or_default();
            let from_folder = result["direction"] == "from_folder";
            let failures = items
                .iter()
                .filter(|item| {
                    if from_folder {
                        item["ok"] != true
                    } else {
                        !matches!(
                            item["status"].as_str(),
                            Some("moved" | "copied" | "pinned_copy" | "same_folder")
                        )
                    }
                })
                .count();
            let pinned = items
                .iter()
                .filter(|item| item["status"] == "pinned_copy")
                .count();
            let moved = items
                .iter()
                .filter(|item| item["status"] == "moved")
                .count();
            let copied = items
                .iter()
                .filter(|item| item["status"] == "copied")
                .count();
            let details = items
                .iter()
                .filter_map(|item| item["error"]["message"].as_str())
                .collect::<Vec<_>>()
                .join("; ");
            self.message = if from_folder {
                format!(
                    "Добавлено в Катран: {} · Ошибок: {failures}. {details}",
                    items.len() - failures
                )
            } else {
                format!(
                    "В Folder: перемещено {moved} · скопировано {copied} · Закреплённых скопировано {pinned} · Ошибок: {failures}. {details}"
                )
            };
            self.refresh();
            return;
        }
        if id == "add_to_katran" {
            let result = self.call(Command::RunAction { id });
            self.actions = false;
            if let Some(items) = result["items"].as_array() {
                let failed = items.iter().filter(|item| item["ok"] != true).count();
                self.message = format!(
                    "Добавлено в Катран: {}. Ошибок: {failed}. {}",
                    items.len() - failed,
                    item_failures(&result["items"])
                );
            }
            self.refresh();
            return;
        }
        if id == "open_with" {
            self.open_apps_request(ctx);
            return;
        }
        if id == "copy" {
            self.copy_request(ctx);
            return;
        }
        if matches!(
            id.as_str(),
            "cleanup" | "delete" | "copy_text" | "use_as_background"
        ) {
            if self.start_job(Command::RunAction { id }, false, ctx.clone()) {
                self.actions = false;
            }
            return;
        }
        if matches!(
            id.as_str(),
            "open" | "preview" | "quicklook" | "reveal" | "open_with"
        ) && let Some(focus) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(focus));
        }
        let preview_action = matches!(
            id.as_str(),
            "open" | "preview" | "quicklook" | "reveal" | "open_with"
        );
        if preview_action {
            if self.start_job(Command::RunAction { id }, false, ctx.clone()) {
                self.actions = false;
            }
            self.state = self.call(Command::Interaction);
            return;
        }
        let sort_action = id.starts_with("sort_");
        let r = self.call(Command::RunAction { id });
        if sort_action && r["sort"].is_object() {
            let enabled = self.call(Command::GetSettings)["sort_notifications"] != false;
            self.sort_notice =
                enabled.then(|| (sorting_ui::confirmation(&r["sort"]), Instant::now()));
        }
        self.action_result(r);
        if preview_action {
            self.state = self.call(Command::Interaction);
        } else {
            self.refresh();
        }
    }
    pub(super) fn action_result(&mut self, r: Value) {
        if let Some(copied) = r["table_copied"].as_bool() {
            self.message = if copied {
                format!(
                    "Скопировано таблиц: {} · {} × {}",
                    r["table_count"].as_u64().unwrap_or(1),
                    r["row_count"].as_u64().unwrap_or(0),
                    r["column_count"].as_u64().unwrap_or(0)
                )
            } else if r["text_written"] == true {
                "Скопированы значения с табуляциями; HTML таблицы записать не удалось".into()
            } else if r["html_written"] == true {
                "Скопирована HTML-таблица; текстовый формат записать не удалось".into()
            } else if r["merged"] == false {
                format!("Таблицы не скопированы: {}", item_failures(&r["items"]))
            } else {
                "Не удалось записать таблицы в буфер обмена".into()
            };
            if let Some(warnings) = r["warnings"].as_array() {
                for warning in warnings.iter().filter_map(Value::as_str) {
                    self.message.push_str(" · ");
                    self.message.push_str(warning);
                }
            }
        }
        if r["background_applied"] == true {
            self.message = "Изображение сохранено и установлено на фон".into();
        }
        if r["sort"].is_object() {
            self.message = sorting_ui::confirmation(&r["sort"]);
        }
        if let Some(copied) = r["text_copied"].as_bool() {
            self.message = if copied && r["kind"] == "link_fragment" {
                "Ссылки скопированы текстом".into()
            } else if copied {
                "Текстовые фрагменты скопированы одним текстом".into()
            } else {
                format!("Текст не скопирован: {}", item_failures(&r["items"]))
            };
        }
        if r["requested"] == true {
            match r["operation"].as_str() {
                Some("open") => {
                    self.message = if r["link"] == true {
                        "Ссылка передана браузеру по умолчанию".into()
                    } else {
                        "Файл передан приложению по умолчанию".into()
                    }
                }
                Some("reveal") => self.message = "Файл показан в Проводнике".into(),
                Some("open_with") => {
                    self.message = if r["application_id"].is_string() {
                        "Файл передан выбранному приложению".into()
                    } else {
                        "Запрошен выбор приложения Windows".into()
                    }
                }
                _ => {}
            }
        }
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
            self.pending_delete_originals = r["original_files"] == true;
            self.pending_delete_permanent = r["permanent_files"] == true;
            self.pending_delete_recycle = r["recycle_bin"] == true;
            self.pending_delete = r["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
        }
        if let Some(count) = r["deleted"].as_u64()
            && r["confirmation_required"] != true
        {
            self.message = if r["original_files"] == true {
                format!("Удалено объектов: {count}{}", cleanup_message(&r))
            } else {
                format!("Убрано с полки: {count}{}", cleanup_message(&r))
            };
            let errors = item_failures(&r["items"]);
            if !errors.is_empty() {
                self.message.push_str(&format!(" · не удалено: {errors}"));
            }
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
        if let Some(items) = r["items"].as_array()
            && items.iter().any(|i| i.get("path").is_some())
            && r.get("atomic").is_some()
        {
            self.message = format!(
                "Сохранено {}/{}",
                items.iter().filter(|i| i["ok"] == true).count(),
                items.len()
            );
            let errors = item_failures(&r["items"]);
            if !errors.is_empty() {
                self.message.push_str(&format!(" · Ошибки: {errors}"));
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

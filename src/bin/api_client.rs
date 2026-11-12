//! Single-writer dispatcher jobs and UI result handling.
use super::i18n::tr;
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
                self.message = tr(
                    "Дождитесь завершения текущей операции",
                    "Wait for the current operation to finish",
                )
                .into();
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
            self.message = tr(
                "Выбор устарел; обновите полку",
                "Selection is out of date; refresh the shelf",
            )
            .into();
            return;
        };
        if self.start_job(Command::DragEventsChecked { items }, false, ctx.clone()) {
            self.message = tr("Перетаскивание файлов…", "Dragging files…").into();
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
            self.message = r["error"]["message"]
                .as_str()
                .unwrap_or(tr("Ошибка", "Error"))
                .into();
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
    fn preview_request_context(&mut self) -> Value {
        let state = self.call(Command::Interaction);
        serde_json::json!({"selected":state["selected"],"cursor":state["cursor"],"tab":state["tab"],"pinned_only":state["pinned_only"],"folder_path":state["folder_path"]})
    }
    pub(super) fn preview_warmup_tick(&mut self, ctx: &egui::Context) {
        if let Some(job_id) = self.preview_warmup {
            let status = self.call(Command::PollJob { job_id });
            if !matches!(
                status["state"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                return;
            }
            self.preview_warmup = None;
            if let Some(log) = &self.logger {
                log.record(
                    "preview_backend_startup",
                    serde_json::json!({"state":status["state"],"error":status["error"]}),
                );
            }
            if status["state"] != "succeeded" {
                self.pending_preview = None;
                self.message = format!(
                    "QuickLook: {}",
                    status["error"]["message"].as_str().unwrap_or(tr(
                        "Не удалось подготовить просмотр",
                        "Could not prepare preview"
                    ))
                );
                return;
            }
        }
        if self.job.is_none()
            && let Some((id, context)) = self.pending_preview.take()
            && self.preview_request_context() == context
        {
            self.start_job(Command::RunAction { id }, false, ctx.clone());
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
                        serde_json::json!({"ok":false,"error":{"message":tr("Потерян статус фоновой операции", "Background operation status lost")}}),
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
                .unwrap_or(tr("Ошибка", "Error"))
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
                    self.message = format!(
                        "{}: {error}",
                        tr(
                            "Не удалось захватить Clipboard",
                            "Could not capture Clipboard"
                        )
                    );
                } else if result["capture_schedule"]["exhausted"] == true {
                    self.message = tr("Clipboard оставался занят или менялся; повторы исчерпаны. Скопируйте объект ещё раз.", "Clipboard stayed busy or changed; retries exhausted. Copy the item again.").into();
                }
            }
        }
        if let Some(token) = result["drag_token"].as_str() {
            self.native_drag = Some(token.to_owned());
            self.message = tr("Файлы готовы к перетаскиванию…", "Files are ready to drag…").into();
            return;
        }
        if result["scanned"] == true {
            let errors = item_failures(&result["items"]);
            if !errors.is_empty() {
                self.message = format!("{}: {errors}", tr("Папки", "Folders"));
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
                self.message = format!(
                    "{}: {errors}",
                    tr("Не удалось захватить", "Could not capture")
                );
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
        self.preloaded_has_more = false;
        self.tab_preload.waiting = None;
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
            self.message = tr("Копирование уже в очереди", "Copy is already queued").into();
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
            self.message = tr("Копирование в очереди", "Copy queued").into();
            return;
        }
        let Some(items) = self.selection_snapshot() else {
            self.message = tr(
                "Выбор устарел. Обновите полку и выберите объекты заново.",
                "Selection is out of date. Refresh the shelf and select the items again.",
            )
            .into();
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
            self.message = tr("Копирование в очереди", "Copy queued").into();
        } else if self.api.is_some() {
            if self.start_job(Command::CopySelectedObjects, false, ctx.clone()) {
                self.actions = false;
            }
        } else if self.job.is_some() {
            self.pending_copy = Some(items);
            self.message = tr("Копирование в очереди", "Copy queued").into();
        } else {
            self.message = tr(
                "Не скопировано. Перезапустите Katran.",
                "Not copied. Restart Katran.",
            )
            .into();
        }
    }
    pub(super) fn take_queued_copy(&mut self) -> Option<Command> {
        let items = if let Some(ids) = self.pending_copy_ids.take() {
            match self.snapshot_ids(ids) {
                Some(items) => items,
                None => {
                    self.message = tr(
                        "Выбор устарел; копирование не выполнено",
                        "Selection is out of date; nothing was copied",
                    )
                    .into();
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
        if id == "shelf_search" {
            self.toggle_search_focus(ctx);
            return;
        }
        if id == "restart" {
            self.restart_pending = true;
            self.actions = false;
            ctx.request_repaint();
            return;
        }
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
                    "{}: {} · {}: {failures}. {details}",
                    tr("Добавлено в Катран", "Added to Katran"),
                    items.len() - failures,
                    tr("Ошибок", "Failures")
                )
            } else {
                if super::i18n::is_english() {
                    format!(
                        "To Folder: moved {moved} · copied {copied} · pinned copied {pinned} · failures: {failures}. {details}"
                    )
                } else {
                    format!(
                        "В Folder: перемещено {moved} · скопировано {copied} · Закреплённых скопировано {pinned} · Ошибок: {failures}. {details}"
                    )
                }
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
                    "{}: {}. {}: {failed}. {}",
                    tr("Добавлено в Катран", "Added to Katran"),
                    items.len() - failed,
                    tr("Ошибок", "Failures"),
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
            "cleanup" | "delete" | "copy_text" | "use_as_background" | "use_as_video_background"
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
            if self.preview_warmup.is_some() && matches!(id.as_str(), "open" | "quicklook") {
                let context = self.preview_request_context();
                self.pending_preview = Some((id, context));
                self.actions = false;
                ctx.request_repaint();
                return;
            }
            if self.start_job(Command::RunAction { id }, false, ctx.clone()) {
                self.actions = false;
            }
            self.state = self.call(Command::Interaction);
            return;
        }
        let sort_action = id.starts_with("sort_");
        let tab_action = id.starts_with("tab_position_")
            || matches!(
                id.as_str(),
                "tab_toggle"
                    | "tab_previous"
                    | "tab_next"
                    | "images"
                    | "screenshots"
                    | "downloads"
                    | "text"
                    | "links"
                    | "tables"
                    | "pods"
                    | "files"
            );
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
            self.state = self.call(Command::Interaction);
            if !tab_action || !self.show_preloaded_tab(ctx) {
                self.refresh();
            }
        }
    }
    pub(super) fn action_result(&mut self, r: Value) {
        if let Some(copied) = r["table_copied"].as_bool() {
            self.message = if copied {
                format!(
                    "{}: {} · {} × {}",
                    tr("Скопировано таблиц", "Tables copied"),
                    r["table_count"].as_u64().unwrap_or(1),
                    r["row_count"].as_u64().unwrap_or(0),
                    r["column_count"].as_u64().unwrap_or(0)
                )
            } else if r["text_written"] == true {
                tr(
                    "Скопированы значения с табуляциями; HTML таблицы записать не удалось",
                    "Tab-separated values copied; table HTML could not be written",
                )
                .into()
            } else if r["html_written"] == true {
                tr(
                    "Скопирована HTML-таблица; текстовый формат записать не удалось",
                    "HTML table copied; text format could not be written",
                )
                .into()
            } else if r["merged"] == false {
                format!(
                    "{}: {}",
                    tr("Таблицы не скопированы", "Tables not copied"),
                    item_failures(&r["items"])
                )
            } else {
                tr(
                    "Не удалось записать таблицы в буфер обмена",
                    "Could not write tables to the clipboard",
                )
                .into()
            };
            if let Some(warnings) = r["warnings"].as_array() {
                for warning in warnings.iter().filter_map(Value::as_str) {
                    self.message.push_str(" · ");
                    self.message.push_str(warning);
                }
            }
        }
        if r["background_applied"] == true {
            self.message = tr(
                "Изображение сохранено и установлено на фон",
                "Image saved and set as background",
            )
            .into();
        }
        if r["sort"].is_object() {
            self.message = sorting_ui::confirmation(&r["sort"]);
        }
        if let Some(copied) = r["text_copied"].as_bool() {
            self.message = if copied && r["kind"] == "link_fragment" {
                tr("Ссылки скопированы текстом", "Links copied as text").into()
            } else if copied {
                tr(
                    "Текстовые фрагменты скопированы одним текстом",
                    "Text fragments copied as one text",
                )
                .into()
            } else {
                format!(
                    "{}: {}",
                    tr("Текст не скопирован", "Text not copied"),
                    item_failures(&r["items"])
                )
            };
        }
        if r["requested"] == true {
            match r["operation"].as_str() {
                Some("open") => {
                    self.message = if r["link"] == true {
                        tr(
                            "Ссылка передана браузеру по умолчанию",
                            "Link sent to the default browser",
                        )
                        .into()
                    } else {
                        tr(
                            "Файл передан приложению по умолчанию",
                            "File sent to the default app",
                        )
                        .into()
                    }
                }
                Some("reveal") => {
                    self.message = tr("Файл показан в Проводнике", "File shown in Explorer").into()
                }
                Some("open_with") => {
                    self.message = if r["application_id"].is_string() {
                        tr(
                            "Файл передан выбранному приложению",
                            "File sent to the selected app",
                        )
                        .into()
                    } else {
                        tr(
                            "Запрошен выбор приложения Windows",
                            "Windows app picker requested",
                        )
                        .into()
                    }
                }
                _ => {}
            }
        }
        if r["cancelled"] == true {
            self.message = tr("Перетаскивание отменено", "Drag cancelled").into();
        }
        if let Some(count) = r["dragged"].as_u64() {
            self.message = format!(
                "{}: {count}",
                tr("Перетаскивание завершено", "Drag completed")
            );
            let errors = item_failures(&r["failures"]);
            if !errors.is_empty() {
                self.message.push_str(&format!(
                    " · {}: {errors}",
                    tr("не передано", "not transferred")
                ));
            }
            if let Some(warnings) = r["warnings"].as_array() {
                for warning in warnings.iter().filter_map(Value::as_str) {
                    self.message.push_str(&format!(" · {warning}"));
                }
            }
        }
        if let Some(bytes) = r["reclaimed_blob_bytes"].as_u64() {
            self.message = format!(
                "{}: {bytes} {}",
                tr("Очищено временных данных", "Temporary data cleared"),
                tr("байт", "bytes")
            );
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
                format!(
                    "{}: {count}{}",
                    tr("Удалено объектов", "Items deleted"),
                    cleanup_message(&r)
                )
            } else {
                format!(
                    "{}: {count}{}",
                    tr("Убрано с полки", "Removed from shelf"),
                    cleanup_message(&r)
                )
            };
            let errors = item_failures(&r["items"]);
            if !errors.is_empty() {
                self.message
                    .push_str(&format!(" · {}: {errors}", tr("не удалено", "not deleted")));
            }
        }
        if let Some(count) = r["copied"].as_u64() {
            self.message = format!("{}: {count}", tr("Скопировано", "Copied"));
            if let Some(failures) = r["failures"].as_array()
                && !failures.is_empty()
            {
                self.message.push_str(&format!(
                    " · {}: {}",
                    tr("не скопировано", "not copied"),
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
                "{} {}/{}",
                tr("Сохранено", "Saved"),
                items.iter().filter(|i| i["ok"] == true).count(),
                items.len()
            );
            let errors = item_failures(&r["items"]);
            if !errors.is_empty() {
                self.message
                    .push_str(&format!(" · {}: {errors}", tr("Ошибки", "Errors")));
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

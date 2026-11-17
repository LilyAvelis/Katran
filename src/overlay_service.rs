//! Dispatcher-owned copy-before-paste state; GUI only requests and polls.
use crate::{ApiError, Command, Result, Service, domain_jobs::JobId};
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub(crate) struct PendingPaste {
    job_id: JobId,
    lease: crate::overlay::PasteLease,
}
fn overlay_error(error: crate::overlay::OverlayError) -> ApiError {
    ApiError::new(&error.code, error.message)
}
impl Service {
    pub(crate) fn toggle_resident_overlay(
        &mut self,
        owner: u64,
        capture: Option<crate::overlay::SummonReceipt>,
    ) -> Result<Value> {
        if let Some(capture) = capture {
            let mut result = self.summon_captured_overlay(owner, capture)?;
            self.window_visible = true;
            result["visible"] = json!(true);
            Ok(result)
        } else {
            let foreground = self.overlay_host.foreground().map_err(overlay_error)?;
            let own = self.overlay_host.identity(owner).map_err(overlay_error)?;
            let current = self
                .overlay_host
                .identity(foreground)
                .map_err(overlay_error)?;
            if current.pid != own.pid {
                return Err(ApiError::new(
                    "foreground_changed",
                    "Katran уже не является активным приложением",
                ));
            }
            self.window_visible = false;
            self.interaction.settings_open = false;
            Ok(json!({"visible":false}))
        }
    }
    pub(crate) fn summon_captured_overlay(
        &mut self,
        owner: u64,
        capture: crate::overlay::SummonReceipt,
    ) -> Result<Value> {
        if self.overlay_copy.is_some() {
            return Err(ApiError::new("paste_busy", "Вставка ещё выполняется"));
        }
        self.overlay_session
            .adopt(self.overlay_host.as_mut(), owner, capture)
            .map(serde_json::to_value)
            .map_err(overlay_error)?
            .map_err(|error| ApiError::new("overlay_error", error))
    }
    pub fn set_overlay_host(&mut self, host: Box<dyn crate::overlay::Host + Send>) {
        self.overlay_host = host;
        self.overlay_session.clear();
        self.overlay_copy = None;
    }
    pub(crate) fn summon_overlay(&mut self, owner: u64) -> Result<Value> {
        if self.overlay_copy.is_some() {
            return Err(ApiError::new("paste_busy", "Вставка ещё выполняется"));
        }
        let receipt = self
            .overlay_session
            .summon(self.overlay_host.as_mut(), owner)
            .map_err(overlay_error)?;
        Ok(json!(receipt))
    }
    pub(crate) fn begin_paste_previous(&mut self, owner: u64) -> Result<Value> {
        if self.overlay_copy.is_some() {
            return Err(ApiError::new("paste_busy", "Вставка уже подготовлена"));
        }
        let preparation = self
            .overlay_session
            .begin_lease(self.overlay_host.as_mut(), owner);
        if let Some(logger) = self.logger() {
            logger.record("paste_prepare",json!({"ok":preparation.is_ok(),"error_code":preparation.as_ref().err().map(|e|e.code.as_str())}));
        }
        let lease = preparation.map_err(overlay_error)?;
        let result = self.begin_job(Command::CopySelectedObjects)?;
        let id = result["job_id"].as_u64().ok_or_else(|| {
            ApiError::new("copy_job_missing", "Не удалось подготовить копирование")
        })?;
        self.overlay_copy = Some(PendingPaste {
            job_id: JobId(id),
            lease,
        });
        Ok(json!({"state":"copying","job_id":id}))
    }
    pub(crate) fn complete_paste_previous(&mut self, owner: u64) -> Result<Value> {
        let pending = self
            .overlay_copy
            .ok_or_else(|| ApiError::new("paste_not_prepared", "Сначала подготовьте вставку"))?;
        if owner != pending.lease.owner.hwnd {
            return Err(ApiError::new("owner_changed", "Окно Katran изменилось"));
        }
        if let Err(error) =
            crate::overlay::Session::check_lease(self.overlay_host.as_mut(), &pending.lease)
        {
            self.overlay_copy = None;
            let _ = self.jobs.cancel(pending.job_id);
            if let Some(logger) = self.logger() {
                logger.record(
                    "paste_cancelled",
                    json!({"phase":"preparation","error_code":error.code}),
                );
            }
            return Err(overlay_error(error));
        }
        let status = self.poll_job(pending.job_id)?;
        if matches!(
            status["state"].as_str(),
            Some("queued" | "running" | "ready")
        ) {
            return Ok(json!({"state":"copying"}));
        }
        // A ready copy may have published while polling; recheck before any input.
        self.overlay_copy = None;
        crate::overlay::Session::check_lease(self.overlay_host.as_mut(), &pending.lease)
            .map_err(overlay_error)?;
        if status["state"] != "succeeded" {
            return Err(ApiError::new(
                "copy_failed",
                status["error"]["message"]
                    .as_str()
                    .unwrap_or("Копирование не выполнено"),
            ));
        }
        let result = &status["result"];
        let success = result["text_copied"] == true
            || result["table_copied"] == true
            || result["copied"].as_u64().is_some_and(|n| n > 0);
        let failures = result["failures"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
            || result["items"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["ok"] == false));
        if let Some(logger) = self.logger() {
            // Publication facts contain only static formats, counts and a warning flag.
            // Human-readable warnings may contain paths and must not enter diagnostics.
            logger.record("paste_copy",json!({"ok":success&&!failures,"text":result["text_copied"]==true,"table":result["table_copied"]==true,"publication":result["publication"]}));
        }
        if !success || failures {
            return Err(ApiError::new(
                "copy_failed",
                "Данные скопированы не полностью; вставка не отправлена",
            ));
        }
        let sequence = result["clipboard_sequence"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .ok_or_else(|| {
                ApiError::new("copy_unverified", "Нет подтверждения публикации в буфер")
            })? as u32;
        let delivery =
            self.overlay_session
                .deliver_lease(self.overlay_host.as_mut(), pending.lease, sequence);
        if let Some(logger) = self.logger() {
            logger.record("paste_delivery",json!({"expected_clipboard_sequence":sequence,"observed_clipboard_sequence":crate::capture::sequence_number(),"ok":delivery.is_ok(),"error_code":delivery.as_ref().err().map(|e|e.code.as_str()),"tracking_warning":delivery.as_ref().ok().and_then(|r|r.tracking_warning.as_deref())}));
        }
        let receipt = delivery.map_err(overlay_error)?;
        Ok(json!({"state":"sent","receipt":receipt,"copy":result}))
    }
}

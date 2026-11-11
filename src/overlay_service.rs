//! Dispatcher-owned copy-before-paste state; GUI only requests and polls.
use crate::{ApiError, Command, Result, Service, domain_jobs::JobId};
use serde_json::{Value, json};
fn overlay_error(error: crate::overlay::OverlayError) -> ApiError {
    ApiError::new(&error.code, error.message)
}
impl Service {
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
        self.overlay_session
            .validate(self.overlay_host.as_mut(), owner)
            .map_err(overlay_error)?;
        let result = self.begin_job(Command::CopySelectedObjects)?;
        let id = result["job_id"].as_u64().ok_or_else(|| {
            ApiError::new("copy_job_missing", "Не удалось подготовить копирование")
        })?;
        self.overlay_copy = Some((JobId(id), owner));
        Ok(json!({"state":"copying","job_id":id}))
    }
    pub(crate) fn complete_paste_previous(&mut self, owner: u64) -> Result<Value> {
        let (id, expected_owner) = self
            .overlay_copy
            .ok_or_else(|| ApiError::new("paste_not_prepared", "Сначала подготовьте вставку"))?;
        if owner != expected_owner {
            return Err(ApiError::new("owner_changed", "Окно Katran изменилось"));
        }
        let status = self.poll_job(id)?;
        if matches!(
            status["state"].as_str(),
            Some("queued" | "running" | "ready")
        ) {
            return Ok(json!({"state":"copying"}));
        }
        self.overlay_copy = None;
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
        let receipt = self
            .overlay_session
            .paste(self.overlay_host.as_mut(), owner, sequence)
            .map_err(overlay_error)?;
        Ok(json!({"state":"sent","receipt":receipt,"copy":result}))
    }
}

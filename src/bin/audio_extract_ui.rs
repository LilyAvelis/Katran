//! Extract audio through the dispatcher; the dialog never owns the decoder.
use super::i18n::tr;
use super::*;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    context: Value,
    track: u32,
    job: Option<katran::domain_jobs::JobId>,
    status: String,
    path: String,
}

fn track_index(one_based: u32) -> Option<u32> {
    one_based.checked_sub(1)
}

fn friendly_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        path.to_owned()
    }
}

fn success_status(result: &Value) -> String {
    let clipboard = &result["clipboard"];
    let mut status = if clipboard["ok"] == true {
        tr(
            "Аудио сохранено в Files и скопировано как файл",
            "Audio saved in Files and copied as a file",
        )
        .to_owned()
    } else {
        let error = clipboard["error"]["message"]
            .as_str()
            .or_else(|| clipboard["error"].as_str())
            .unwrap_or(tr(
                "состояние буфера неизвестно",
                "clipboard status unknown",
            ));
        format!(
            "{}: {error}",
            tr(
                "Аудио сохранено в Files, но не скопировано",
                "Audio saved in Files, but not copied"
            )
        )
    };
    if let Some(warnings) = clipboard["warnings"].as_array() {
        for warning in warnings.iter().filter_map(Value::as_str) {
            status.push_str(" · ");
            status.push_str(warning);
        }
    }
    status
}

impl Shelf {
    fn audio_context(&mut self) -> Value {
        let state = self.call(Command::Interaction);
        let selected = state["selected"].as_array();
        let id = match selected.map(|s| s.as_slice()) {
            Some([]) | None => state["cursor"].as_i64(),
            Some([id]) => id.as_i64(),
            _ => None,
        };
        let Some(id) = id else {
            return Value::Null;
        };
        let event = self.call(Command::Get { id });
        if event.is_null() {
            return Value::Null;
        }
        json!({"tab": state["tab"], "folder_path": state["folder_path"],
            "selected": state["selected"], "cursor": state["cursor"],
            "id": id, "revision": event["revision"], "path": event["path"],
            "kind": event["kind"], "source": event["source"]})
    }

    pub(super) fn open_audio_extract(&mut self) {
        if self.audio_extract.job.is_some() {
            self.audio_extract.open = true;
            return;
        }
        let context = self.audio_context();
        self.audio_extract = State {
            open: true,
            context,
            track: 1,
            ..State::default()
        };
    }

    pub(super) fn audio_extract_ui(&mut self, ctx: &egui::Context) {
        if let Some(job_id) = self.audio_extract.job {
            let response = self.call(Command::PollJob { job_id });
            match response["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
                Some("succeeded") => {
                    self.audio_extract.job = None;
                    self.audio_extract.path =
                        friendly_path(response["result"]["path"].as_str().unwrap_or(""));
                    self.audio_extract.status = success_status(&response["result"]);
                    if self.state["tab"] == "files" {
                        self.refresh();
                    }
                }
                Some("cancelled") => {
                    self.audio_extract.job = None;
                    self.audio_extract.status =
                        tr("Операция отменена", "Operation cancelled").into();
                }
                Some("failed") => {
                    self.audio_extract.job = None;
                    self.audio_extract.status = response["error"]["message"]
                        .as_str()
                        .unwrap_or(tr("Не удалось извлечь аудио", "Could not extract audio"))
                        .into();
                }
                _ => {
                    self.audio_extract.job = None;
                    self.audio_extract.status = tr(
                        "Не удалось получить состояние операции",
                        "Could not retrieve operation status",
                    )
                    .into();
                }
            }
        }
        if !self.audio_extract.open {
            return;
        }
        let mut start = false;
        let mut cancel = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("extract-video-audio")).show(ctx, |ui| {
            ui.heading(tr("Извлечь аудио", "Extract audio"));
            if self.audio_extract.job.is_some() {
                ui.spinner();
                ui.label(tr("Извлечение аудио…", "Extracting audio…"));
                cancel = ui.button(tr("Отменить", "Cancel")).clicked();
            } else {
                if self.audio_extract.path.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(tr("Номер аудиодорожки", "Audio track number"));
                        ui.add(
                            egui::DragValue::new(&mut self.audio_extract.track).range(1..=u32::MAX),
                        );
                    });
                    ui.weak(tr("Первая дорожка — 1", "First track is 1"));
                    start = ui.button(tr("Извлечь", "Extract")).clicked();
                }
                if !self.audio_extract.status.is_empty() {
                    ui.label(&self.audio_extract.status);
                }
                if !self.audio_extract.path.is_empty() {
                    ui.add(
                        egui::Label::new(&self.audio_extract.path)
                            .selectable(true)
                            .wrap(),
                    );
                }
                close = ui.button(tr("Закрыть", "Close")).clicked();
            }
        });
        if start {
            let current = self.audio_context();
            if current.is_null() || current != self.audio_extract.context {
                self.audio_extract.status = tr(
                    "Выбор изменился. Закройте окно и выберите видео снова.",
                    "Selection changed. Close this dialog and select the video again.",
                )
                .into();
            } else {
                let track = track_index(self.audio_extract.track);
                let result = self.call(Command::ExtractVideoAudio { audio_track: track });
                if let Some(id) = result["job_id"].as_u64() {
                    self.audio_extract.job = Some(katran::domain_jobs::JobId(id));
                    self.audio_extract.status.clear();
                    ctx.request_repaint_after(Duration::from_millis(100));
                } else {
                    self.audio_extract.status = self.message.clone();
                }
            }
        }
        if cancel {
            if let Some(job_id) = self.audio_extract.job {
                self.call(Command::CancelJob { job_id });
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.audio_extract.job.is_none() && (close || modal.should_close()) {
            self.audio_extract.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_audio_reports_clipboard_failure_and_warnings() {
        let failed = success_status(
            &json!({"clipboard":{"ok":false,"error":{"message":"synthetic failure"},"warnings":["synthetic warning"]}}),
        );
        assert!(failed.contains("Files"));
        assert!(failed.contains("synthetic failure"));
        assert!(failed.contains("synthetic warning"));
        let copied = success_status(&json!({"clipboard":{"ok":true,"warnings":[]}}));
        assert!(copied.contains("Files"));
        assert!(!copied.contains("synthetic failure"));
    }
    #[test]
    fn user_track_number_maps_to_zero_based_api() {
        assert_eq!(track_index(1), Some(0));
        assert_eq!(track_index(2), Some(1));
        assert_eq!(track_index(0), None);
        assert_eq!(friendly_path(r"\\?\C:\audio.mka"), r"C:\audio.mka");
        assert_eq!(
            friendly_path(r"\\?\UNC\server\share\audio.mka"),
            r"\\server\share\audio.mka"
        );
    }
}

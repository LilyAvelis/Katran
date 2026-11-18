//! Upscale jobs are bound by the dispatcher to the current selected image.
use super::i18n::tr;
use super::*;
use std::time::Duration;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    job: Option<katran::domain_jobs::JobId>,
    label: &'static str,
    status: String,
}

fn label(operation: katran::UpscaleOperation) -> &'static str {
    use katran::UpscaleOperation::*;
    match operation {
        RemacriX2 => "Remacri ×2",
        RemacriX4 => "Remacri ×4",
        RealEsrganX2 => "Real-ESRGAN ×2",
        RealEsrganX4 => "Real-ESRGAN ×4",
    }
}

impl Shelf {
    pub(super) fn start_upscale(
        &mut self,
        operation: katran::UpscaleOperation,
        ctx: &egui::Context,
    ) {
        if self.upscale.job.is_some() {
            self.upscale.open = true;
            return;
        }
        let result = self.call(Command::UpscaleSelectedImage { operation });
        let job = result["job_id"].as_u64().map(katran::domain_jobs::JobId);
        self.upscale = State {
            open: true,
            job,
            label: label(operation),
            status: if job.is_some() {
                String::new()
            } else {
                self.message.clone()
            },
        };
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    pub(super) fn upscale_ui(&mut self, ctx: &egui::Context) {
        if let Some(job_id) = self.upscale.job {
            let response = self.call(Command::PollJob { job_id });
            match response["state"].as_str() {
                Some("queued" | "running" | "ready") => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
                Some("succeeded") => {
                    self.upscale.job = None;
                    let result = &response["result"];
                    self.upscale.status = format!(
                        "{}: {} × {} → {} × {}. {}",
                        tr("Размер", "Size"),
                        result["input_width"],
                        result["input_height"],
                        result["width"],
                        result["height"],
                        tr("Сохранено в Images", "Saved in Images")
                    );
                    self.refresh();
                }
                Some("cancelled") => {
                    self.upscale.job = None;
                    self.upscale.status = tr("Операция отменена", "Operation cancelled").into();
                }
                Some("failed") => {
                    self.upscale.job = None;
                    self.upscale.status = response["error"]["message"]
                        .as_str()
                        .unwrap_or(tr(
                            "Не удалось увеличить изображение",
                            "Could not upscale image",
                        ))
                        .into();
                }
                _ => {
                    self.upscale.job = None;
                    self.upscale.status = tr(
                        "Не удалось получить состояние операции",
                        "Could not retrieve operation status",
                    )
                    .into();
                }
            }
        }
        if !self.upscale.open {
            return;
        }
        let mut cancel = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("image-upscale")).show(ctx, |ui| {
            ui.heading(self.upscale.label);
            if self.upscale.job.is_some() {
                ui.spinner();
                ui.label(tr("Увеличение изображения…", "Upscaling image…"));
                cancel = ui.button(tr("Отменить", "Cancel")).clicked();
            } else {
                ui.label(&self.upscale.status);
                close = ui.button(tr("Закрыть", "Close")).clicked();
            }
        });
        if cancel {
            if let Some(job_id) = self.upscale.job {
                self.call(Command::CancelJob { job_id });
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.upscale.job.is_none() && (close || modal.should_close()) {
            self.upscale.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katran::actions::upscale_operation;
    #[test]
    fn four_actions_map_to_distinct_operations() {
        let names = [
            "upscale_remacri_x2",
            "upscale_remacri_x4",
            "upscale_realesrgan_x2",
            "upscale_realesrgan_x4",
        ];
        let labels: Vec<_> = names
            .iter()
            .map(|id| label(upscale_operation(id).unwrap()))
            .collect();
        assert_eq!(
            labels,
            [
                "Remacri ×2",
                "Remacri ×4",
                "Real-ESRGAN ×2",
                "Real-ESRGAN ×4"
            ]
        );
        assert!(upscale_operation("unknown").is_none());
    }
}

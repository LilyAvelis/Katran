//! Thin window-thread client. All preparation/publication stays in the dispatcher.
use super::*;

impl Shelf {
    pub(super) fn cancel_released_notion_drag(
        &mut self,
        ctx: &egui::Context,
        input: &egui::RawInput,
    ) {
        let preparing = self.transfer_busy.as_ref().is_some_and(|busy| {
            busy.kind == TransferKind::NotionExport && (busy.active || self.native_drag.is_some())
        });
        if !preparing {
            return;
        }
        let ended = input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } | egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                }
            )
        });
        if ended || !self.notion_database_active() {
            if let Some(UiJob::Domain { job_id, .. }) = self.job.take() {
                self.call(Command::CancelJob { job_id });
            }
            if let Some(token) = self.native_drag.take() {
                self.call(Command::CancelPreparedDrag { token });
            }
            self.transfer_busy = None;
            self.message = i18n::tr("Перетаскивание отменено", "Drag cancelled").into();
            ctx.stop_dragging();
            ctx.request_repaint();
        }
    }

    pub(super) fn perform_native_drag(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let Some(token) = self.native_drag.take() else {
            return;
        };
        if self.api.is_none() {
            self.native_drag = Some(token);
            return;
        }
        let result = self.call(Command::RunPreparedDrag {
            token,
            owner_window: self.native_window,
        });
        self.action_result(result);
        // Native OLE consumed mouse-up/Escape in its modal message loop. Discard
        // pre-loop pointer events and reset egui so a later click cannot inherit
        // the old drag gesture, re-open the item or leave it visually pressed.
        ctx.stop_dragging();
        input.events.retain(|event| {
            !matches!(
                event,
                egui::Event::PointerMoved(_)
                    | egui::Event::PointerButton { .. }
                    | egui::Event::PointerGone
                    | egui::Event::MouseMoved(_)
                    | egui::Event::Key {
                        key: egui::Key::Escape,
                        ..
                    }
            )
        });
        input.events.push(egui::Event::PointerButton {
            pos: egui::Pos2::ZERO,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        input.events.push(egui::Event::PointerGone);
        ctx.request_repaint();
    }
}

//! Pre-route workspace Tab before egui performs focus traversal.
use eframe::egui::{self, Context, Event, Key, Modifiers, RawInput};

#[derive(Default)]
pub(super) struct State {
    previously_focused: Option<bool>,
    search_at_loss: bool,
}
impl State {
    pub(super) fn intercept(&mut self, ctx: &Context, input: &mut RawInput, modal: bool) -> bool {
        let search = egui::Id::new("shelf-search");
        let focused = input.focused;
        if self.previously_focused == Some(true) && !focused {
            self.search_at_loss = ctx.memory(|m| m.has_focus(search));
        }
        if self.previously_focused == Some(false) && focused && !modal {
            ctx.memory_mut(|m| {
                if self.search_at_loss {
                    m.request_focus(search);
                } else if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        self.previously_focused = Some(focused);
        if !focused || modal || ctx.memory(|m| m.has_focus(search)) {
            return false;
        }
        let mut fresh = false;
        input.events.retain(|event| match event {
            Event::Key {
                key: Key::Tab,
                pressed: true,
                repeat,
                modifiers,
                ..
            } if *modifiers == Modifiers::NONE => {
                fresh |= !*repeat;
                false
            }
            _ => true,
        });
        if fresh {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        fresh
    }
}

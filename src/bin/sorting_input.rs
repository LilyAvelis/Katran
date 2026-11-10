//! Comma sequence adapter. Sort behavior lives in the shared domain Actions.
use eframe::egui::{self, Key};

#[derive(Default)]
pub struct State {
    started: Option<f64>,
    reverse_prefix: bool,
}
impl State {
    pub fn take(&mut self, ctx: &egui::Context, allowed: bool) -> Option<String> {
        if !allowed {
            self.started = None;
            return None;
        }
        ctx.input_mut(|input| {
            if self.started.is_some_and(|time|input.time-time>2.) {self.started=None;}
            let mut action=None;
            let mut consumed_text=false;
            let now=input.time;
            let raw_shift=input.modifiers.shift;
            let raw_modified=input.modifiers.ctrl||input.modifiers.alt||input.modifiers.command;
            input.events.retain(|event| {
                if matches!(event,egui::Event::Key{pressed:true,modifiers,..} if modifiers.ctrl||modifiers.alt||modifiers.command) {self.started=None;return true;}
                match event {
                    egui::Event::Key{key,physical_key,pressed:true,repeat:false,modifiers,..}=>{
                        let key=physical_key.unwrap_or(*key);
                        if key==Key::Comma {
                            self.started=Some(now);self.reverse_prefix=modifiers.shift||raw_shift;consumed_text=true;return false;
                        }
                        if self.started.take().is_some() {
                            if let Some(mode)=key_mode(key) {
                                action=Some(action_id(mode,self.reverse_prefix||modifiers.shift||raw_shift));consumed_text=true;return false;
                            }
                            if key==Key::Escape{return false;}
                        }
                    }
                    egui::Event::Text(text) if !raw_modified=>{
                        if consumed_text {return false;}
                        if text==","||text=="<" {
                            self.started=Some(now);self.reverse_prefix=text=="<"||raw_shift;return false;
                        }
                        if self.started.take().is_some() && let Some(mode)=text_mode(text) {
                            let uppercase=text.chars().any(char::is_uppercase);
                            action=Some(action_id(mode,self.reverse_prefix||raw_shift||uppercase));return false;
                        }
                    }
                    _=>{}
                }
                true
            });
            action
        })
    }
}
fn key_mode(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::E => "extension",
        Key::M => "modified",
        Key::R => "shuffle",
        Key::N => "natural",
        Key::S => "size",
        Key::A => "name",
        Key::O => "origin",
        _ => return None,
    })
}
fn text_mode(text: &str) -> Option<&'static str> {
    Some(match text.to_lowercase().as_str() {
        "e" | "у" => "extension",
        "m" | "ь" => "modified",
        "r" | "к" => "shuffle",
        "n" | "т" => "natural",
        "s" | "ы" => "size",
        "a" | "ф" => "name",
        "o" | "щ" => "origin",
        _ => return None,
    })
}
fn action_id(mode: &str, reverse: bool) -> String {
    format!(
        "sort_{mode}{}",
        if reverse && mode != "shuffle" {
            "_reverse"
        } else {
            ""
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        state: &mut State,
        time: f64,
        events: Vec<egui::Event>,
        allowed: bool,
    ) -> Option<String> {
        let mut events = events;
        let releases: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let egui::Event::Key {
                    key,
                    physical_key,
                    modifiers,
                    ..
                } = e
                {
                    Some(egui::Event::Key {
                        key: *key,
                        physical_key: *physical_key,
                        modifiers: *modifiers,
                        pressed: false,
                        repeat: false,
                    })
                } else {
                    None
                }
            })
            .collect();
        events.extend(releases);
        let mut result = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| result = state.take(ui.ctx(), allowed),
        );
        output.textures_delta.clear();
        result
    }
    fn key(key: Key, shift: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        }
    }
    #[test]
    fn sequences_reverse_origin_shuffle_and_timeout() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        assert_eq!(
            frame(&ctx, &mut state, 0., vec![key(Key::Comma, false)], true),
            None
        );
        assert_eq!(
            frame(&ctx, &mut state, 0.2, vec![key(Key::O, true)], true),
            Some("sort_origin_reverse".into())
        );
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                1.,
                vec![key(Key::Comma, false), key(Key::R, true)],
                true
            ),
            Some("sort_shuffle".into())
        );
        frame(&ctx, &mut state, 2., vec![key(Key::Comma, false)], true);
        assert_eq!(
            frame(&ctx, &mut state, 5., vec![key(Key::N, false)], true),
            None
        );
    }
    #[test]
    fn editing_escape_and_ctrl_comma_preserve_input() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                0.,
                vec![key(Key::Comma, false), key(Key::A, false)],
                false
            ),
            None
        );
        frame(
            &ctx,
            &mut state,
            1.,
            vec![key(Key::Comma, false), key(Key::Escape, false)],
            true,
        );
        assert_eq!(
            frame(&ctx, &mut state, 1.2, vec![key(Key::A, false)], true),
            None
        );
        let ctrl = egui::Event::Key {
            key: Key::Comma,
            physical_key: Some(Key::Comma),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        assert_eq!(
            frame(&ctx, &mut state, 2., vec![ctrl, key(Key::A, false)], true),
            None
        );
    }

    #[test]
    fn modified_shortcut_cancels_pending_prefix_before_following_letter() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        frame(&ctx, &mut state, 0., vec![key(Key::Comma, false)], true);
        let ctrl = egui::Event::Key {
            key: Key::Comma,
            physical_key: Some(Key::Comma),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        assert_eq!(
            frame(&ctx, &mut state, 0.1, vec![ctrl, key(Key::A, false)], true),
            None
        );
    }
    #[test]
    fn native_raw_events_shifted_letter_physical_layout_and_shifted_comma() {
        for (prefix_shift, letter_shift, logical, physical, text) in [
            (false, true, Key::E, Key::E, "E"),
            (false, true, Key::A, Key::E, "У"),
            (true, false, Key::E, Key::E, "e"),
        ] {
            let ctx = egui::Context::default();
            let mut state = State::default();
            let prefix_mod = egui::Modifiers {
                shift: prefix_shift,
                ..Default::default()
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(0.),
                    events: vec![
                        egui::Event::Key {
                            key: Key::Comma,
                            physical_key: Some(Key::Comma),
                            pressed: true,
                            repeat: false,
                            modifiers: prefix_mod,
                        },
                        egui::Event::Text(if prefix_shift { "<" } else { "," }.into()),
                    ],
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(state.take(ui.ctx(), true), None);
                },
            );
            output.textures_delta.clear();
            let modifiers = egui::Modifiers {
                shift: letter_shift,
                ..Default::default()
            };
            let mut action = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(0.1),
                    events: vec![
                        egui::Event::Key {
                            key: logical,
                            physical_key: Some(physical),
                            pressed: true,
                            repeat: false,
                            modifiers,
                        },
                        egui::Event::Text(text.into()),
                    ],
                    ..Default::default()
                },
                |ui| action = state.take(ui.ctx(), true),
            );
            output.textures_delta.clear();
            assert_eq!(action, Some("sort_extension_reverse".into()));
        }
        let ctx = egui::Context::default();
        let mut state = State::default();
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                0.,
                vec![egui::Event::Text("<".into())],
                true
            ),
            None
        );
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                0.1,
                vec![egui::Event::Text("Щ".into())],
                true
            ),
            Some("sort_origin_reverse".into())
        );
    }
}

//! Modal sort launcher; the selected sort is still a shared domain Action.
use eframe::egui::{self, Key};

#[derive(Default)]
pub struct State {
    open: bool,
    selected: usize,
    scroll: bool,
}
impl State {
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.scroll = true;
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn take_scroll(&mut self) -> bool {
        std::mem::take(&mut self.scroll)
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    pub fn take(&mut self, ctx: &egui::Context, allowed: bool) -> Option<String> {
        if !allowed {
            self.close();
            return None;
        }
        let choices: Vec<_> = katran::actions::DEFS
            .iter()
            .filter(|a| a.id.starts_with("sort_"))
            .map(|a| a.id)
            .collect();
        ctx.input_mut(|input| {
            let mut action = None;
            let mut consume_text = false;
            let raw_shift = input.modifiers.shift;
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => {
                    let physical = physical_key.unwrap_or(*key);
                    let plain = !modifiers.ctrl
                        && !modifiers.alt
                        && !modifiers.command
                        && !modifiers.mac_cmd;
                    if *repeat && !matches!(physical, Key::ArrowDown | Key::ArrowUp) {
                        return !self.open && !consume_text;
                    }
                    if !self.open {
                        if plain && physical == Key::Comma {
                            self.open();
                            consume_text = true;
                            return false;
                        }
                        return true;
                    }
                    consume_text = true;
                    if plain && matches!(physical, Key::ArrowDown | Key::ArrowUp) {
                        if physical == Key::ArrowDown {
                            self.selected = (self.selected + 1) % choices.len();
                        } else {
                            self.selected = (self.selected + choices.len() - 1) % choices.len();
                        }
                        self.scroll = true;
                        return false;
                    }
                    if plain && physical == Key::Enter {
                        action = Some(choices[self.selected].to_owned());
                        self.close();
                        return false;
                    }
                    if plain && physical == Key::Escape {
                        self.close();
                        return false;
                    }
                    if plain && let Some(mode) = key_mode(physical) {
                        action = Some(action_id(mode, modifiers.shift || raw_shift));
                        self.close();
                    }
                    false
                }
                egui::Event::Key { .. } if self.open || consume_text => false,
                egui::Event::Text(text) => {
                    if consume_text {
                        return false;
                    }
                    if !self.open {
                        if text == "," || text == "<" {
                            self.open();
                            return false;
                        }
                        return true;
                    }
                    if let Some(mode) = text_mode(text) {
                        action = Some(action_id(
                            mode,
                            raw_shift || text.chars().any(char::is_uppercase),
                        ));
                        self.close();
                    }
                    false
                }
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
                    if self.open || consume_text =>
                {
                    false
                }
                _ => true,
            });
            action
        })
    }
}
fn key_mode(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::T => "arrival",
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
        "t" | "е" => "arrival",
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
    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    fn frame(
        ctx: &egui::Context,
        state: &mut State,
        events: Vec<egui::Event>,
        allowed: bool,
    ) -> (Option<String>, usize) {
        let releases: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some(egui::Event::Key {
                    key: *key,
                    physical_key: *physical_key,
                    pressed: false,
                    repeat: false,
                    modifiers: *modifiers,
                }),
                _ => None,
            })
            .collect();
        let mut result = None;
        let mut remaining = 0;
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                result = state.take(ui.ctx(), allowed);
                remaining = ui.ctx().input(|i| i.events.len());
            },
        );
        out.textures_delta.clear();
        if !releases.is_empty() {
            let mut release = ctx.run_ui(
                egui::RawInput {
                    events: releases,
                    ..Default::default()
                },
                |_| {},
            );
            release.textures_delta.clear();
        }
        (result, remaining)
    }
    #[test]
    fn arrows_select_rows_and_enter_executes_the_highlighted_sort() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        state.open();
        let choices: Vec<_> = katran::actions::DEFS
            .iter()
            .filter(|a| a.id.starts_with("sort_"))
            .map(|a| a.id)
            .collect();
        frame(
            &ctx,
            &mut state,
            vec![key(Key::ArrowDown, egui::Modifiers::NONE)],
            true,
        );
        assert_eq!(state.selected(), 1);
        assert!(state.take_scroll());
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![key(Key::Enter, egui::Modifiers::NONE)],
                true
            )
            .0
            .as_deref(),
            Some(choices[1])
        );
    }
    #[test]
    fn opens_persists_and_selects_arrival_in_both_directions() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![
                    key(Key::Comma, egui::Modifiers::NONE),
                    egui::Event::Text(",".into())
                ],
                true
            ),
            (None, 0)
        );
        assert!(state.is_open());
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![
                    key(Key::Q, egui::Modifiers::NONE),
                    egui::Event::Text("q".into())
                ],
                true
            ),
            (None, 0)
        );
        assert!(state.is_open());
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![
                    key(Key::T, egui::Modifiers::SHIFT),
                    egui::Event::Text("T".into())
                ],
                true
            ),
            (Some("sort_arrival_reverse".into()), 0)
        );
        assert!(!state.is_open());
        state.open();
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![key(Key::T, egui::Modifiers::NONE)],
                true
            )
            .0,
            Some("sort_arrival".into())
        );
    }
    #[test]
    fn repeats_in_selection_frame_do_not_escape_modal() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        state.open();
        let repeated = egui::Event::Key {
            key: Key::T,
            physical_key: Some(Key::T),
            pressed: true,
            repeat: true,
            modifiers: egui::Modifiers::NONE,
        };
        let result = frame(
            &ctx,
            &mut state,
            vec![
                key(Key::T, egui::Modifiers::NONE),
                repeated,
                egui::Event::Text("t".into()),
            ],
            true,
        );
        assert_eq!(result, (Some("sort_arrival".into()), 0));
        assert!(!state.is_open());
    }
    #[test]
    fn other_sorts_layout_escape_and_local_inputs() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        state.open();
        assert_eq!(
            frame(&ctx, &mut state, vec![egui::Event::Text("У".into())], true).0,
            Some("sort_extension_reverse".into())
        );
        state.open();
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![key(Key::R, egui::Modifiers::SHIFT)],
                true
            )
            .0,
            Some("sort_shuffle".into())
        );
        state.open();
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![key(Key::Escape, egui::Modifiers::NONE)],
                true
            ),
            (None, 0)
        );
        assert!(!state.is_open());
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![
                    key(Key::Comma, egui::Modifiers::NONE),
                    egui::Event::Text(",".into())
                ],
                false
            ),
            (None, 2)
        );
        assert!(!state.is_open());
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                vec![key(Key::Comma, egui::Modifiers::CTRL)],
                true
            ),
            (None, 1)
        );
    }
}

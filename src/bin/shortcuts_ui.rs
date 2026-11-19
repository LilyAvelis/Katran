//! Input and presentation adapter shared by Actions and Settings.
use super::i18n::tr;
use eframe::egui::{self, Key};
use egui_lucide::Lucide;
use katran::shortcuts::{ShortcutInfo, normalize};
pub fn apply(api: &mut katran::Service, editor: &mut Editor, intent: Intent) -> bool {
    match intent {
        Intent::Cancel => true,
        Intent::Set { binding, replace } => {
            let r = api.execute(katran::Request {
                api_version: 1,
                request_id: "shortcut-editor".into(),
                command: katran::Command::SetShortcut {
                    id: editor.id.clone(),
                    binding: Some(binding.clone()),
                    replace,
                    reset: false,
                },
            });
            if r["ok"] == true {
                true
            } else {
                editor.failed(
                    binding,
                    r["error"]["message"]
                        .as_str()
                        .unwrap_or(tr("Ошибка назначения", "Could not assign shortcut"))
                        .into(),
                    r["error"]["code"] == "shortcut_conflict",
                );
                false
            }
        }
    }
}
#[derive(Clone, Debug)]
pub enum Intent {
    Set { binding: String, replace: bool },
    Cancel,
}
#[derive(Default)]
pub struct Editor {
    pub id: String,
    pub label: String,
    pub current_binding: String,
    binding: Option<String>,
    error: Option<String>,
    replaceable: bool,
    selected: Option<usize>,
    physical_keys: bool,
    confirm_pending: bool,
    pending_binding: Option<String>,
    held_modifiers: [bool; 8],
    alt_armed: bool,
}
impl Editor {
    pub fn with_confirmation(mut self) -> Self {
        self.confirm_pending = true;
        self
    }

    pub fn with_physical_keys(mut self) -> Self {
        self.physical_keys = true;
        self
    }
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        current_binding: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            current_binding: current_binding.into(),
            ..Self::default()
        }
    }
    pub fn failed(&mut self, binding: String, message: String, replaceable: bool) {
        self.binding = replaceable.then_some(binding);
        self.error = Some(message);
        self.replaceable = replaceable;
        self.selected = replaceable.then_some(0);
    }
    fn choices(&self) -> Vec<(&'static str, usize)> {
        let mut c = vec![];
        if self.replaceable {
            c.push((tr("Заменить назначение", "Replace binding"), 0));
        }
        if self.binding.is_some() {
            c.push((tr("Другая клавиша", "Another key"), 1));
        }
        if !self.current_binding.is_empty() {
            c.push((
                tr(
                    "Снять назначение · Ctrl Backspace",
                    "Clear binding · Ctrl Backspace",
                ),
                2,
            ));
        }
        c.push((tr("Отмена · Esc", "Cancel · Esc"), 3));
        c
    }
    fn choice(&mut self, choice: usize) -> Option<Intent> {
        match choice {
            0 => Some(Intent::Set {
                binding: self.binding.clone()?,
                replace: true,
            }),
            1 => {
                self.binding = None;
                self.error = None;
                self.replaceable = false;
                self.selected = None;
                None
            }
            2 => Some(Intent::Set {
                binding: String::new(),
                replace: false,
            }),
            _ => Some(Intent::Cancel),
        }
    }
    pub fn input(&mut self, ctx: &egui::Context) -> Option<Intent> {
        ctx.input_mut(|input| {
            if !input.focused {
                self.held_modifiers = [false; 8];
                self.alt_armed = false;
                discard(input);
                return None;
            }
            let mut captured = None;
            for event in &input.events {
                let egui::Event::Key {
                    key,
                    physical_key,
                    pressed,
                    repeat,
                    modifiers,
                } = event
                else {
                    continue;
                };
                let physical = physical_key.unwrap_or(*key);
                if let Some(index) = modifier_index(physical).or_else(|| modifier_index(*key)) {
                    if self.physical_keys {
                        self.held_modifiers[index] = *pressed;
                    }
                    if matches!(physical, Key::AltLeft | Key::AltRight)
                        || matches!(key, Key::AltLeft | Key::AltRight)
                    {
                        self.alt_armed = if *pressed {
                            true
                        } else if self.physical_keys {
                            self.held_modifiers[2] || self.held_modifiers[3]
                        } else {
                            false
                        };
                    }
                    continue;
                }
                if captured.is_some() || *repeat {
                    continue;
                }
                let held_alt =
                    self.physical_keys && (self.held_modifiers[2] || self.held_modifiers[3]);
                if !*pressed && !(self.alt_armed && (modifiers.alt || held_alt)) {
                    continue;
                }
                let mut active = *modifiers;
                if self.physical_keys {
                    active.ctrl |= self.held_modifiers[0] || self.held_modifiers[1];
                    active.alt |= held_alt;
                    active.shift |= self.held_modifiers[4] || self.held_modifiers[5];
                    active.mac_cmd |= self.held_modifiers[6] || self.held_modifiers[7];
                }
                captured = Some((if self.physical_keys { physical } else { *key }, active));
            }
            if !input.modifiers.alt && !self.held_modifiers[2] && !self.held_modifiers[3] {
                self.alt_armed = false;
            }
            discard(input);
            let (key, m) = captured?;
            if key == Key::Escape && m.is_none() {
                return Some(Intent::Cancel);
            }
            if key == Key::Backspace && m.ctrl && !m.alt && !m.shift && !m.mac_cmd {
                return self.choice(2);
            }
            if self.confirm_pending {
                if key == Key::Enter
                    && m.is_none()
                    && (self.pending_binding.is_some() || self.physical_keys)
                {
                    return self.confirm_pending();
                }
                if key != Key::Tab || m.ctrl || m.alt || m.mac_cmd {
                    self.pending_binding = Some(chord(key, m));
                    self.error = None;
                    self.replaceable = false;
                    self.binding = None;
                }
                return None;
            }
            let controls = self.binding.is_some() || self.selected.is_some();
            let backwards =
                matches!(key, Key::ArrowLeft | Key::ArrowUp) || (key == Key::Tab && m.shift);
            if key == Key::Tab && !m.ctrl && !m.alt
                || controls
                    && m.is_none()
                    && matches!(
                        key,
                        Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
                    )
            {
                let n = self.choices().len();
                self.selected = Some(match self.selected {
                    Some(i) if backwards => (i + n - 1) % n,
                    Some(i) => (i + 1) % n,
                    None if backwards => n - 1,
                    None => 0,
                });
                return None;
            }
            if controls && key == Key::Enter && m.is_none() {
                let choice = self.choices()[self.selected.unwrap_or(0)].1;
                return self.choice(choice);
            }
            if self.binding.is_some() {
                return None;
            }
            Some(Intent::Set {
                binding: chord(key, m),
                replace: false,
            })
        })
    }
    fn confirm_pending(&self) -> Option<Intent> {
        if self.replaceable {
            return None;
        }
        self.pending_binding.clone().map(|binding| Intent::Set {
            binding,
            replace: false,
        })
    }

    pub fn show_popover(
        &mut self,
        ui: &egui::Ui,
        anchor: &egui::Response,
        opened: bool,
        icon: Lucide,
        raised: bool,
    ) -> (bool, Option<Intent>) {
        let mut open = ui.is_rect_visible(anchor.rect);
        let intent = egui::Popup::from_response(anchor)
            .open_bool(&mut open)
            .gap(6.0)
            .close_behavior(if opened {
                egui::PopupCloseBehavior::IgnoreClicks
            } else {
                egui::PopupCloseBehavior::CloseOnClickOutside
            })
            .show(|ui| self.render_popover(ui, icon, raised))
            .and_then(|response| response.inner);
        (open, intent)
    }

    fn render_popover(&mut self, ui: &mut egui::Ui, icon: Lucide, raised: bool) -> Option<Intent> {
        use super::special_mode_bar;
        ui.set_width(360.0);
        let chord = self
            .pending_binding
            .as_deref()
            .unwrap_or(&self.current_binding);
        capture_field(ui, chord, raised);
        ui.separator();
        if let Some(error) = &self.error {
            let mut replace = false;
            ui.horizontal(|ui| {
                if self.replaceable {
                    replace = ui.button(tr("Заменить", "Replace")).clicked();
                }
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(error)
                            .small()
                            .color(egui::Color32::LIGHT_RED),
                    )
                    .truncate(),
                )
                .on_hover_text(error);
            });
            if replace {
                return self.choice(0);
            }
        }
        special_mode_bar::show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if keycap_button(
                    ui,
                    "↵",
                    self.pending_binding.is_some() && !self.replaceable,
                    raised,
                    true,
                )
                .on_hover_text(tr("Применить сочетание", "Apply shortcut"))
                .clicked()
                {
                    return self.confirm_pending();
                }
                if keycap_button(ui, "Esc", true, raised, true)
                    .on_hover_text(tr("Отмена", "Cancel"))
                    .clicked()
                {
                    return Some(Intent::Cancel);
                }
                if keycap_button(ui, "⌫", !self.current_binding.is_empty(), raised, true)
                    .on_hover_text(tr("Очистить · Ctrl+Backspace", "Clear · Ctrl+Backspace"))
                    .clicked()
                {
                    return self.choice(2);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(icon.size(16.0).image().tint(ui.visuals().text_color()));
                    ui.add(
                        egui::Label::new(egui::RichText::new(&self.label).size(14.0)).truncate(),
                    )
                    .on_hover_text(&self.label);
                });
                None
            })
            .inner
        })
        .inner
    }

    pub fn render(&mut self, ui: &mut egui::Ui) -> Option<Intent> {
        ui.strong(format!("{} · {}", tr("Клавиша", "Shortcut"), self.label));
        ui.label(format!(
            "{}: {}",
            tr("Текущее сочетание", "Current shortcut"),
            if self.current_binding.is_empty() {
                tr("не назначено", "not assigned")
            } else {
                &self.current_binding
            }
        ));
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        } else {
            ui.label(tr(
                "Нажмите сочетание клавиш. Оно сохранится сразу.",
                "Press a shortcut. It will be saved immediately.",
            ));
        }
        let mut choice = None;
        ui.horizontal_wrapped(|ui| {
            for (i, (label, value)) in self.choices().into_iter().enumerate() {
                if ui
                    .add(
                        egui::Button::new(label)
                            .selected(self.selected == Some(i))
                            .wrap(),
                    )
                    .clicked()
                {
                    choice = Some(value);
                }
            }
        });
        ui.weak(tr(
            "Tab — к кнопкам · стрелки — выбор · Enter — подтвердить",
            "Tab — buttons · arrows — select · Enter — confirm",
        ));
        choice.and_then(|c| self.choice(c))
    }
}

pub fn binding_button(
    ui: &mut egui::Ui,
    binding: Option<&str>,
    enabled: bool,
    raised: bool,
) -> egui::Response {
    keycap_button(
        ui,
        binding
            .filter(|binding| !binding.is_empty())
            .unwrap_or(tr("Назначить", "Set shortcut")),
        enabled,
        raised,
        false,
    )
}

fn keycap_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    raised: bool,
    small: bool,
) -> egui::Response {
    let painter = ui.painter().clone();
    let shadow = raised.then(|| painter.add(egui::Shape::Noop));
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(if small { 7.0 } else { 11.0 }, 3.0);
            let mut text = egui::RichText::new(label).size(if small { 13.0 } else { 15.0 });
            if raised && enabled && !small {
                text = text.color(egui::Color32::from_rgb(247, 125, 116));
            }
            ui.add_enabled(
                enabled,
                egui::Button::new(text)
                    .min_size(egui::vec2(28.0, if small { 23.0 } else { 30.0 }))
                    .corner_radius(5.0)
                    .stroke(ui.visuals().widgets.inactive.bg_stroke),
            )
        })
        .inner;
    if let Some(shadow) = shadow
        && enabled
        && !response.is_pointer_button_down_on()
    {
        painter.set(
            shadow,
            egui::Shape::rect_filled(
                response.rect.translate(egui::vec2(0.0, 2.0)),
                5.0,
                egui::Color32::from_black_alpha(120),
            ),
        );
    }
    response
}

fn capture_field(ui: &mut egui::Ui, binding: &str, raised: bool) {
    use super::theme_ui::{self, Role};
    use egui::{Color32, FontId};
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 112.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    let top = theme_ui::color(ui.ctx(), Role::Surface, ui.visuals().window_fill).to_opaque();
    let bottom = ui.visuals().window_fill.to_opaque();
    let mut gradient = egui::Mesh::default();
    gradient.colored_vertex(rect.left_top(), top);
    gradient.colored_vertex(rect.right_top(), top);
    gradient.colored_vertex(rect.right_bottom(), bottom);
    gradient.colored_vertex(rect.left_bottom(), bottom);
    gradient.add_triangle(0, 1, 2);
    gradient.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(gradient));

    let color = ui.visuals().text_color();
    let keys: Vec<_> = binding
        .split_whitespace()
        .map(|key| painter.layout_no_wrap(key.to_owned(), FontId::proportional(23.0), color))
        .collect();
    if keys.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr("Нажмите сочетание", "Press a shortcut"),
            FontId::proportional(18.0),
            ui.visuals().weak_text_color(),
        );
        return;
    }
    let gap = 8.0;
    let width = |key: &egui::Galley| (key.size().x + 24.0).max(42.0);
    let total = keys.iter().map(|key| width(key)).sum::<f32>() + gap * (keys.len() - 1) as f32;
    let mut left = rect.center().x - total * 0.5;
    for key in keys {
        let bounds = egui::Rect::from_min_size(
            egui::pos2(left, rect.center().y - 22.0),
            egui::vec2(width(&key), 44.0),
        );
        if raised {
            painter.rect_filled(
                bounds.translate(egui::vec2(0.0, 3.0)),
                7.0,
                Color32::from_black_alpha(130),
            );
        }
        painter.rect(
            bounds,
            7.0,
            theme_ui::color(ui.ctx(), Role::Raised, Color32::from_gray(59)),
            ui.visuals().widgets.inactive.bg_stroke,
            egui::StrokeKind::Inside,
        );
        painter.galley(bounds.center() - key.size() * 0.5, key, color);
        left = bounds.right() + gap;
    }
}
fn modifier_index(key: Key) -> Option<usize> {
    match key {
        Key::ControlLeft => Some(0),
        Key::ControlRight => Some(1),
        Key::AltLeft => Some(2),
        Key::AltRight => Some(3),
        Key::ShiftLeft => Some(4),
        Key::ShiftRight => Some(5),
        Key::SuperLeft => Some(6),
        Key::SuperRight => Some(7),
        _ => None,
    }
}
pub fn discard(input: &mut egui::InputState) {
    input.events.retain(|e| {
        !matches!(
            e,
            egui::Event::Key { .. }
                | egui::Event::Text(_)
                | egui::Event::Copy
                | egui::Event::Cut
                | egui::Event::Paste(_)
        )
    });
}
fn chord(key: Key, m: egui::Modifiers) -> String {
    let mut p = vec![];
    if m.ctrl {
        p.push("Ctrl");
    }
    if m.alt {
        p.push("Alt");
    }
    if m.shift {
        p.push("Shift");
    }
    if m.mac_cmd {
        p.push("Super");
    }
    p.push(match key {
        Key::ArrowLeft => "ArrowLeft",
        Key::ArrowRight => "ArrowRight",
        Key::ArrowUp => "ArrowUp",
        Key::ArrowDown => "ArrowDown",
        _ => key.name(),
    });
    p.join(" ")
}
pub fn take_assignment_key(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i|{let found=i.events.iter().any(|e|matches!(e,egui::Event::Key{key:Key::Comma,pressed:true,repeat:false,modifiers,..} if modifiers.ctrl&&modifiers.shift&&!modifiers.alt&&!modifiers.mac_cmd));if found{discard(i);}found})
}
pub fn take_action(ctx: &egui::Context, catalogue: &[ShortcutInfo]) -> Option<String> {
    take_action_with_editing(ctx, catalogue, ctx.egui_wants_keyboard_input())
}
pub fn take_action_with_editing(
    ctx: &egui::Context,
    catalogue: &[ShortcutInfo],
    editing: bool,
) -> Option<String> {
    ctx.input_mut(|i| {
        let found = i.events.iter().find_map(|e| match e {
            egui::Event::Copy if !editing => catalogue
                .iter()
                .find(|a| {
                    a.binding
                        .as_deref()
                        .is_some_and(|b| b.split(" / ").any(|c| c == "Ctrl C"))
                })
                .map(|a| a.id.clone()),
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                physical_key,
                ..
            } if !editing || (modifiers.ctrl && matches!(key, Key::K | Key::Comma)) => {
                let physical = physical_key.unwrap_or(*key);
                let key = if modifiers.is_none()
                    && matches!(
                        physical,
                        Key::Backtick
                            | Key::Num1
                            | Key::Num2
                            | Key::Num3
                            | Key::Num4
                            | Key::Num5
                            | Key::Num6
                            | Key::Num7
                            | Key::Num8
                            | Key::Num9
                    ) {
                    physical
                } else {
                    *key
                };
                let current = normalize(&chord(key, *modifiers)).ok()?;
                catalogue
                    .iter()
                    .find(|a| {
                        a.binding
                            .as_deref()
                            .is_some_and(|b| b.split(" / ").any(|s| s == current))
                    })
                    .map(|a| a.id.clone())
            }
            _ => None,
        });
        if found.is_some() {
            discard(i);
        }
        found
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key_event(key: Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        }
    }
    fn recorder_frame(
        ctx: &egui::Context,
        editor: &mut Editor,
        events: Vec<egui::Event>,
    ) -> Option<Intent> {
        let mut result = None;
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| result = editor.input(ui.ctx()),
        );
        out.textures_delta.clear();
        result
    }
    #[test]
    fn summon_popover_confirms_only_a_recorded_candidate() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new("overlay_hotkey", "Summon Katran", "Alt E")
            .with_physical_keys()
            .with_confirmation();
        let mut press = |key, modifiers| {
            recorder_frame(
                &ctx,
                &mut editor,
                vec![
                    key_event(key, true, modifiers),
                    key_event(key, false, modifiers),
                ],
            )
        };
        assert!(press(Key::Enter, egui::Modifiers::NONE).is_none());
        assert!(press(Key::Q, egui::Modifiers::CTRL).is_none());
        assert!(press(Key::R, egui::Modifiers::SHIFT).is_none());
        assert!(matches!(
            press(Key::Enter, egui::Modifiers::NONE),
            Some(Intent::Set { binding, replace: false }) if binding == "Shift R"
        ));
        assert_eq!(editor.current_binding, "Alt E");
    }

    #[test]
    fn summon_popover_can_replace_a_rejected_candidate_or_cancel() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new("overlay_hotkey", "Summon Katran", "Alt E")
            .with_physical_keys()
            .with_confirmation();
        recorder_frame(
            &ctx,
            &mut editor,
            vec![key_event(Key::A, true, egui::Modifiers::NONE)],
        );
        editor.failed("A".into(), "Invalid shortcut".into(), false);
        assert_eq!(editor.pending_binding.as_deref(), Some("A"));
        assert!(
            recorder_frame(
                &ctx,
                &mut editor,
                vec![key_event(Key::R, true, egui::Modifiers::ALT)]
            )
            .is_none()
        );
        assert_eq!(editor.pending_binding.as_deref(), Some("Alt R"));
        assert!(editor.error.is_none());
        assert!(matches!(
            recorder_frame(
                &ctx,
                &mut editor,
                vec![key_event(Key::Escape, true, egui::Modifiers::NONE)]
            ),
            Some(Intent::Cancel)
        ));
    }

    #[test]
    fn compact_shortcut_conflict_requires_explicit_replacement() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new("copy", "Copy", "Ctrl C").with_confirmation();
        let enter = |editor: &mut Editor| {
            recorder_frame(
                &ctx,
                editor,
                vec![
                    key_event(Key::Enter, true, egui::Modifiers::NONE),
                    key_event(Key::Enter, false, egui::Modifiers::NONE),
                ],
            )
        };
        assert!(enter(&mut editor).is_none());
        assert!(
            matches!(enter(&mut editor), Some(Intent::Set { binding, replace: false }) if binding == "Enter")
        );
        editor.failed("Enter".into(), "In use".into(), true);
        assert!(enter(&mut editor).is_none());
        assert!(
            matches!(editor.choice(0), Some(Intent::Set { binding, replace: true }) if binding == "Enter")
        );
        recorder_frame(
            &ctx,
            &mut editor,
            vec![key_event(Key::R, true, egui::Modifiers::ALT)],
        );
        assert!(!editor.replaceable);
        assert!(
            matches!(enter(&mut editor), Some(Intent::Set { binding, replace: false }) if binding == "Alt R")
        );
    }
    #[test]
    fn physical_recording_recovers_shift_and_other_held_modifiers() {
        for (held, target, expected) in [
            (
                vec![Key::ControlLeft, Key::ShiftLeft],
                Key::Slash,
                "Ctrl Shift Slash",
            ),
            (
                vec![Key::ControlRight, Key::ShiftRight],
                Key::Comma,
                "Ctrl Shift Comma",
            ),
            (vec![Key::AltLeft, Key::ShiftRight], Key::R, "Alt Shift R"),
            (
                vec![Key::ControlLeft, Key::AltRight, Key::ShiftLeft],
                Key::R,
                "Ctrl Alt Shift R",
            ),
        ] {
            let ctx = egui::Context::default();
            let mut editor = Editor::new("tab", "Tab", "").with_physical_keys();
            for modifier in held {
                assert!(
                    recorder_frame(
                        &ctx,
                        &mut editor,
                        vec![key_event(modifier, true, egui::Modifiers::NONE)]
                    )
                    .is_none()
                );
            }
            let result = recorder_frame(
                &ctx,
                &mut editor,
                vec![key_event(target, true, egui::Modifiers::NONE)],
            );
            assert!(
                matches!(result, Some(Intent::Set { binding, .. }) if binding == expected),
                "expected {expected}"
            );
        }
    }
    #[test]
    fn physical_modifiers_release_independently_and_clear_on_focus_loss() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new("tab", "Tab", "").with_physical_keys();
        for (key, pressed) in [
            (Key::ShiftLeft, true),
            (Key::ShiftRight, true),
            (Key::ShiftLeft, false),
        ] {
            assert!(
                recorder_frame(
                    &ctx,
                    &mut editor,
                    vec![key_event(key, pressed, egui::Modifiers::NONE)]
                )
                .is_none()
            );
        }
        assert!(
            matches!(recorder_frame(&ctx, &mut editor, vec![key_event(Key::R, true, egui::Modifiers::NONE)]), Some(Intent::Set { binding, .. }) if binding == "Shift R")
        );
        assert!(
            recorder_frame(
                &ctx,
                &mut editor,
                vec![
                    key_event(Key::R, false, egui::Modifiers::NONE),
                    key_event(Key::ShiftRight, false, egui::Modifiers::NONE)
                ]
            )
            .is_none()
        );
        assert!(
            matches!(recorder_frame(&ctx, &mut editor, vec![key_event(Key::Q, true, egui::Modifiers::CTRL)]), Some(Intent::Set { binding, .. }) if binding == "Ctrl Q")
        );
        assert!(
            recorder_frame(
                &ctx,
                &mut editor,
                vec![key_event(Key::ShiftLeft, true, egui::Modifiers::NONE)]
            )
            .is_none()
        );
        let mut out = ctx.run_ui(
            egui::RawInput {
                focused: false,
                ..Default::default()
            },
            |ui| {
                assert!(editor.input(ui.ctx()).is_none());
            },
        );
        out.textures_delta.clear();
        assert!(
            matches!(recorder_frame(&ctx, &mut editor, vec![key_event(Key::K, true, egui::Modifiers::CTRL)]), Some(Intent::Set { binding, .. }) if binding == "Ctrl K")
        );
    }
    #[test]
    fn physical_windows_modifier_is_released_without_sticking() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new("tab", "Tab", "").with_physical_keys();
        for (keys, expected) in [
            (vec![(Key::SuperLeft, true), (Key::Num5, true)], "Super 5"),
            (
                vec![(Key::SuperLeft, false), (Key::Num5, false), (Key::R, true)],
                "R",
            ),
        ] {
            let mut out = ctx.run_ui(egui::RawInput {
                events: keys.into_iter().map(|(key,pressed)| egui::Event::Key {
                    key,physical_key:Some(key),pressed,repeat:false,modifiers:egui::Modifiers::NONE,
                }).collect(), ..Default::default()
            }, |ui| assert!(matches!(editor.input(ui.ctx()), Some(Intent::Set {binding,..}) if binding == expected)));
            out.textures_delta.clear();
        }
    }
    #[test]
    fn alt_system_key_release_is_recorded_only_after_alt_was_pressed_in_editor() {
        for armed in [false, true] {
            let ctx = egui::Context::default();
            let mut editor = Editor::new("overlay_hotkey", "Overlay", "").with_physical_keys();
            if armed {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        events: vec![egui::Event::Key {
                            key: Key::AltLeft,
                            physical_key: Some(Key::AltLeft),
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::ALT,
                        }],
                        ..Default::default()
                    },
                    |ui| {
                        assert!(editor.input(ui.ctx()).is_none());
                    },
                );
                out.textures_delta.clear();
            }
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: Key::R,
                        physical_key: Some(Key::R),
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::ALT,
                    }],
                    ..Default::default()
                },
                |ui| {
                    let intent = editor.input(ui.ctx());
                    if armed {
                        assert!(matches!(intent,Some(Intent::Set{binding,..}) if binding=="Alt R"));
                    } else {
                        assert!(intent.is_none());
                    }
                },
            );
            out.textures_delta.clear();
        }
    }
    #[test]
    fn global_recorder_accepts_fresh_physical_chord_without_idle_frame() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new("overlay_hotkey", "Overlay", "").with_physical_keys();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::SuperLeft,
                    physical_key: Some(Key::SuperLeft),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                assert!(editor.input(ui.ctx()).is_none());
            },
        );
        output.textures_delta.clear();
        let mut output = ctx.run_ui(egui::RawInput {events: vec![egui::Event::Key {
            key:Key::K,physical_key:Some(Key::R),pressed:true,repeat:false,modifiers:egui::Modifiers::ALT,
        }], ..Default::default()}, |ui| {
            assert!(matches!(editor.input(ui.ctx()),Some(Intent::Set {binding,..}) if binding=="Alt Super R"));
            assert!(ui.ctx().input(|i|i.events.is_empty()));
        });
        output.textures_delta.clear();
    }
    #[test]
    fn enter_pastes_alt_enter_opens_and_text_fields_keep_both_keys() {
        let ctx = egui::Context::default();
        let catalogue = katran::shortcuts::Shortcuts::default().catalogue();
        for (editing, modifiers, expected) in [
            (false, egui::Modifiers::NONE, Some("paste_previous")),
            (false, egui::Modifiers::ALT, Some("open")),
            (true, egui::Modifiers::NONE, None),
            (true, egui::Modifiers::ALT, None),
        ] {
            let mut result = None;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: Key::Enter,
                        physical_key: Some(Key::Enter),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ui| result = take_action_with_editing(ui.ctx(), &catalogue, editing),
            );
            out.textures_delta.clear();
            assert_eq!(result.as_deref(), expected);
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: Key::Enter,
                        physical_key: Some(Key::Enter),
                        pressed: false,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |_| {},
            );
            out.textures_delta.clear();
        }
    }
    #[test]
    fn recorder_keeps_modifiers_and_conflict_choices() {
        assert_eq!(
            chord(Key::Comma, egui::Modifiers::CTRL | egui::Modifiers::SHIFT),
            "Ctrl Shift Comma"
        );
        let mut editor = Editor::new("save", "Сохранить", "Ctrl S");
        editor.failed("Ctrl H".into(), "Конфликт".into(), true);
        assert_eq!(
            editor.choices().iter().map(|c| c.1).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert!(
            matches!(editor.choice(0), Some(Intent::Set { binding, replace: true }) if binding == "Ctrl H")
        );
        assert!(editor.choice(1).is_none());
        assert!(editor.binding.is_none());
        assert!(
            matches!(editor.choice(2), Some(Intent::Set { binding, replace: false }) if binding.is_empty())
        );
        assert!(matches!(editor.choice(3), Some(Intent::Cancel)));
    }
}

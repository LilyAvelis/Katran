//! Input and presentation adapter shared by Actions and Settings.
use eframe::egui::{self, Key};
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
                        .unwrap_or("Ошибка назначения")
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
    release: bool,
}
impl Editor {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        current_binding: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            current_binding: current_binding.into(),
            release: true,
            ..Self::default()
        }
    }
    pub fn failed(&mut self, binding: String, message: String, replaceable: bool) {
        self.binding = replaceable.then_some(binding);
        self.error = Some(message);
        self.replaceable = replaceable;
        self.selected = replaceable.then_some(0);
        self.release = true;
    }
    fn choices(&self) -> Vec<(&'static str, usize)> {
        let mut c = vec![];
        if self.replaceable {
            c.push(("Заменить назначение", 0));
        }
        if self.binding.is_some() {
            c.push(("Другая клавиша", 1));
        }
        if !self.current_binding.is_empty() {
            c.push(("Снять назначение · Ctrl Backspace", 2));
        }
        c.push(("Отмена · Esc", 3));
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
                self.release = true;
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
            if self.release {
                let released = input.keys_down.is_empty() && input.modifiers.is_none();
                discard(input);
                self.release = !released;
                return None;
            }
            let captured = input.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if !modifier_key(*key) => Some((*key, *modifiers)),
                _ => None,
            });
            discard(input);
            let (key, m) = captured?;
            if key == Key::Escape && m.is_none() {
                return Some(Intent::Cancel);
            }
            if key == Key::Backspace && m.ctrl && !m.alt && !m.shift && !m.mac_cmd {
                return self.choice(2);
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
    pub fn render(&mut self, ui: &mut egui::Ui) -> Option<Intent> {
        ui.strong(format!("Клавиша · {}", self.label));
        ui.label(format!(
            "Текущее сочетание: {}",
            if self.current_binding.is_empty() {
                "не назначено"
            } else {
                &self.current_binding
            }
        ));
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        } else {
            ui.label("Нажмите сочетание клавиш. Оно сохранится сразу.");
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
        ui.weak("Tab — к кнопкам · стрелки — выбор · Enter — подтвердить");
        choice.and_then(|c| self.choice(c))
    }
}
fn modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::ShiftLeft
            | Key::ShiftRight
            | Key::SuperLeft
            | Key::SuperRight
    )
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
            } if !editing
                || *key == Key::Enter
                || (modifiers.ctrl && matches!(key, Key::K | Key::Comma)) =>
            {
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

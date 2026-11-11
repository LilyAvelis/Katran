//! Shared shortcut catalogue and persistent assignments for every dispatcher client.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShortcutInfo {
    pub id: String,
    pub label: String,
    pub default_binding: Option<String>,
    pub binding: Option<String>,
    pub rebindable: bool,
}
#[derive(Debug, Serialize)]
pub struct ShortcutError {
    pub code: String,
    pub message: String,
    pub conflicts: Vec<String>,
}
impl ShortcutError {
    fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
            conflicts: vec![],
        }
    }
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Shortcuts {
    overrides: BTreeMap<String, Option<String>>,
}
use crate::actions::DEFS as DEFAULTS;
/// Canonical, UI-independent representation; rejects modifiers without a main key.
pub fn normalize(text: &str) -> Result<String, ShortcutError> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    text.split('/')
        .map(|chord| {
            let mut modifiers = [false; 4];
            let mut key = None;
            let spaced = chord.replace('+', " ");
            for part in spaced.split_whitespace() {
                match part.to_ascii_lowercase().as_str() {
                    "ctrl" | "control" => modifiers[0] = true,
                    "alt" => modifiers[1] = true,
                    "shift" => modifiers[2] = true,
                    "super" | "cmd" | "command" => modifiers[3] = true,
                    _ => {
                        if key.is_some() {
                            return Err(ShortcutError::new(
                                "invalid_shortcut",
                                "Используйте одну основную клавишу",
                            ));
                        }
                        let name = match part {
                            "," => "Comma",
                            "Esc" => "Escape",
                            "↑" => "ArrowUp",
                            "↓" => "ArrowDown",
                            "←" => "ArrowLeft",
                            "→" => "ArrowRight",
                            _ => part,
                        };
                        let valid = name.len() == 1
                            && name.chars().all(|c| c.is_ascii_alphanumeric())
                            || matches!(
                                name,
                                "Enter"
                                    | "Escape"
                                    | "Space"
                                    | "Tab"
                                    | "Backspace"
                                    | "Delete"
                                    | "Insert"
                                    | "Home"
                                    | "End"
                                    | "PageUp"
                                    | "PageDown"
                                    | "ArrowUp"
                                    | "ArrowDown"
                                    | "ArrowLeft"
                                    | "ArrowRight"
                                    | "Comma"
                                    | "Period"
                                    | "Minus"
                                    | "Plus"
                                    | "Equals"
                                    | "Semicolon"
                                    | "Colon"
                                    | "Slash"
                                    | "Backslash"
                                    | "Quote"
                                    | "OpenBracket"
                                    | "CloseBracket"
                                    | "Backtick"
                            )
                            || name
                                .strip_prefix('F')
                                .and_then(|n| n.parse::<u8>().ok())
                                .is_some_and(|n| (1..=35).contains(&n));
                        if !valid {
                            return Err(ShortcutError::new(
                                "invalid_shortcut",
                                format!("Неизвестная клавиша: {name}"),
                            ));
                        }
                        key = Some(if name.len() == 1 {
                            name.to_uppercase()
                        } else {
                            name.to_owned()
                        });
                    }
                }
            }
            let key = key.ok_or_else(|| {
                ShortcutError::new(
                    "invalid_shortcut",
                    "Модификатору нужна основная клавиша, например Ctrl+H",
                )
            })?;
            let mut parts = Vec::new();
            for (enabled, name) in modifiers.into_iter().zip(["Ctrl", "Alt", "Shift", "Super"]) {
                if enabled {
                    parts.push(name.to_owned());
                }
            }
            parts.push(key);
            Ok(parts.join(" "))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|v| v.join(" / "))
}
impl Shortcuts {
    pub fn load(root: &Path) -> Result<Self, ShortcutError> {
        let path = root.join("shortcuts.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut value: Self = serde_json::from_slice(
            &fs::read(path).map_err(|e| ShortcutError::new("storage_error", e))?,
        )
        .map_err(|e| ShortcutError::new("invalid_shortcuts", e))?;
        for (id, binding) in &mut value.overrides {
            if !DEFAULTS.iter().any(|d| d.id == id && d.rebindable) {
                return Err(ShortcutError::new(
                    "invalid_shortcuts",
                    format!("Unknown or protected action: {id}"),
                ));
            }
            if let Some(binding) = binding {
                *binding = normalize(binding)?;
            }
        }
        Ok(value)
    }
    pub fn binding(&self, id: &str) -> String {
        self.overrides
            .get(id)
            .map(|b| b.clone().unwrap_or_default())
            .unwrap_or_else(|| {
                DEFAULTS
                    .iter()
                    .find(|d| d.id == id)
                    .map_or("", |d| d.default_binding)
                    .into()
            })
    }
    pub fn catalogue(&self) -> Vec<ShortcutInfo> {
        DEFAULTS
            .iter()
            .map(|definition| ShortcutInfo {
                id: definition.id.into(),
                label: definition.label.into(),
                default_binding: (!definition.default_binding.is_empty())
                    .then(|| definition.default_binding.into()),
                binding: Some(self.binding(definition.id)).filter(|s| !s.is_empty()),
                rebindable: definition.rebindable,
            })
            .collect()
    }
    pub fn set(
        &mut self,
        root: &Path,
        id: &str,
        binding: Option<String>,
        replace: bool,
        reset: bool,
    ) -> Result<Vec<ShortcutInfo>, ShortcutError> {
        let default = DEFAULTS
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| ShortcutError::new("unknown_action", id))?;
        if !default.rebindable {
            return Err(ShortcutError::new(
                "reserved_shortcut",
                "Это сочетание нельзя изменить",
            ));
        }
        let binding = normalize(if reset {
            default.default_binding
        } else {
            binding.as_deref().unwrap_or("")
        })?;
        let chords: Vec<_> = binding.split(" / ").filter(|s| !s.is_empty()).collect();
        let conflicts: Vec<_> = self
            .catalogue()
            .into_iter()
            .filter(|s| {
                s.id != id
                    && s.binding
                        .as_deref()
                        .is_some_and(|b| b.split(" / ").any(|c| chords.contains(&c)))
            })
            .collect();
        if chords.iter().any(|c| {
            let parts: Vec<_> = c.split_whitespace().collect();
            let key = parts.last().copied().unwrap_or("");
            (key == "Tab" && !(id == "transfer_selection" && *c == "Shift Tab"))
                || key == "Escape"
                || c.contains("Super")
                || (key.starts_with("Arrow") && !parts.contains(&"Ctrl") && !parts.contains(&"Alt"))
                || matches!(*c, "Ctrl Shift Comma" | "Ctrl Backspace")
        }) || conflicts.iter().any(|c| !c.rebindable)
        {
            return Err(ShortcutError::new(
                "reserved_shortcut",
                "Сочетание зарезервировано интерфейсом",
            ));
        }
        if !conflicts.is_empty() && !replace {
            return Err(ShortcutError {
                code: "shortcut_conflict".into(),
                message: format!(
                    "Сочетание уже назначено: {}. Замена снимет его с этих команд.",
                    conflicts
                        .iter()
                        .map(|c| c.label.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                conflicts: conflicts.iter().map(|c| c.id.clone()).collect(),
            });
        }
        let mut next = self.clone();
        for conflict in conflicts {
            let remaining = conflict
                .binding
                .unwrap_or_default()
                .split(" / ")
                .filter(|c| !chords.contains(c))
                .collect::<Vec<_>>()
                .join(" / ");
            next.overrides
                .insert(conflict.id, (!remaining.is_empty()).then_some(remaining));
        }
        if reset {
            next.overrides.remove(id);
        } else {
            next.overrides
                .insert(id.into(), (!binding.is_empty()).then_some(binding));
        }
        let bytes =
            serde_json::to_vec_pretty(&next).map_err(|e| ShortcutError::new("storage_error", e))?;
        let staging = root.join("shortcuts.json.tmp");
        {
            use std::io::Write;
            let mut file =
                fs::File::create(&staging).map_err(|e| ShortcutError::new("storage_error", e))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| ShortcutError::new("storage_error", e))?;
        }
        fs::rename(&staging, root.join("shortcuts.json"))
            .map_err(|e| ShortcutError::new("storage_error", e))?;
        *self = next;
        Ok(self.catalogue())
    }
}
impl std::fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ShortcutError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflicts_replace_persist_and_clear() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Shortcuts::default();
        s.set(
            root.path(),
            "save",
            Some("Ctrl+Shift+H".into()),
            false,
            false,
        )
        .unwrap();
        assert!(
            s.set(
                root.path(),
                "gallery",
                Some("Ctrl Shift H".into()),
                false,
                false
            )
            .is_err()
        );
        assert_eq!(s.binding("save"), "Ctrl Shift H");
        s.set(
            root.path(),
            "gallery",
            Some("Ctrl Shift H".into()),
            true,
            false,
        )
        .unwrap();
        assert_eq!(s.binding("save"), "");
        assert_eq!(
            Shortcuts::load(root.path()).unwrap().binding("gallery"),
            "Ctrl Shift H"
        );
        s.set(root.path(), "gallery", None, false, false).unwrap();
        assert_eq!(s.binding("gallery"), "");
    }
    #[test]
    fn modifiers_and_reserved() {
        assert!(normalize("Ctrl Shift").is_err());
        assert_eq!(normalize("Ctrl+Shift+Comma").unwrap(), "Ctrl Shift Comma");
        let root = tempfile::tempdir().unwrap();
        assert!(
            Shortcuts::default()
                .set(root.path(), "save", Some("Ctrl K".into()), true, false)
                .is_err()
        );
    }
}

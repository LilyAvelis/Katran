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
#[derive(Clone, Serialize, Deserialize)]
pub struct Shortcuts {
    #[serde(default)]
    version: u32,
    overrides: BTreeMap<String, Option<String>>,
}
impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            version: 1,
            overrides: BTreeMap::new(),
        }
    }
}
use crate::actions::DEFS as DEFAULTS;
/// Katran wire-compatible facade over the reusable pure policy crate.
pub fn normalize(text: &str) -> Result<String, ShortcutError> {
    garden_actions::normalize(text).map_err(Into::into)
}
fn definitions() -> Vec<garden_actions::Definition<'static>> {
    DEFAULTS
        .iter()
        .map(|d| garden_actions::Definition {
            id: d.id,
            label: d.label,
            default_binding: d.default_binding,
            rebindable: d.rebindable,
        })
        .collect()
}
fn reserved(id: &str, chord: &str) -> bool {
    let parts: Vec<_> = chord.split_whitespace().collect();
    let key = parts.last().copied().unwrap_or("");
    (key == "Tab"
        && !((id == "transfer_selection" && chord == "Shift Tab")
            || (id == "transfer_copy" && chord == "Ctrl Tab")))
        || key == "Escape"
        || chord.contains("Super")
        || (key.starts_with("Arrow") && !parts.contains(&"Ctrl") && !parts.contains(&"Alt"))
        || matches!(chord, "Ctrl Shift Comma" | "Ctrl Backspace")
}
impl From<garden_actions::Error> for ShortcutError {
    fn from(error: garden_actions::Error) -> Self {
        use garden_actions::Error;
        match error {
            Error::UnknownAction { id } => Self::new("unknown_action", id),
            Error::ProtectedAction { .. } => {
                Self::new("reserved_shortcut", "Это сочетание нельзя изменить")
            }
            Error::InvalidShortcut { message } => Self::new("invalid_shortcut", message),
            Error::ReservedShortcut { .. } => {
                Self::new("reserved_shortcut", "Сочетание зарезервировано интерфейсом")
            }
            Error::Conflict { conflicts } => Self {
                code: "shortcut_conflict".into(),
                message: format!(
                    "Сочетание уже назначено: {}. Замена снимет его с этих команд.",
                    conflicts
                        .iter()
                        .map(|c| c.label.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                conflicts: conflicts.into_iter().map(|c| c.id).collect(),
            },
        }
    }
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
        value.overrides = garden_actions::validate_overrides(&definitions(), &value.overrides)
            .map_err(|error| match error {
                garden_actions::Error::UnknownAction { id }
                | garden_actions::Error::ProtectedAction { id } => ShortcutError::new(
                    "invalid_shortcuts",
                    format!("Unknown or protected action: {id}"),
                ),
                other => other.into(),
            })?;
        if value.version == 0 {
            value.migrate_enter_bindings();
            value.version = 1;
        }
        Ok(value)
    }
    fn migrate_enter_bindings(&mut self) {
        if self
            .overrides
            .get("paste_previous")
            .and_then(Option::as_deref)
            .is_some_and(|binding| matches!(binding, "Comma" | "Alt Enter"))
        {
            self.overrides.remove("paste_previous");
        }
        if self.overrides.get("open").and_then(Option::as_deref) == Some("Enter") {
            self.overrides.remove("open");
        }
        // Explicit assignments, including earlier replacements, belong to their
        // user. Never introduce a conflicting new default over those bindings.
        for (id, chord) in [("paste_previous", "Enter"), ("open", "Alt Enter")] {
            if !self.overrides.contains_key(id)
                && self.overrides.iter().any(|(other, value)| {
                    other != id
                        && value
                            .as_deref()
                            .is_some_and(|binding| binding.split(" / ").any(|part| part == chord))
                })
            {
                self.overrides.insert(id.into(), None);
            }
        }
    }
    pub fn binding(&self, id: &str) -> String {
        garden_actions::binding(&definitions(), &self.overrides, id).to_owned()
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
        let assignment = if reset {
            garden_actions::Assignment::Reset
        } else {
            garden_actions::Assignment::Set(binding.as_deref())
        };
        let overrides = garden_actions::plan_set(
            &definitions(),
            &self.overrides,
            id,
            assignment,
            replace,
            reserved,
        )?;
        let mut next = self.clone();
        next.overrides = overrides;
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

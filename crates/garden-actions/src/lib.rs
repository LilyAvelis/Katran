//! Host-supplied shortcut definitions and pure assignment policy.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Definition<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub default_binding: &'a str,
    pub rebindable: bool,
}
pub type Overrides = BTreeMap<String, Option<String>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    UnknownAction {
        id: String,
    },
    ProtectedAction {
        id: String,
    },
    InvalidShortcut {
        message: String,
    },
    ReservedShortcut {
        chord: Option<String>,
        conflicts: Vec<Conflict>,
    },
    Conflict {
        conflicts: Vec<Conflict>,
    },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAction { id } => write!(f, "Unknown action: {id}"),
            Self::ProtectedAction { id } => write!(f, "Protected action: {id}"),
            Self::InvalidShortcut { message } => f.write_str(message),
            Self::ReservedShortcut { .. } => f.write_str("Shortcut is reserved"),
            Self::Conflict { conflicts } => write!(
                f,
                "Shortcut conflicts with: {}",
                conflicts
                    .iter()
                    .map(|c| c.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assignment<'a> {
    Set(Option<&'a str>),
    Reset,
}

/// Canonical, UI-independent representation; rejects modifiers without a main key.
pub fn normalize(text: &str) -> Result<String, Error> {
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
                            return Err(Error::InvalidShortcut {
                                message: "Используйте одну основную клавишу".into(),
                            });
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
                            return Err(Error::InvalidShortcut {
                                message: format!("Неизвестная клавиша: {name}"),
                            });
                        }
                        key = Some(if name.len() == 1 {
                            name.to_uppercase()
                        } else {
                            name.to_owned()
                        });
                    }
                }
            }
            let key = key.ok_or_else(|| Error::InvalidShortcut {
                message: "Модификатору нужна основная клавиша, например Ctrl+H".into(),
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
/// Return the effective shortcut, or the empty string when unbound or unknown.
pub fn binding<'a>(catalogue: &'a [Definition<'a>], overrides: &'a Overrides, id: &str) -> &'a str {
    overrides
        .get(id)
        .map(|value| value.as_deref().unwrap_or(""))
        .unwrap_or_else(|| {
            catalogue
                .iter()
                .find(|d| d.id == id)
                .map_or("", |d| d.default_binding)
        })
}

/// Normalize stored overrides. Loading intentionally does not reject conflicts or reserved chords.
pub fn validate_overrides(
    catalogue: &[Definition<'_>],
    current: &Overrides,
) -> Result<Overrides, Error> {
    let mut result = Overrides::new();
    for (id, value) in current {
        let definition = catalogue
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| Error::UnknownAction { id: id.clone() })?;
        if !definition.rebindable {
            return Err(Error::ProtectedAction { id: id.clone() });
        }
        result.insert(id.clone(), value.as_deref().map(normalize).transpose()?);
    }
    Ok(result)
}

/// Plan a change without mutating the input; persistence is the host's responsibility.
pub fn plan_set(
    catalogue: &[Definition<'_>],
    current: &Overrides,
    id: &str,
    assignment: Assignment<'_>,
    replace: bool,
    reserved: impl Fn(&str, &str) -> bool,
) -> Result<Overrides, Error> {
    let definition = catalogue
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| Error::UnknownAction { id: id.into() })?;
    if !definition.rebindable {
        return Err(Error::ProtectedAction { id: id.into() });
    }
    let text = match assignment {
        Assignment::Set(value) => value.unwrap_or(""),
        Assignment::Reset => definition.default_binding,
    };
    let normalized = normalize(text)?;
    let chords: Vec<_> = normalized.split(" / ").filter(|c| !c.is_empty()).collect();
    let overlapping: Vec<_> = catalogue
        .iter()
        .filter(|d| {
            d.id != id
                && binding(catalogue, current, d.id)
                    .split(" / ")
                    .any(|chord| chords.contains(&chord))
        })
        .collect();
    let conflicts: Vec<_> = overlapping
        .iter()
        .map(|d| Conflict {
            id: d.id.into(),
            label: d.label.into(),
        })
        .collect();
    if let Some(chord) = chords.iter().find(|chord| reserved(id, chord)) {
        return Err(Error::ReservedShortcut {
            chord: Some((*chord).into()),
            conflicts,
        });
    }
    if overlapping.iter().any(|d| !d.rebindable) {
        return Err(Error::ReservedShortcut {
            chord: None,
            conflicts,
        });
    }
    if !conflicts.is_empty() && !replace {
        return Err(Error::Conflict { conflicts });
    }
    let mut next = current.clone();
    for other in overlapping {
        let remaining = binding(catalogue, current, other.id)
            .split(" / ")
            .filter(|chord| !chords.contains(chord))
            .collect::<Vec<_>>()
            .join(" / ");
        next.insert(
            other.id.into(),
            (!remaining.is_empty()).then_some(remaining),
        );
    }
    match assignment {
        Assignment::Reset => {
            next.remove(id);
        }
        Assignment::Set(_) => {
            next.insert(id.into(), (!normalized.is_empty()).then_some(normalized));
        }
    }
    Ok(next)
}

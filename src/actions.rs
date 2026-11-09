//! Single catalogue for Actions, shortcuts, grouping and dispatch validation.
use crate::{
    Command,
    interaction::{Interaction, View},
};
pub type Definition = (
    &'static str,
    &'static str,
    &'static str,
    bool,
    &'static str,
    bool,
);
pub const DEFS: &[Definition] = &[
    ("cleanup", "Повторить очистку кэша", "", true, "Полка", true),
    ("open", "Открыть изображение", "Enter", true, "Файл", true),
    ("copy", "Копировать выбранное", "Ctrl C", true, "Файл", true),
    ("delete", "Убрать с полки", "Delete", true, "Файл", true),
    (
        "save",
        "Сохранить выбранное в папку…",
        "",
        true,
        "Файл",
        true,
    ),
    ("gallery", "Вид: галерея", "", true, "Вид", true),
    ("list", "Вид: поток", "", true, "Вид", true),
    (
        "capture",
        "Приостановить / включить захват",
        "",
        true,
        "Полка",
        true,
    ),
    ("refresh", "Обновить", "F5", true, "Полка", true),
    (
        "settings",
        "Настройки",
        "Ctrl Comma",
        false,
        "Приложение",
        true,
    ),
    ("commands", "Actions", "Ctrl K", false, "Приложение", false),
];
pub enum Dispatch {
    Command(Command),
    Intent(serde_json::Value),
}
pub fn resolve(id: &str, state: &Interaction, capture: bool) -> Option<Dispatch> {
    let selected = || {
        if state.selected.is_empty() {
            state.cursor.into_iter().collect()
        } else {
            state.selected.clone()
        }
    };
    Some(match id {
        "cleanup" => Dispatch::Command(Command::MaintainCache),
        "open" => Dispatch::Command(Command::OpenCurrent),
        "copy" => Dispatch::Command(Command::CopyEvents { ids: selected() }),
        "delete" => Dispatch::Command(Command::DeleteEvents {
            ids: selected(),
            confirmed: false,
        }),
        "save" => {
            Dispatch::Intent(serde_json::json!({"intent":"choose_destination","ids":selected()}))
        }
        "gallery" => Dispatch::Command(Command::SetView {
            view: View::Gallery,
        }),
        "list" => Dispatch::Command(Command::SetView { view: View::List }),
        "capture" => Dispatch::Command(Command::SetCaptureEnabled { enabled: !capture }),
        "refresh" => Dispatch::Command(Command::List { limit: 1000 }),
        "settings" => Dispatch::Command(Command::ShowSettings),
        "commands" => Dispatch::Intent(serde_json::json!({"intent":"show_commands"})),
        _ => return None,
    })
}

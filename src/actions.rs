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
        "quicklook",
        "Открыть через QuickLook",
        "Ctrl Enter",
        true,
        "Файл",
        true,
    ),
    ("open", "Открыть просмотр", "Enter", true, "Файл", true),
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
    (
        "density_more",
        "Больше карточек",
        "Ctrl Minus",
        true,
        "Вид",
        true,
    ),
    (
        "density_less",
        "Крупнее карточки",
        "Ctrl Plus / Ctrl Equals / Ctrl Shift Plus / Ctrl Shift Equals",
        true,
        "Вид",
        true,
    ),
    ("tab_position_1", "Tab position 1", "1", true, "Вид", false),
    ("tab_position_2", "Tab position 2", "2", true, "Вид", false),
    ("tab_position_3", "Tab position 3", "3", true, "Вид", false),
    ("tab_position_4", "Tab position 4", "4", true, "Вид", false),
    ("tab_position_5", "Tab position 5", "5", true, "Вид", false),
    ("tab_position_6", "Tab position 6", "6", true, "Вид", false),
    ("tab_position_7", "Tab position 7", "7", true, "Вид", false),
    ("tab_position_8", "Tab position 8", "8", true, "Вид", false),
    ("tab_position_9", "Tab position 9", "9", true, "Вид", false),
    ("downloads", "Downloads", "", true, "Вид", true),
    (
        "tab_toggle",
        "All / предыдущий таб",
        "Tab",
        false,
        "Вид",
        true,
    ),
    ("all", "Таб: All", "", true, "Вид", true),
    ("images", "Таб: Images", "", true, "Вид", true),
    ("screenshots", "Таб: Screenshots", "", true, "Вид", true),
    ("preview", "Просмотр / полка", "Space", true, "Вид", true),
    ("gallery", "Вид: галерея", "", true, "Вид", true),
    ("list", "Вид: поток", "", true, "Вид", true),
    (
        "settings",
        "Настройки",
        "Ctrl Comma",
        true,
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
    if let Some(position) = id
        .strip_prefix("tab_position_")
        .and_then(|p| p.parse::<u8>().ok())
        .filter(|p| (1..=9).contains(p))
    {
        return Some(Dispatch::Command(Command::SelectTabPosition { position }));
    }
    Some(match id {
        "quicklook" => Dispatch::Command(Command::PreviewExternal {
            id: state
                .cursor
                .or_else(|| state.selected.first().copied())
                .unwrap_or(0),
        }),
        "density_more" => Dispatch::Command(Command::AdjustDensity { delta: 1 }),
        "density_less" => Dispatch::Command(Command::AdjustDensity { delta: -1 }),
        "tab_toggle" => Dispatch::Command(Command::ToggleTab),
        "all" => Dispatch::Command(Command::SetTab {
            tab: crate::interaction::ShelfTab::All,
        }),
        "images" => Dispatch::Command(Command::SetTab {
            tab: crate::interaction::ShelfTab::Images,
        }),
        "downloads" => Dispatch::Command(Command::SetTab {
            tab: crate::interaction::ShelfTab::Downloads,
        }),
        "screenshots" => Dispatch::Command(Command::SetTab {
            tab: crate::interaction::ShelfTab::Screenshots,
        }),
        "preview" => Dispatch::Command(Command::TogglePreview),
        "cleanup" => Dispatch::Command(Command::MaintainCache),
        "open" => Dispatch::Command(Command::OpenCurrent),
        "copy" => Dispatch::Command(Command::CopyEvents { ids: selected() }),
        "delete" => Dispatch::Command(Command::DeleteSelection {
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

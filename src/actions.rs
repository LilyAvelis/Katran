//! Single catalogue for Actions, shortcuts, grouping and dispatch validation.
use crate::{
    Command,
    interaction::{Interaction, View},
};
#[derive(Clone, Copy, Debug)]
pub struct Definition {
    pub id: &'static str,
    pub label: &'static str,
    pub default_binding: &'static str,
    pub rebindable: bool,
    pub group: &'static str,
    pub in_palette: bool,
}
pub const DEFS: &[Definition] = &[
    Definition {id:"transfer_selection",label:"Перенести между полкой и Folder",default_binding:"Shift Tab",rebindable:true,group:"Передача",in_palette:true},
    Definition {id:"add_to_katran",label:"Добавить в Катран",default_binding:"Ctrl Shift C",rebindable:true,group:"Передача",in_palette:true},
    Definition {
        id: "tables",
        label: "Таб: Tables",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "toggle_tables",
        label: "Включить / отключить Tables",
        default_binding: "",
        rebindable: true,
        group: "Вкладки",
        in_palette: false,
    },
    Definition {
        id: "sort_arrival",
        label: "Сортировать по поступлению",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_arrival_reverse",
        label: "Сортировать по поступлению · обратно",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_extension",
        label: "Сортировать по расширению",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_extension_reverse",
        label: "Сортировать по расширению · обратно",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_modified",
        label: "Сортировать по дате изменения",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_modified_reverse",
        label: "Сортировать по дате изменения · обратно",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_shuffle",
        label: "Перемешать",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_natural",
        label: "Сортировать по естественному имени",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_natural_reverse",
        label: "Сортировать по естественному имени · обратно",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_size",
        label: "Сортировать по размеру",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_size_reverse",
        label: "Сортировать по размеру · обратно",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_name",
        label: "Сортировать по имени",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_name_reverse",
        label: "Сортировать по имени · обратно",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_origin",
        label: "Сортировать по источнику",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "sort_origin_reverse",
        label: "Сортировать по источнику · обратно",
        default_binding: "",
        rebindable: true,
        group: "Сортировка",
        in_palette: true,
    },
    Definition {
        id: "text",
        label: "Таб: Text",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "toggle_text",
        label: "Включить / отключить Text",
        default_binding: "",
        rebindable: true,
        group: "Вкладки",
        in_palette: false,
    },
    Definition {
        id: "links",
        label: "Открыть Links",
        default_binding: "",
        rebindable: true,
        group: "Вкладки",
        in_palette: false,
    },
    Definition {
        id: "toggle_links",
        label: "Включить / отключить Links",
        default_binding: "",
        rebindable: true,
        group: "Вкладки",
        in_palette: false,
    },
    Definition {
        id: "copy_text",
        label: "Копировать тексты одним текстом",
        default_binding: "",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "reveal",
        label: "Показать в Проводнике",
        default_binding: "Ctrl Enter",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "open_with",
        label: "Открыть с помощью…",
        default_binding: "Ctrl Shift Enter",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "tabs_settings",
        label: "Настройки вкладок",
        default_binding: "",
        rebindable: true,
        group: "Приложение",
        in_palette: true,
    },
    Definition {
        id: "toggle_images",
        label: "Включить / отключить Images",
        default_binding: "",
        rebindable: true,
        group: "Вкладки",
        in_palette: false,
    },
    Definition {
        id: "toggle_screenshots",
        label: "Включить / отключить Screenshots",
        default_binding: "",
        rebindable: true,
        group: "Вкладки",
        in_palette: false,
    },
    Definition {
        id: "toggle_downloads",
        label: "Включить / отключить Downloads",
        default_binding: "",
        rebindable: true,
        group: "Вкладки",
        in_palette: false,
    },
    Definition {
        id: "cleanup",
        label: "Повторить очистку кэша",
        default_binding: "",
        rebindable: true,
        group: "Полка",
        in_palette: true,
    },
    Definition {
        id: "capture",
        label: "Приостановить / включить захват",
        default_binding: "",
        rebindable: true,
        group: "Полка",
        in_palette: true,
    },
    Definition {
        id: "refresh",
        label: "Обновить",
        default_binding: "F5",
        rebindable: true,
        group: "Полка",
        in_palette: true,
    },
    Definition {
        id: "quicklook",
        label: "Открыть через QuickLook",
        default_binding: "",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "open",
        label: "Открыть приложением по умолчанию",
        default_binding: "Enter",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "copy",
        label: "Копировать выбранное",
        default_binding: "Ctrl C",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "delete",
        label: "Убрать с полки",
        default_binding: "Delete",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "save",
        label: "Сохранить выбранное в папку…",
        default_binding: "",
        rebindable: true,
        group: "Файл",
        in_palette: true,
    },
    Definition {
        id: "density_more",
        label: "Больше карточек",
        default_binding: "Ctrl Minus",
        rebindable: true,
        group: "Вид",
        in_palette: true,
    },
    Definition {
        id: "density_less",
        label: "Крупнее карточки",
        default_binding: "Ctrl Plus / Ctrl Equals / Ctrl Shift Plus / Ctrl Shift Equals",
        rebindable: true,
        group: "Вид",
        in_palette: true,
    },
    Definition {
        id: "tab_position_1",
        label: "Tab position 1",
        default_binding: "1",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_2",
        label: "Tab position 2",
        default_binding: "2",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_3",
        label: "Tab position 3",
        default_binding: "3",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_4",
        label: "Tab position 4",
        default_binding: "4",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_5",
        label: "Tab position 5",
        default_binding: "5",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_6",
        label: "Tab position 6",
        default_binding: "6",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_7",
        label: "Tab position 7",
        default_binding: "7",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_8",
        label: "Tab position 8",
        default_binding: "8",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_position_9",
        label: "Tab position 9",
        default_binding: "9",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "downloads",
        label: "Downloads",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "tab_toggle",
        label: "All / предыдущий таб",
        default_binding: "Tab",
        rebindable: false,
        group: "Вид",
        in_palette: true,
    },
    Definition {
        id: "all",
        label: "Таб: Folder",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "images",
        label: "Таб: Images",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "screenshots",
        label: "Таб: Screenshots",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: false,
    },
    Definition {
        id: "preview",
        label: "Просмотр / полка",
        default_binding: "Space",
        rebindable: true,
        group: "Вид",
        in_palette: true,
    },
    Definition {
        id: "gallery",
        label: "Вид: галерея",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: true,
    },
    Definition {
        id: "list",
        label: "Вид: поток",
        default_binding: "",
        rebindable: true,
        group: "Вид",
        in_palette: true,
    },
    Definition {
        id: "settings",
        label: "Настройки",
        default_binding: "Ctrl Comma",
        rebindable: true,
        group: "Приложение",
        in_palette: true,
    },
    Definition {
        id: "commands",
        label: "Actions",
        default_binding: "Ctrl K",
        rebindable: false,
        group: "Приложение",
        in_palette: false,
    },
];
pub enum Dispatch {
    Command(Command),
    Intent(serde_json::Value),
}
pub fn resolve(id: &str, state: &Interaction, capture: bool) -> Option<Dispatch> {
    if let Some((mode, reverse)) = crate::sorting::action_sort(id) {
        return Some(Dispatch::Command(Command::SetSort {
            mode,
            reverse,
            seed: None,
        }));
    }
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
        "transfer_selection" => Dispatch::Command(Command::TransferSelection),
        "add_to_katran" => Dispatch::Command(Command::AddFolderFiles {ids:selected()}),
        "tables" => Dispatch::Command(Command::SetTab {
            tab: crate::interaction::ShelfTab::Tables,
        }),
        "toggle_tables" => Dispatch::Command(Command::ToggleTabEnabled {
            tab: crate::interaction::ShelfTab::Tables,
        }),
        "links" => Dispatch::Command(Command::SetTab {
            tab: crate::interaction::ShelfTab::Links,
        }),
        "toggle_links" => Dispatch::Command(Command::ToggleTabEnabled {
            tab: crate::interaction::ShelfTab::Links,
        }),
        "text" => Dispatch::Command(Command::SetTab {
            tab: crate::interaction::ShelfTab::Text,
        }),
        "toggle_text" => Dispatch::Command(Command::ToggleTabEnabled {
            tab: crate::interaction::ShelfTab::Text,
        }),
        "copy_text" => Dispatch::Command(Command::CopySelectedText),
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
        "open_with" => Dispatch::Command(Command::ListOpenApps {
            id: state
                .cursor
                .or_else(|| state.selected.first().copied())
                .unwrap_or(0),
        }),
        "open" | "reveal" => Dispatch::Command(Command::OpenFile {
            id: state
                .cursor
                .or_else(|| state.selected.first().copied())
                .unwrap_or(0),
            operation: match id {
                "reveal" => crate::open_file::Operation::Reveal,
                "open_with" => crate::open_file::Operation::OpenWith,
                _ => crate::open_file::Operation::Open,
            },
        }),
        "tabs_settings" => Dispatch::Command(Command::ShowTabSettings),
        "toggle_images" => Dispatch::Command(Command::ToggleTabEnabled {
            tab: crate::interaction::ShelfTab::Images,
        }),
        "toggle_screenshots" => Dispatch::Command(Command::ToggleTabEnabled {
            tab: crate::interaction::ShelfTab::Screenshots,
        }),
        "toggle_downloads" => Dispatch::Command(Command::ToggleTabEnabled {
            tab: crate::interaction::ShelfTab::Downloads,
        }),
        "copy" => Dispatch::Command(Command::CopySelectedObjects),
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

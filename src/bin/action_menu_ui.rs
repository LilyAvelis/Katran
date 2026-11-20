use super::i18n::{is_english, tr};
use eframe::egui;
use egui_lucide::Lucide;
use katran::{
    actions,
    settings::{ActionMenu, ActionMenuGroup},
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) const READER_COMMANDS: &[(&str, &str, &str)] = &[
    (
        "notion_reader_background",
        "Фон страницы…",
        "Page background…",
    ),
    (
        "notion_reader_landscape",
        "Переключить Landscape",
        "Toggle Landscape",
    ),
    (
        "notion_reader_cinema",
        "Переключить Cinema",
        "Toggle Cinema",
    ),
];
pub(super) const GROUP_LABELS: (&str, &str) = ("Меню действий", "Actions menu");

struct Action {
    id: &'static str,
    group: &'static str,
    group_en: &'static str,
    label: &'static str,
}

fn catalogue() -> Vec<Action> {
    let mut definitions: Vec<_> = actions::DEFS
        .iter()
        .filter(|action| action.in_palette)
        .collect();
    definitions.sort_by_key(|action| actions::palette_order(action.id));
    let mut result: Vec<_> = definitions
        .into_iter()
        .map(|action| Action {
            id: action.id,
            group: actions::palette_group(action.id, action.group),
            group_en: actions::english_group(action.id, action.group),
            label: tr(
                actions::palette_label(action.id, action.label),
                actions::english_label(action.id, action.label),
            ),
        })
        .collect();
    result.push(Action {
        id: "sort_notion_order",
        group: "Сортировка",
        group_en: "Sorting",
        label: tr("Порядок Notion", "Notion order"),
    });
    result.extend(READER_COMMANDS.iter().map(|&(id, ru, en)| Action {
        id,
        group: "Читалка",
        group_en: "Reader",
        label: tr(ru, en),
    }));
    result
}

fn resolved_groups(menu: &ActionMenu, catalogue: &[Action]) -> Vec<ActionMenuGroup> {
    let mut groups = menu.groups.clone();
    let mut placed: HashSet<String> = groups
        .iter()
        .flat_map(|group| group.commands.iter().cloned())
        .collect();
    for action in catalogue {
        if placed.insert(action.id.to_owned()) {
            let index = groups
                .iter()
                .position(|group| group.id == action.group)
                .unwrap_or_else(|| {
                    groups.push(ActionMenuGroup {
                        id: action.group.into(),
                        ..Default::default()
                    });
                    groups.len() - 1
                });
            groups[index].commands.push(action.id.into());
        }
    }
    groups
}

fn group_title<'a>(group: &'a ActionMenuGroup, catalogue: &[Action]) -> &'a str {
    group.title.as_deref().unwrap_or_else(|| {
        if is_english()
            && let Some(action) = catalogue.iter().find(|action| action.group == group.id)
        {
            action.group_en
        } else {
            &group.id
        }
    })
}

/// The dispatcher and other command surfaces continue to receive the full catalogue.
pub(super) fn customize(actions: &mut Vec<Value>, menu: &ActionMenu) {
    if menu.is_default() {
        return;
    }
    let catalogue = catalogue();
    let groups = resolved_groups(menu, &catalogue);
    let placements: HashMap<_, _> = groups
        .iter()
        .enumerate()
        .flat_map(|(group_index, group)| {
            group
                .commands
                .iter()
                .enumerate()
                .map(move |(index, id)| (id.as_str(), (group_index, index)))
        })
        .collect();
    let mut ordered = Vec::with_capacity(actions.len());
    for mut action in actions.drain(..) {
        let id = action["id"].as_str().unwrap_or("");
        let visible = menu.command_visibility.get(id).copied();
        if visible == Some(false) {
            continue;
        }
        let placement = placements.get(id).copied().unwrap_or((groups.len(), 0));
        if let Some(group) = groups.get(placement.0) {
            if group.hidden {
                continue;
            }
            action["group"] = group_title(group, &catalogue).into();
            action["menu_group_id"] = group.id.as_str().into();
        }
        if visible == Some(true) {
            action["show_in_empty_query"] = true.into();
        }
        ordered.push((placement, action));
    }
    ordered.sort_by_key(|(placement, _)| *placement);
    actions.extend(ordered.into_iter().map(|(_, action)| action));
}

struct MoveCommand {
    command: String,
    group: usize,
    neighbor: Option<(String, bool)>,
}

fn move_command(groups: &mut [ActionMenuGroup], movement: MoveCommand) {
    for group in &mut *groups {
        group
            .commands
            .retain(|command| *command != movement.command);
    }
    let commands = &mut groups[movement.group].commands;
    let index = movement
        .neighbor
        .and_then(|(id, after)| {
            commands
                .iter()
                .position(|command| *command == id)
                .map(|index| index + usize::from(after))
        })
        .unwrap_or(commands.len());
    commands.insert(index, movement.command);
}

#[derive(Clone)]
struct DragCommand(String);

#[derive(Clone)]
struct DragGroup(usize);

fn move_group(groups: &mut Vec<ActionMenuGroup>, source: usize, target: usize, after: bool) {
    let group = groups.remove(source);
    let target = target - usize::from(source < target);
    groups.insert(target + usize::from(after), group);
}

fn paint_drop_edge(ui: &egui::Ui, rect: egui::Rect, after: bool) {
    let y = if after { rect.bottom() } else { rect.top() };
    ui.painter()
        .hline(rect.x_range(), y, ui.visuals().selection.stroke);
}

fn grip(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    ui.add(
        egui::Button::image(Lucide::GripVertical.size(16.0).image())
            .frame(false)
            .sense(egui::Sense::drag()),
    )
    .on_hover_cursor(egui::CursorIcon::Grab)
    .on_hover_text(tooltip)
}

fn eye(ui: &mut egui::Ui, visible: bool, tooltip: &str) -> egui::Response {
    let icon = if visible { Lucide::Eye } else { Lucide::EyeOff };
    ui.add(egui::Button::image(icon.size(16.0).image()))
        .on_hover_text(tooltip)
}

#[derive(Default)]
pub(super) struct Editor {
    rename: Option<(String, String)>,
}

impl Editor {
    pub(super) fn show(&mut self, ui: &mut egui::Ui, menu: &mut ActionMenu) {
        let catalogue = catalogue();
        let mut groups = resolved_groups(menu, &catalogue);
        let mut edited_groups = false;
        let mut movement = None;
        let mut category_drop = None;
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(format!("+ {}", tr("Категория", "Category")))
                .clicked()
            {
                let number = (1..)
                    .find(|n| !groups.iter().any(|group| group.id == format!("custom/{n}")))
                    .unwrap();
                let id = format!("custom/{number}");
                let title = format!("{} {number}", tr("Категория", "Category"));
                groups.push(ActionMenuGroup {
                    id: id.clone(),
                    title: Some(title.clone()),
                    ..Default::default()
                });
                self.rename = Some((id, title));
                edited_groups = true;
            }
            if ui
                .add_enabled(
                    !menu.is_default(),
                    egui::Button::new(tr("Сбросить меню", "Reset menu")),
                )
                .clicked()
            {
                *menu = ActionMenu::default();
                self.rename = None;
                groups = resolved_groups(menu, &catalogue);
                edited_groups = false;
            }
        });
        ui.label(tr(
            "Глазик скрывает категорию, иконка команды — саму команду. Сочетания клавиш продолжают работать.",
            "Click the eye to hide a category or a command’s icon to hide that command. Keyboard shortcuts still work.",
        ));
        for (index, group) in groups.iter_mut().enumerate() {
            let title = group_title(group, &catalogue).to_owned();
            let id = ui.make_persistent_id(("action-menu-category", &group.id));
            let header = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                false,
            )
            .show_header(ui, |ui| {
                grip(ui, tr("Перетащить категорию", "Drag category"))
                    .dnd_set_drag_payload(DragGroup(index));
                if eye(
                    ui,
                    !group.hidden,
                    tr(
                        "Показать или скрыть категорию в меню",
                        "Show or hide category in the menu",
                    ),
                )
                .clicked()
                {
                    group.hidden = !group.hidden;
                    edited_groups = true;
                }
                if let Some((_, name)) = self
                    .rename
                    .as_mut()
                    .filter(|(target, _)| *target == group.id)
                {
                    ui.add(
                        egui::TextEdit::singleline(name)
                            .char_limit(64)
                            .desired_width(180.0),
                    );
                    if ui.button(tr("Сохранить", "Save")).clicked() && !name.trim().is_empty()
                    {
                        group.title = Some(name.trim().to_owned());
                        self.rename = None;
                        edited_groups = true;
                    }
                    if ui.button(tr("Отмена", "Cancel")).clicked() {
                        self.rename = None;
                    }
                } else {
                    ui.label(egui::RichText::new(&title).strong());
                    if ui
                        .add(egui::Button::image(Lucide::Pencil.size(14.0).image()).frame(false))
                        .on_hover_text(tr("Переименовать категорию", "Rename category"))
                        .clicked()
                    {
                        self.rename = Some((group.id.clone(), title));
                    }
                    if group.id.starts_with("custom/")
                        && group.commands.is_empty()
                        && ui
                            .add(
                                egui::Button::image(Lucide::Trash2.size(14.0).image()).frame(false),
                            )
                            .on_hover_text(tr("Удалить пустую категорию", "Delete empty category"))
                            .clicked()
                    {
                        remove = Some(index);
                    }
                }
                ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
            });
            let (_, header, _) = header.body(|ui| {
                for command in &group.commands {
                    let Some(action) = catalogue.iter().find(|action| action.id == command) else {
                        continue;
                    };
                    let row = ui
                        .push_id(command, |ui| {
                            ui.horizontal(|ui| {
                                let handle = grip(ui, tr("Перетащить команду", "Drag command"));
                                handle.dnd_set_drag_payload(DragCommand(command.clone()));
                                let visible = menu
                                    .command_visibility
                                    .get(command)
                                    .copied()
                                    .unwrap_or(true);
                                let icon_color = if visible {
                                    ui.visuals().text_color()
                                } else {
                                    ui.visuals().error_fg_color
                                };
                                let icon = super::action_icons::icon(command)
                                    .size(16.0)
                                    .image()
                                    .tint(icon_color);
                                let hint = if visible {
                                    tr("Скрыть команду в меню", "Hide command in the menu")
                                } else {
                                    tr("Показать команду в меню", "Show command in the menu")
                                };
                                let toggle = ui.add(egui::Button::image(icon)).on_hover_text(hint);
                                toggle.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        format!("{} · {hint}", action.label),
                                    )
                                });
                                if toggle.clicked() {
                                    menu.command_visibility.insert(command.clone(), !visible);
                                }
                                let label = egui::RichText::new(action.label);
                                ui.label(if visible {
                                    label
                                } else {
                                    label.weak().strikethrough()
                                });
                                ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
                            })
                            .response
                        })
                        .inner;
                    let target = ui.interact(row.rect, row.id.with("drop"), egui::Sense::hover());
                    if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                        let after = pointer.y >= row.rect.center().y;
                        if target
                            .dnd_hover_payload::<DragCommand>()
                            .is_some_and(|drag| drag.0 != *command)
                        {
                            paint_drop_edge(ui, row.rect, after);
                        }
                        if let Some(drag) = target.dnd_release_payload::<DragCommand>()
                            && drag.0 != *command
                        {
                            movement = Some(MoveCommand {
                                command: drag.0.clone(),
                                group: index,
                                neighbor: Some((command.clone(), after)),
                            });
                        }
                    }
                }
            });
            let target = ui.interact(header.response.rect, id.with("drop"), egui::Sense::hover());
            if target.dnd_hover_payload::<DragCommand>().is_some() {
                paint_drop_edge(ui, target.rect, true);
            }
            if let Some(drag) = target.dnd_release_payload::<DragCommand>() {
                movement = Some(MoveCommand {
                    command: drag.0.clone(),
                    group: index,
                    neighbor: None,
                });
            }
            if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                let after = pointer.y >= target.rect.center().y;
                if target
                    .dnd_hover_payload::<DragGroup>()
                    .is_some_and(|drag| drag.0 != index)
                {
                    paint_drop_edge(ui, target.rect, after);
                }
                if let Some(drag) = target.dnd_release_payload::<DragGroup>()
                    && drag.0 != index
                {
                    category_drop = Some((drag.0, index, after));
                }
            }
        }
        if let Some(movement) = movement {
            move_command(&mut groups, movement);
            edited_groups = true;
        }
        if let Some(index) = remove {
            groups.remove(index);
            edited_groups = true;
        } else if let Some((source, target, after)) = category_drop {
            move_group(&mut groups, source, target, after);
            edited_groups = true;
        }
        if edited_groups {
            menu.groups = groups;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reordering_categories_changes_the_palette_order_without_moving_their_commands() {
        let mut menu = ActionMenu::default();
        menu.groups = resolved_groups(&menu, &catalogue());
        let selection = menu
            .groups
            .iter()
            .position(|group| group.commands.iter().any(|id| id == "rename"))
            .unwrap();
        let application = menu
            .groups
            .iter()
            .position(|group| group.commands.iter().any(|id| id == "settings"))
            .unwrap();
        let mut contextual = vec![json!({"id":"rename"}), json!({"id":"settings"})];
        move_group(&mut menu.groups, application, selection, false);
        customize(&mut contextual, &menu);
        assert_eq!(contextual[0]["id"], "settings");
        assert_eq!(contextual[1]["id"], "rename");

        move_group(&mut menu.groups, selection, selection + 1, true);
        customize(&mut contextual, &menu);
        assert_eq!(contextual[0]["id"], "rename");
        assert_eq!(contextual[1]["id"], "settings");
    }

    #[test]
    fn customized_palette_reorders_available_commands_and_hides_whole_groups() {
        let mut menu = ActionMenu::default();
        let catalogue = catalogue();
        let mut groups = resolved_groups(&menu, &catalogue);
        let group = groups.len();
        groups.push(ActionMenuGroup {
            id: "custom/1".into(),
            title: Some("My commands".into()),
            ..Default::default()
        });
        for id in ["rename", "copy"] {
            move_command(
                &mut groups,
                MoveCommand {
                    command: id.into(),
                    group,
                    neighbor: None,
                },
            );
        }
        menu.groups = groups;
        menu.command_visibility.insert("rename".into(), false);
        let contextual = vec![json!({"id":"copy","enabled":false}), json!({"id":"rename"})];
        let mut shown = contextual.clone();
        customize(&mut shown, &menu);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0]["id"], "copy");
        assert_eq!(shown[0]["group"], "My commands");
        assert_eq!(shown[0]["enabled"], false);

        menu.groups[group].hidden = true;
        let mut shown = contextual.clone();
        customize(&mut shown, &menu);
        assert!(shown.is_empty());

        menu.groups[group].hidden = false;
        menu.command_visibility.remove("rename");
        customize(&mut shown, &menu);
        assert!(shown.is_empty());
        let mut shown = contextual;
        customize(&mut shown, &menu);
        assert_eq!(shown[0]["id"], "rename");
        assert_eq!(shown[1]["id"], "copy");
    }
}

//! Compact status bar; detailed information stays in a bounded overlay.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Folder,
    Sort,
    Status,
    Recovery,
}
#[derive(Default)]
pub(super) struct State {
    page: Option<Page>,
}
impl State {
    pub fn is_open(&self) -> bool {
        self.page.is_some()
    }
}
fn friendly_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        path.to_owned()
    }
}
impl Shelf {
    pub(super) fn info_bar(&mut self, ui: &mut egui::Ui) {
        let folder = self.state["tab"] == "folder";
        let pods = self.state["tab"] == "pods";
        let path = self.state["folder_path"]
            .as_str()
            .map(friendly_path)
            .unwrap_or_else(|| "Выбрать папку".to_owned());
        let sort = sorting_ui::confirmation(&self.state["sort"]);
        let status = if self.message.is_empty() {
            "Готово".to_owned()
        } else {
            self.message.clone()
        };
        let width = ui.available_width();
        let available = (width
            - ui.spacing().item_spacing.x
                * if folder {
                    3.0
                } else if pods {
                    1.0
                } else {
                    2.0
                })
        .max(0.0);
        let widths = if folder {
            [
                available * 0.30,
                available * 0.22,
                available * 0.28,
                available * 0.20,
            ]
        } else if pods {
            [available * 0.32, 0.0, available * 0.68, 0.0]
        } else {
            [available * 0.25, available * 0.30, available * 0.45, 0.0]
        };
        ui.horizontal(|ui| {
            let context = if folder {
                format!("Folder · {path}")
            } else if self.state["pinned_only"] == true {
                "Закреплённые".into()
            } else {
                "Категория".into()
            };
            for (index, (label, page, tooltip)) in [
                (
                    context,
                    Page::Folder,
                    if folder {
                        format!("{path}\nShift Tab · Добавить в Катран")
                    } else {
                        "Shift Tab · Перенести в Folder".into()
                    },
                ),
                (sort.clone(), Page::Sort, sort.clone()),
                (format!("Статус · {status}"), Page::Status, status.clone()),
            ]
            .into_iter()
            .enumerate()
            {
                if pods && page == Page::Sort {
                    continue;
                }
                if ui
                    .add_sized(
                        [widths[index], 24.0],
                        egui::Button::new(RichText::new(label).size(11.0)).truncate(),
                    )
                    .on_hover_text(tooltip)
                    .clicked()
                {
                    self.info.page = Some(page);
                }
            }
            if folder
                && ui
                    .add_sized(
                        [widths[3], 24.0],
                        egui::Button::new(RichText::new("Восстановление").size(11.0)).truncate(),
                    )
                    .clicked()
            {
                self.info.page = Some(Page::Recovery);
                let result = self.call(Command::FolderRecovery);
                self.workspace.recovery = result["items"].as_array().cloned();
                self.workspace.recovery_root =
                    self.state["folder_path"].as_str().map(str::to_owned);
            }
        });
    }
    pub(super) fn info_panel(&mut self, ctx: &egui::Context) {
        let Some(page) = self.info.page else {
            return;
        };
        let width = (ctx.content_rect().width() - 32.0).clamp(200.0, 560.0);
        let modal=egui::Modal::new(egui::Id::new("information-bar"))
            .area(egui::Modal::default_area(egui::Id::new("information-bar")).anchor(egui::Align2::CENTER_BOTTOM,[0.0,-90.0]))
            .backdrop_color(Color32::from_black_alpha(24)).frame(crate::action_row::popup_frame(ctx)).show(ctx,|ui| {
                ui.set_width(width);
                ui.heading(match page {Page::Folder=>"Папка и категория",Page::Sort=>"Сортировка",Page::Status=>"Статус и операции",Page::Recovery=>"Восстановление файлов"});
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-200.0).clamp(80.0,300.0)).show(ui,|ui| match page {
                    Page::Folder=>{if self.state["tab"]=="folder" {ui.label(friendly_path(self.state["folder_path"].as_str().unwrap_or("Папка не выбрана")));ui.label("Ctrl Shift K · выбрать папку\nShift Tab · Добавить в Катран\nCtrl Shift C · добавить в Катран\nCtrl . · избранное");} else {ui.label(if self.state["pinned_only"]==true {"Закреплённые объекты"} else {"Поток категории"});ui.label("Shift Tab · Перенести в Folder");}},
                    Page::Sort=>{ui.label(sorting_ui::confirmation(&self.state["sort"]));},
                    Page::Status=>{ui.label(if self.message.is_empty(){"Нет сообщений"}else{&self.message});},
                    Page::Recovery=>{if let Some(items)=&self.workspace.recovery {if items.is_empty(){ui.label("Записей восстановления нет");} for item in items {let path=item["recovery_path"].as_str().or_else(||item["journal_path"].as_str()).unwrap_or("Путь недоступен");let error=item["error"]["message"].as_str().or_else(||item["error"].as_str()).unwrap_or("");ui.label(format!("{} {error}",friendly_path(path)));}}else{ui.label(if self.message.is_empty(){"Не удалось получить записи восстановления"}else{&self.message});}},
                });
            });
        if modal.should_close() {
            self.info.page = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalizes_windows_display_prefixes_only() {
        assert_eq!(super::friendly_path(r"\\?\C:\folder"), r"C:\folder");
        assert_eq!(
            super::friendly_path(r"\\?\UNC\server\share"),
            r"\\server\share"
        );
        assert_eq!(super::friendly_path(r"C:\folder"), r"C:\folder");
    }
}

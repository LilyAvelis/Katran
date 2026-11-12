//! Compact status bar; detailed information stays in a bounded overlay.
use super::i18n::tr;
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
    pub(super) fn sort_notice(&mut self, ctx: &egui::Context) {
        if self
            .favorite_notice
            .as_ref()
            .is_some_and(|(_, started)| started.elapsed() >= Duration::from_secs(2))
        {
            self.favorite_notice = None;
        }
        if self
            .sort_notice
            .as_ref()
            .is_some_and(|(_, started)| started.elapsed() >= Duration::from_secs(2))
        {
            self.sort_notice = None;
        }
        let favorite = self.favorite_notice.is_some();
        if !favorite
            && self.sort_notice.is_some()
            && self.call(Command::GetSettings)["sort_notifications"] == false
        {
            self.sort_notice = None;
        }
        let Some((text, _)) = self.favorite_notice.as_ref().or(self.sort_notice.as_ref()) else {
            return;
        };
        let text = text.clone();
        egui::Area::new(egui::Id::new("sort-notice"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -42.0])
            .show(ctx, |ui| {
                crate::action_row::popup_frame(ctx).show(ui, |ui| {
                    ui.label(text);
                });
            });
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    pub(super) fn info_bar(&mut self, ui: &mut egui::Ui) {
        let folder = self.state["tab"] == "folder";
        let path = self.state["folder_path"]
            .as_str()
            .map(friendly_path)
            .unwrap_or_else(|| tr("Выбрать папку", "Choose folder").to_owned());
        let sort = sorting_ui::confirmation(&self.state["sort"]);
        let status = if self.message.is_empty() {
            tr("Готово", "Ready").to_owned()
        } else {
            self.message.clone()
        };
        // Compact IDE-style segments; detail belongs to the existing API-backed panels.
        let context = if folder {
            format!("Folder · {path}")
        } else if self.state["pinned_only"] == true {
            tr("Закреплённые", "Pinned").into()
        } else {
            tr("Категория", "Category").into()
        };
        for (icon, page, tooltip) in [
            (egui_lucide::Lucide::FolderOpen, Page::Folder, context),
            (egui_lucide::Lucide::ArrowUpDown, Page::Sort, sort),
            (
                egui_lucide::Lucide::CircleQuestionMark,
                Page::Status,
                status,
            ),
            (
                egui_lucide::Lucide::RefreshCw,
                Page::Recovery,
                tr("Восстановление файлов", "File recovery").into(),
            ),
        ] {
            if page == Page::Sort || (!folder && page == Page::Recovery) {
                continue;
            }
            if crate::ui_style::status_icon(ui, icon, &tooltip).clicked() {
                self.info.page = Some(page);
                if page == Page::Recovery {
                    let result = self.call(Command::FolderRecovery);
                    self.workspace.recovery = result["items"].as_array().cloned();
                    self.workspace.recovery_root =
                        self.state["folder_path"].as_str().map(str::to_owned);
                }
            }
        }
    }

    pub(super) fn info_panel(&mut self, ctx: &egui::Context) {
        let Some(page) = self.info.page else {
            return;
        };
        let width = (ctx.content_rect().width() - 32.0).clamp(200.0, 560.0);
        let modal=egui::Modal::new(egui::Id::new("information-bar"))
            .area(egui::Modal::default_area(egui::Id::new("information-bar")).anchor(egui::Align2::CENTER_BOTTOM,[0.0,-38.0]))
            .backdrop_color(Color32::from_black_alpha(24)).frame(crate::action_row::popup_frame(ctx)).show(ctx,|ui| {
                ui.set_width(width);
                ui.heading(match page {Page::Folder=>tr("Папка и категория", "Folder and category"),Page::Sort=>tr("Сортировка", "Sorting"),Page::Status=>tr("Статус и операции", "Status and operations"),Page::Recovery=>tr("Восстановление файлов", "File recovery")});
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-200.0).clamp(80.0,300.0)).show(ui,|ui| match page {
                    Page::Folder=>{if self.state["tab"]=="folder" {ui.label(friendly_path(self.state["folder_path"].as_str().unwrap_or(tr("Папка не выбрана", "No folder selected"))));ui.label(tr("Ctrl Shift K · выбрать папку\nShift Tab · Добавить в Катран\nCtrl Shift C · добавить в Катран\nCtrl . · избранное", "Ctrl Shift K · choose folder\nShift Tab · add to Katran\nCtrl Shift C · add to Katran\nCtrl . · favorite"));} else {ui.label(if self.state["pinned_only"]==true {tr("Закреплённые объекты", "Pinned items")} else {tr("Поток категории", "Category stream")});ui.label(tr("Shift Tab · Перенести в Folder", "Shift Tab · Move to Folder"));}},
                    Page::Sort=>{ui.label(sorting_ui::confirmation(&self.state["sort"]));},
                    Page::Status=>{ui.label(if self.message.is_empty(){tr("Нет сообщений", "No messages")}else{&self.message});},
                    Page::Recovery=>{if let Some(items)=&self.workspace.recovery {if items.is_empty(){ui.label(tr("Записей восстановления нет", "No recovery records"));} for item in items {let path=item["recovery_path"].as_str().or_else(||item["journal_path"].as_str()).unwrap_or(tr("Путь недоступен", "Path unavailable"));let error=item["error"]["message"].as_str().or_else(||item["error"].as_str()).unwrap_or("");ui.label(format!("{} {error}",friendly_path(path)));}}else{ui.label(if self.message.is_empty(){tr("Не удалось получить записи восстановления", "Could not retrieve recovery records")}else{&self.message});}},
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

//! Afio's settings shell and shared section/row components; API owns preferences.
use super::ui_foundation::{SettingsRow, SettingsSection};
use eframe::egui::{self, Color32, RichText};
use katran::{Command, Request, Service, interaction::View, settings::Settings};

#[derive(Default)]
pub struct State {
    updates: super::updates_ui::State,
    query: String,
    section: usize,
    requested_section: Option<usize>,
    draft: Option<Settings>,
    error: String,
    editor: Option<super::shortcuts_ui::Editor>,
    shortcut_filter: String,
    sharex_folder: String,
    downloads_folder: String,
}
fn call(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "settings-ui".into(),
        command,
    })
}
fn merge_tab_settings(draft: &mut Settings, current: &Settings) {
    draft.tab_order = current.tab_order.clone();
    draft.hidden_tabs = current.hidden_tabs.clone();
    draft.downloads_enabled = current.downloads_enabled;
    draft.text_enabled = current.text_enabled;
    draft.links_enabled = current.links_enabled;
    draft.tables_enabled = current.tables_enabled;
    draft.sort = current.sort;
}
impl State {
    pub fn show(&mut self, ctx: &egui::Context, api: &mut Service) -> bool {
        self.updates.tick(api, ctx);
        let interaction = call(api, Command::Interaction);
        let open = interaction["result"]["settings_open"] == true;
        if !open {
            self.draft = None;
            self.sharex_folder.clear();
            self.downloads_folder.clear();
            self.editor = None;
            self.requested_section = None;
            return false;
        }
        let requested = interaction["result"]["settings_section"]
            .as_u64()
            .unwrap_or(0) as usize;
        if self.requested_section != Some(requested) {
            self.section = requested;
            self.requested_section = Some(requested);
        }
        if self.draft.is_none() {
            self.draft =
                serde_json::from_value(call(api, Command::GetSettings)["result"].clone()).ok();
        }
        if let Some(draft) = self.draft.as_ref()
            && self.sharex_folder.is_empty()
        {
            self.sharex_folder = draft
                .sharex_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        if let Some(draft) = self.draft.as_ref()
            && self.downloads_folder.is_empty()
        {
            self.downloads_folder = draft
                .downloads_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        let logger = api.logger();
        let mut close = false;
        let mut tabs_changed = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("katran-settings"),egui::ViewportBuilder::default().with_title("Katran Settings").with_inner_size([980.0,680.0]).with_min_inner_size([680.0,460.0]),|ctx,_| {
   let updates_ctx=ctx.clone();
   super::input_log::record(ctx,logger.as_ref());
   let recording=self.editor.is_some();
   if let Some(editor)=self.editor.as_mut() && let Some(intent)=editor.input(ctx)&& super::shortcuts_ui::apply(api,editor,intent){self.editor=None;}
   close=ctx.input(|i|i.viewport().close_requested())||(!recording&&ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)));
   egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_gray(69))).show(ctx,|ui| {
    egui::Panel::left("settings-sidebar").exact_size(215.0).resizable(false).frame(egui::Frame::new().fill(Color32::from_gray(65)).inner_margin(16)).show(ui,|ui|{
     ui.add_space(8.0);ui.label(RichText::new("Settings").size(19.0).strong());ui.add_space(24.0);
     ui.add_sized([ui.available_width(),34.0],egui::TextEdit::singleline(&mut self.query).hint_text("Поиск настроек…").margin(egui::vec2(10.0,9.0)));
     ui.add_space(24.0);ui.label(RichText::new("WORKSPACE").size(10.0).color(Color32::from_gray(173)));ui.add_space(8.0);
     for(index,label)in ["Основные","Горячие клавиши","Вкладки","Обновления"].iter().enumerate(){if label.to_lowercase().contains(&self.query.to_lowercase())&&ui.add_sized([ui.available_width(),38.0],egui::Button::new(*label).selected(self.section==index)).clicked(){self.section=index;self.editor=None;}}
     ui.with_layout(egui::Layout::bottom_up(egui::Align::Min),|ui|{ui.weak(crate::BUILD_LABEL);});
    });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_gray(69)).inner_margin(24)).show(ui,|ui|{
     egui::ScrollArea::vertical().show(ui,|ui|{
      if self.section==0 {
       ui.heading("Основные");ui.add_space(16.0);
       if let Some(draft)=self.draft.as_mut(){
        SettingsSection::show(ui,|ui|{
         SettingsRow::show(ui,"Захват при запуске","По умолчанию выключен. Текущий захват переключается через Actions.",|ui|{ui.checkbox(&mut draft.capture_on_start,"Включать");});
         SettingsRow::show(ui,"Вид при запуске","Текущий вид меняется отдельно через Actions.",|ui|{ui.selectable_value(&mut draft.default_view,View::Gallery,"Галерея");ui.selectable_value(&mut draft.default_view,View::List,"Поток");});
         SettingsRow::show(ui,"Столбцы галереи","Ctrl− / Ctrl+: количество карточек, без масштабирования интерфейса.",|ui|{ui.add(egui::Slider::new(&mut draft.gallery_columns,1..=12));});
         SettingsRow::show(ui,"Строки потока","Предпочтительное количество видимых карточек; в маленьком окне их будет меньше.",|ui|{ui.add(egui::Slider::new(&mut draft.stream_rows,1..=12));});
         SettingsRow::show(ui,"Папка ShareX","Абсолютный путь к папке снимков. Пустое поле отключает импорт; изменения поступают из Windows.",|ui|{
          if ui.add(egui::TextEdit::singleline(&mut self.sharex_folder).hint_text("C:\\Screenshots")).changed(){draft.sharex_folder=if self.sharex_folder.trim().is_empty(){None}else{Some(std::path::PathBuf::from(self.sharex_folder.trim()))};}
         });
         SettingsRow::show(ui,"Папка Downloads","Пустое поле использует системную папку загрузок Windows. Можно указать другую абсолютную папку.",|ui|{
          if ui.add(egui::TextEdit::singleline(&mut self.downloads_folder).hint_text("Системная папка Downloads")).changed(){draft.downloads_folder=if self.downloads_folder.trim().is_empty(){None}else{Some(std::path::PathBuf::from(self.downloads_folder.trim()))};}
         });
         let mut mib=draft.quota_bytes as f64/(1024.0*1024.0);
         SettingsRow::show(ui,"Квота кэша · МиБ","Снижение ниже текущего объёма отклоняется. Исходники не удаляются.",|ui|{if ui.add(egui::DragValue::new(&mut mib).range(1.0..=1048576.0)).changed(){draft.quota_bytes=(mib*1024.0*1024.0)as u64;}});
        });
       }
       ui.add_space(16.0);
       if ui.button("Сохранить настройки").clicked()&&let Some(mut settings)=self.draft.clone(){
        let current=call(api,Command::GetSettings);
        if current["ok"]==true {
         if let Ok(saved)=serde_json::from_value::<Settings>(current["result"].clone()) {
          merge_tab_settings(&mut settings,&saved);
          let r=call(api,Command::SetSettings{settings});self.error=if r["ok"]==true{"Настройки сохранены".into()}else{r["error"]["message"].as_str().unwrap_or("Ошибка сохранения").into()};
         } else {self.error="Не удалось прочитать текущие настройки".into();}
        } else {self.error=current["error"]["message"].as_str().unwrap_or("Не удалось прочитать текущие настройки").into();}
       }
      } else if self.section==2 {
       ui.heading("Вкладки");ui.add_space(8.0);
       ui.label("Выключенная вкладка скрывается и останавливает сбор связанных данных. Сохранённые данные остаются на месте.");ui.add_space(16.0);
       let result=call(api,Command::Tabs);
       if let Some(tabs)=result["result"]["tabs"].as_array() {
        SettingsSection::show(ui,|ui|{
         for entry in tabs {
          let label=entry["label"].as_str().unwrap_or("");
          let enabled=entry["enabled"]==true;
          let can_toggle=entry["can_toggle"]==true;
          if let Ok(tab)=serde_json::from_value(entry["tab"].clone()) {
           SettingsRow::show(ui,label,if can_toggle {if enabled {"Включена"} else {"Выключена"}} else {"Всегда включена"},|ui|{
            if ui.add_enabled(can_toggle,egui::Button::image(if enabled {egui_lucide::Lucide::Eye.size(18.0).image()} else {egui_lucide::Lucide::EyeOff.size(18.0).image()})).on_hover_text(if enabled {"Выключить вкладку"} else {"Включить вкладку"}).clicked() {
             let response=call(api,Command::SetTabEnabled{tab,enabled:!enabled});
             if response["ok"]==true {
              tabs_changed=true;
              if let Ok(saved)=serde_json::from_value::<Settings>(call(api,Command::GetSettings)["result"].clone()) && let Some(draft)=self.draft.as_mut() {merge_tab_settings(draft,&saved);}
              self.error.clear();
             } else {self.error=response["error"]["message"].as_str().unwrap_or("Ошибка").into();}
            }
           });
          }
         }
        });
       }
      } else if self.section==3 {
       self.updates.show(ui,api,&updates_ctx);
      } else {
       ui.heading("Горячие клавиши");ui.add_space(16.0);
       if ui.add(egui::TextEdit::singleline(&mut self.shortcut_filter).hint_text("Поиск команды…").desired_width(ui.available_width())).changed(){self.editor=None;}
       let shortcuts:Vec<katran::shortcuts::ShortcutInfo>=serde_json::from_value(call(api,Command::Shortcuts)["result"].clone()).unwrap_or_default();
       let mut edit=None;
       SettingsSection::show(ui,|ui|{
        for shortcut in shortcuts.iter().filter(|s|s.label.to_lowercase().contains(&self.shortcut_filter.to_lowercase())) {
         ui.push_id(&shortcut.id,|ui|{
          SettingsRow::show(ui,&shortcut.label,shortcut.binding.as_deref().unwrap_or("Не назначено"),|ui|{
           ui.add(super::action_icons::icon(&shortcut.id).size(16.0).image());
           if shortcut.rebindable {
            if ui.button("Изменить").clicked(){edit=Some(shortcut.clone());}
            if ui.button("Сбросить").clicked(){let r=call(api,Command::SetShortcut{id:shortcut.id.clone(),binding:None,replace:false,reset:true});if r["ok"]==false{self.error=r["error"]["message"].as_str().unwrap_or("Ошибка").into();}}
           }else{ui.weak("Системное сочетание");}
          });
          if self.editor.as_ref().is_some_and(|e|e.id==shortcut.id) && let Some(editor)=self.editor.as_mut() {
           ui.separator();if let Some(intent)=editor.render(ui) && super::shortcuts_ui::apply(api,editor,intent){self.editor=None;}ui.separator();
          }
         });
        }
       });
       if let Some(s)=edit {self.editor=Some(super::shortcuts_ui::Editor::new(s.id,s.label,s.binding.unwrap_or_default()));}
      }
      if !self.error.is_empty(){super::ui_foundation::message(ui,&self.error,false);}
     });
    });
   });
  });
        if close {
            call(api, Command::CloseSettings);
            self.draft = None;
            self.sharex_folder.clear();
            self.downloads_folder.clear();
            self.editor = None;
            self.requested_section = None;
        }
        tabs_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katran::interaction::ShelfTab;

    #[test]
    fn general_draft_preserves_newer_tab_changes() {
        let mut draft = Settings {
            gallery_columns: 7,
            capture_on_start: true,
            ..Settings::default()
        };
        let current = Settings {
            tab_order: vec![ShelfTab::Downloads, ShelfTab::Images, ShelfTab::Screenshots],
            hidden_tabs: vec![ShelfTab::Images, ShelfTab::Downloads],
            downloads_enabled: false,
            text_enabled: true,
            links_enabled: true,
            tables_enabled: true,
            sort: katran::sorting::SortSpec {
                mode: katran::sorting::SortMode::Origin,
                reverse: true,
                seed: 42,
            },
            ..Settings::default()
        };
        merge_tab_settings(&mut draft, &current);
        assert_eq!(draft.tab_order, current.tab_order);
        assert_eq!(draft.hidden_tabs, current.hidden_tabs);
        assert!(!draft.downloads_enabled);
        assert!(draft.text_enabled);
        assert!(draft.links_enabled);
        assert!(draft.tables_enabled);
        assert_eq!(draft.sort, current.sort);
        assert_eq!(draft.gallery_columns, 7);
        assert!(draft.capture_on_start);
    }
}

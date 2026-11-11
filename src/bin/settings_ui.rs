//! Afio's settings shell and shared section/row components; API owns preferences.
use super::ui_foundation::{SettingsRow, SettingsSection};
use eframe::egui::{self, Color32, RichText};
use katran::{Command, Request, Service, interaction::View, settings::Settings};

#[derive(Default)]
pub struct State {
    updates: super::updates_ui::State,
    confirm_history: bool,
    query: String,
    section: usize,
    requested_section: Option<usize>,
    draft: Option<Settings>,
    error: String,
    editor: Option<super::shortcuts_ui::Editor>,
    global_editor: Option<super::shortcuts_ui::Editor>,
    overlay_status: String,
    overlay_retry: bool,
    shortcut_filter: String,
    sharex_folder: String,
    downloads_folder: String,
}
fn apply_global_hotkey(
    api: &mut Service,
    editor: &mut super::shortcuts_ui::Editor,
    intent: super::shortcuts_ui::Intent,
    draft: &mut Option<Settings>,
) -> bool {
    use super::shortcuts_ui::Intent;
    let Intent::Set { binding, .. } = intent else {
        return true;
    };
    if !binding.is_empty()
        && let Err(error) = katran::overlay::Hotkey::parse(&binding)
    {
        editor.failed(binding, error.message, false);
        return false;
    }
    let current = call(api, Command::GetSettings);
    let result = if current["ok"] == true {
        match serde_json::from_value::<Settings>(current["result"].clone()) {
            Ok(mut current) => {
                current.overlay_hotkey = binding.clone();
                call(api, Command::SetSettings { settings: current })
            }
            Err(_) => {
                serde_json::json!({"ok":false,"error":{"message":"Не удалось прочитать текущие настройки"}})
            }
        }
    } else {
        current
    };
    if result["ok"] == true {
        if let Some(draft) = draft.as_mut() {
            draft.overlay_hotkey = binding;
        }
        true
    } else {
        editor.failed(
            binding,
            result["error"]["message"]
                .as_str()
                .unwrap_or("Ошибка сохранения")
                .into(),
            false,
        );
        false
    }
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
    draft.pods_enabled = current.pods_enabled;
    draft.theme = current.theme.clone();
    draft.sort = current.sort;
}
fn apply_theme(
    api: &mut Service,
    draft: &mut Option<Settings>,
    theme: katran::theme::ThemePreferences,
) -> Result<(), String> {
    let response = call(api, Command::SetTheme { theme });
    if response["ok"] != true {
        return Err(response["error"]["message"]
            .as_str()
            .unwrap_or("Не удалось применить тему")
            .into());
    }
    let current = serde_json::from_value(response["result"]["current"].clone())
        .map_err(|_| "Не удалось прочитать применённую тему".to_string())?;
    if let Some(draft) = draft.as_mut() {
        draft.theme = current;
    }
    Ok(())
}
impl State {
    pub fn set_overlay_status(&mut self, status: String) {
        self.overlay_status = status;
    }
    pub fn take_overlay_retry(&mut self) -> bool {
        std::mem::take(&mut self.overlay_retry)
    }
    pub fn is_recording_overlay(&self) -> bool {
        self.global_editor.is_some()
    }
    pub fn show(&mut self, ctx: &egui::Context, api: &mut Service) -> bool {
        self.updates.tick(api, ctx);
        let interaction = call(api, Command::Interaction);
        let open = interaction["result"]["settings_open"] == true;
        if !open {
            if self.global_editor.is_some() {
                ctx.request_repaint();
            }
            self.confirm_history = false;
            self.draft = None;
            self.sharex_folder.clear();
            self.downloads_folder.clear();
            self.editor = None;
            self.global_editor = None;
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
        let mut global_intent = None;
        let mut theme_intent = None;
        let root_ctx = ctx.clone();
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("katran-settings"),egui::ViewportBuilder::default().with_title("Katran Settings").with_inner_size([980.0,680.0]).with_min_inner_size([680.0,460.0]),|ctx,_| {
   let updates_ctx=ctx.clone();
   super::input_log::record(ctx,logger.as_ref());
   let recording=self.editor.is_some()||self.global_editor.is_some();
   if let Some(editor)=self.global_editor.as_mut() && let Some(intent)=editor.input(ctx) && apply_global_hotkey(api,editor,intent,&mut self.draft){self.global_editor=None;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
   if let Some(editor)=self.editor.as_mut() && let Some(intent)=editor.input(ctx)&& super::shortcuts_ui::apply(api,editor,intent){self.editor=None;}
   let escape=!recording&&ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape));
   if escape && self.confirm_history { self.confirm_history=false; } else { close=ctx.input(|i|i.viewport().close_requested())||escape; }
   let surface=super::theme_ui::color(ctx,super::theme_ui::Role::Surface,Color32::from_gray(69));
   let background=super::theme_ui::color(ctx,super::theme_ui::Role::Background,Color32::from_gray(65));
   egui::CentralPanel::default().frame(egui::Frame::new().fill(surface)).show(ctx,|ui| {
    egui::Panel::left("settings-sidebar").exact_size(215.0).resizable(false).frame(egui::Frame::new().fill(background).inner_margin(16)).show(ui,|ui|{
     super::theme_ui::decorate(ui,ui.max_rect());
     ui.add_space(8.0);ui.label(RichText::new("Settings").size(19.0).strong());ui.add_space(24.0);
     ui.add_sized([ui.available_width(),34.0],egui::TextEdit::singleline(&mut self.query).hint_text("Поиск настроек…").margin(egui::vec2(10.0,9.0)));
     ui.add_space(24.0);ui.label(RichText::new("WORKSPACE").size(10.0).color(Color32::from_gray(173)));ui.add_space(8.0);
     for(index,label)in ["Основные","Горячие клавиши","Вкладки","Обновления","Темы"].iter().enumerate(){if label.to_lowercase().contains(&self.query.to_lowercase())&&ui.add_sized([ui.available_width(),38.0],egui::Button::new(*label).selected(self.section==index)).clicked(){self.section=index;self.editor=None;self.global_editor=None;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}}
     ui.with_layout(egui::Layout::bottom_up(egui::Align::Min),|ui|{ui.weak(crate::BUILD_LABEL);});
    });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(surface).inner_margin(24)).show(ui,|ui|{
     super::theme_ui::decorate(ui,ui.max_rect());
     egui::ScrollArea::vertical().show(ui,|ui|{
      if self.section==0 {
       ui.heading("Основные");ui.add_space(16.0);
       if let Some(draft)=self.draft.as_mut(){
        SettingsSection::show(ui,|ui|{
         let binding=if draft.overlay_hotkey.is_empty(){"Не назначена"}else{draft.overlay_hotkey.as_str()};
         SettingsRow::show(ui,"Вызов Katran поверх окна",binding,|ui|{
          if ui.button("Изменить").clicked(){self.global_editor=Some(super::shortcuts_ui::Editor::new("overlay_hotkey","Вызов Katran",draft.overlay_hotkey.clone()));root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
          if ui.button("Очистить").clicked(){self.global_editor=Some(super::shortcuts_ui::Editor::new("overlay_hotkey","Вызов Katran",draft.overlay_hotkey.clone()));global_intent=Some(super::shortcuts_ui::Intent::Set{binding:String::new(),replace:false});}
          if ui.button("Повторить").clicked(){self.overlay_retry=true;root_ctx.request_repaint_of(egui::ViewportId::ROOT);}
         });
         if !self.overlay_status.is_empty(){ui.weak(&self.overlay_status);}
         if let Some(editor)=self.global_editor.as_mut(){ui.separator();if let Some(intent)=editor.render(ui){global_intent=Some(intent);}ui.separator();}
         SettingsRow::show(ui,"Захват при запуске","По умолчанию включён. Текущий захват переключается через Actions.",|ui|{ui.checkbox(&mut draft.capture_on_start,"Включать");});
         SettingsRow::show(ui,"Подсказка сортировки","Показывать выбранный порядок на две секунды.",|ui|{ui.checkbox(&mut draft.sort_notifications,"Показывать");});
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
       if ui.button("Очистить историю Folder").clicked() {
        let r=call(api,Command::ClearFolderHistory{confirmed:false});
        if r["result"]["confirmation_required"]==true {self.confirm_history=true;} else if r["ok"]==false {self.error=r["error"]["message"].as_str().unwrap_or("Ошибка").into();}
       }
       if self.confirm_history {
        ui.label(RichText::new("Действительно удалить историю?").strong());
        ui.label("Избранные папки и файлы сохранятся.");
        ui.horizontal(|ui|{
         if ui.button("Удалить историю").clicked(){let r=call(api,Command::ClearFolderHistory{confirmed:true});self.confirm_history=false;self.error=if r["result"]["cleared"]==true {"История очищена".into()} else {r["error"]["message"].as_str().unwrap_or("Ошибка").into()};}
         if ui.button("Отмена").clicked(){self.confirm_history=false;}
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
      } else if self.section==4 {
       ui.heading("Темы");ui.add_space(8.0);
       ui.label("Тема применяется сразу. Стандартный вид остаётся доступен.");ui.add_space(16.0);
       let response=call(api,Command::Themes);
       if response["ok"]==true {
        if let Ok(mut theme)=serde_json::from_value::<katran::theme::ThemePreferences>(response["result"]["current"].clone()) {
         SettingsSection::show(ui,|ui|{
          SettingsRow::show(ui,"Стандартная","Привычный внешний вид Katran.",|ui|{
           if ui.selectable_value(&mut theme.preset,katran::theme::Preset::Standard,"Выбрать").changed(){theme_intent=Some(theme.clone());}
          });
          SettingsRow::show(ui,"Синяя","Экспериментальная тема с голубыми акцентами.",|ui|{
           if ui.selectable_value(&mut theme.preset,katran::theme::Preset::BlueExperimental,"Выбрать").changed(){theme_intent=Some(theme.clone());}
          });
          if theme.preset==katran::theme::Preset::BlueExperimental {
           SettingsRow::show(ui,"Цвет акцента","Выберите свой цвет или верните исходный.",|ui|{
            let mut accent=theme.accent.unwrap_or([75,139,210]);
            if ui.color_edit_button_srgb(&mut accent).changed(){theme.accent=Some(accent);theme_intent=Some(theme.clone());}
            if ui.button("Сбросить цвет").clicked(){theme.accent=None;theme_intent=Some(theme.clone());}
           });
           SettingsRow::show(ui,"Компактное окно","Убрать название и крестик. Перемещение — за узкую полосу над поиском.",|ui|{
            if ui.checkbox(&mut theme.compact_header,"Без шапки").changed(){theme_intent=Some(theme.clone());}
           });
           SettingsRow::show(ui,"Стекло · эксперимент","Windows 11 Acrylic. После включения полностью выйдите через трей и запустите Katran снова. При недоступности — сплошной фон.",|ui|{
            if ui.checkbox(&mut theme.glass,"Включить").changed(){if theme.glass{theme.lily_background=None;}theme_intent=Some(theme.clone());}
           });
           SettingsRow::show(ui,"Фон","Лилии от дизайнера. Изображение загружается один раз.",|ui|{
            if ui.selectable_value(&mut theme.lily_background,None,"Сплошной").changed(){theme_intent=Some(theme.clone());}
            for i in 1..=5 {if ui.selectable_value(&mut theme.lily_background,Some(i),format!("{i}")).changed(){theme.glass=false;theme_intent=Some(theme.clone());}}
           });
           SettingsRow::show(ui,"Декоративные линии","Мягкий рисунок на фоне синей темы.",|ui|{
            if ui.checkbox(&mut theme.decoration,"Показывать").changed(){theme_intent=Some(theme.clone());}
           });
          }
         });
        } else {ui.label("Не удалось прочитать текущую тему.");}
       } else {ui.label(response["error"]["message"].as_str().unwrap_or("Не удалось загрузить темы"));}
      } else {
       ui.heading("Горячие клавиши");ui.add_space(16.0);
       if ui.add(egui::TextEdit::singleline(&mut self.shortcut_filter).hint_text("Поиск команды…").desired_width(ui.available_width())).changed(){self.editor=None;}
       let shortcuts:Vec<katran::shortcuts::ShortcutInfo>=serde_json::from_value(call(api,Command::Shortcuts)["result"].clone()).unwrap_or_default();
       let mut edit=None;
       SettingsSection::show(ui,|ui|{
        for shortcut in shortcuts.iter().filter(|s|!s.id.starts_with("sort_") && s.label.to_lowercase().contains(&self.shortcut_filter.to_lowercase())) {
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
        if let Some(theme) = theme_intent {
            self.error = match apply_theme(api, &mut self.draft, theme) {
                Ok(()) => String::new(),
                Err(error) => error,
            };
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if let Some(intent) = global_intent
            && let Some(editor) = self.global_editor.as_mut()
            && apply_global_hotkey(api, editor, intent, &mut self.draft)
        {
            self.global_editor = None;
            ctx.request_repaint();
        }
        if close {
            call(api, Command::CloseSettings);
            self.draft = None;
            self.sharex_folder.clear();
            self.downloads_folder.clear();
            self.editor = None;
            self.global_editor = None;
            ctx.request_repaint();
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

    #[test]
    fn theme_applies_immediately_without_saving_unrelated_draft_fields() {
        use katran::theme::{Preset, ThemePreferences};
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 1_000_000).unwrap();
        let saved: Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
        let mut draft = Some(Settings {
            gallery_columns: 7,
            ..saved.clone()
        });
        let blue = ThemePreferences {
            preset: Preset::BlueExperimental,
            accent: Some([40, 100, 220]),
            decoration: false, lily_background:None,glass:false,compact_header:false,
        };
        apply_theme(&mut api, &mut draft, blue.clone()).unwrap();
        let current: Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
        assert_eq!(current.theme, blue);
        assert_eq!(current.gallery_columns, saved.gallery_columns);
        assert_eq!(draft.as_ref().unwrap().gallery_columns, 7);
        assert_eq!(draft.as_ref().unwrap().theme, blue);
        let mut stale = Settings {
            gallery_columns: 9,
            ..saved
        };
        merge_tab_settings(&mut stale, &current);
        assert_eq!(stale.theme, blue);
        assert_eq!(stale.gallery_columns, 9);
        call(&mut api, Command::SetSettings { settings: stale });
        assert_eq!(
            call(&mut api, Command::Themes)["result"]["current"]["preset"],
            "blue_experimental"
        );
        apply_theme(&mut api, &mut draft, ThemePreferences::default()).unwrap();
        assert_eq!(
            call(&mut api, Command::Themes)["result"]["current"]["preset"],
            "standard"
        );
    }

    #[test]
    fn global_recorder_saves_only_binding_and_keeps_draft_changes() {
        use super::super::shortcuts_ui::{Editor, Intent};
        let root = tempfile::tempdir().unwrap();
        let mut api = Service::open(root.path(), 1_000_000).unwrap();
        let saved: Settings =
            serde_json::from_value(call(&mut api, Command::GetSettings)["result"].clone()).unwrap();
        let mut draft = Some(Settings {
            gallery_columns: 7,
            ..saved.clone()
        });
        let mut editor = Editor::new("overlay_hotkey", "Вызов Katran", "");
        assert!(!apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Set {
                binding: "A".into(),
                replace: false
            },
            &mut draft
        ));
        assert_eq!(
            call(&mut api, Command::GetSettings)["result"]["overlay_hotkey"],
            ""
        );
        assert!(apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Set {
                binding: "Alt R".into(),
                replace: false
            },
            &mut draft
        ));
        let stored = call(&mut api, Command::GetSettings);
        assert_eq!(stored["result"]["overlay_hotkey"], "Alt R");
        assert_eq!(stored["result"]["gallery_columns"], saved.gallery_columns);
        assert_eq!(draft.as_ref().unwrap().gallery_columns, 7);
        assert_eq!(draft.as_ref().unwrap().overlay_hotkey, "Alt R");
        assert!(apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Cancel,
            &mut draft
        ));
        assert_eq!(
            call(&mut api, Command::GetSettings)["result"]["overlay_hotkey"],
            "Alt R"
        );
        assert!(apply_global_hotkey(
            &mut api,
            &mut editor,
            Intent::Set {
                binding: String::new(),
                replace: false
            },
            &mut draft
        ));
        assert_eq!(
            call(&mut api, Command::GetSettings)["result"]["overlay_hotkey"],
            ""
        );
        assert_eq!(draft.as_ref().unwrap().gallery_columns, 7);
        assert!(draft.unwrap().overlay_hotkey.is_empty());
    }

    #[test]
    fn global_editor_consumes_chord_and_escape_and_retry_is_one_shot() {
        use super::super::shortcuts_ui::{Editor, Intent};
        let ctx = egui::Context::default();
        let mut editor = Editor::new("overlay_hotkey", "Вызов Katran", "Alt R");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(editor.input(ui.ctx()).is_none());
        });
        output.textures_delta.clear();
        let input = egui::RawInput {
            events: vec![
                egui::Event::Key {
                    key: egui::Key::R,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                },
                egui::Event::Text("r".into()),
            ],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(matches!(editor.input(ui.ctx()), Some(Intent::Set { binding, .. }) if binding == "Alt R"));
            assert!(ui.ctx().input(|i| i.events.is_empty()));
        });
        output.textures_delta.clear();
        let ctx = egui::Context::default();
        let mut editor = Editor::new("overlay_hotkey", "Вызов Katran", "Alt R");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            editor.input(ui.ctx());
        });
        output.textures_delta.clear();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(matches!(editor.input(ui.ctx()), Some(Intent::Cancel)));
            assert!(ui.ctx().input(|i| i.events.is_empty()));
        });
        output.textures_delta.clear();
        let mut state = State {
            overlay_retry: true,
            ..Default::default()
        };
        assert!(state.take_overlay_retry());
        assert!(!state.take_overlay_retry());
    }
}

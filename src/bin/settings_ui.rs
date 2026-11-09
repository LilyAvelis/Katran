//! Afio's settings shell and shared section/row components; API owns preferences.
use super::ui_foundation::{SettingsRow, SettingsSection};
use eframe::egui::{self, Color32, RichText};
use katran::{Command, Request, Service, interaction::View, settings::Settings};

#[derive(Default)]
pub struct State {
    query: String,
    section: usize,
    draft: Option<Settings>,
    error: String,
    editor: Option<super::shortcuts_ui::Editor>,
    shortcut_filter: String,
}
fn call(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "settings-ui".into(),
        command,
    })
}
impl State {
    pub fn show(&mut self, ctx: &egui::Context, api: &mut Service) {
        let open = call(api, Command::Interaction)["result"]["settings_open"] == true;
        if !open {
            self.draft = None;
            self.editor = None;
            return;
        }
        if self.draft.is_none() {
            self.draft =
                serde_json::from_value(call(api, Command::GetSettings)["result"].clone()).ok();
        }
        let logger = api.logger();
        let mut close = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("katran-settings"),egui::ViewportBuilder::default().with_title("Katran Settings").with_inner_size([980.0,680.0]).with_min_inner_size([680.0,460.0]),|ctx,_| {
   super::input_log::record(ctx,logger.as_ref());
   let recording=self.editor.is_some();
   if let Some(editor)=self.editor.as_mut() && let Some(intent)=editor.input(ctx)&& super::shortcuts_ui::apply(api,editor,intent){self.editor=None;}
   close=ctx.input(|i|i.viewport().close_requested())||(!recording&&ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)));
   egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_gray(69))).show(ctx,|ui| {
    egui::Panel::left("settings-sidebar").exact_size(215.0).resizable(false).frame(egui::Frame::new().fill(Color32::from_gray(65)).inner_margin(16)).show(ui,|ui|{
     ui.add_space(8.0);ui.label(RichText::new("Settings").size(19.0).strong());ui.add_space(24.0);
     ui.add_sized([ui.available_width(),34.0],egui::TextEdit::singleline(&mut self.query).hint_text("Поиск настроек…").margin(egui::vec2(10.0,9.0)));
     ui.add_space(24.0);ui.label(RichText::new("WORKSPACE").size(10.0).color(Color32::from_gray(173)));ui.add_space(8.0);
     for(index,label)in ["Основные","Горячие клавиши"].iter().enumerate(){if label.to_lowercase().contains(&self.query.to_lowercase())&&ui.add_sized([ui.available_width(),38.0],egui::Button::new(*label).selected(self.section==index)).clicked(){self.section=index;self.editor=None;}}
     ui.with_layout(egui::Layout::bottom_up(egui::Align::Min),|ui|{ui.weak("Katran · 1.0.2 · Release");});
    });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_gray(69)).inner_margin(24)).show(ui,|ui|{
     egui::ScrollArea::vertical().show(ui,|ui|{
      if self.section==0 {
       ui.heading("Основные");ui.add_space(16.0);
       if let Some(draft)=self.draft.as_mut(){
        SettingsSection::show(ui,|ui|{
         SettingsRow::show(ui,"Захват при запуске","По умолчанию выключен. Текущий захват переключается через Actions.",|ui|{ui.checkbox(&mut draft.capture_on_start,"Включать");});
         SettingsRow::show(ui,"Вид при запуске","Текущий вид меняется отдельно через Actions.",|ui|{ui.selectable_value(&mut draft.default_view,View::Gallery,"Галерея");ui.selectable_value(&mut draft.default_view,View::List,"Поток");});
         SettingsRow::show(ui,"Размер карточек","Ширина карточки галереи в логических пикселях.",|ui|{ui.add(egui::Slider::new(&mut draft.card_width,160.0..=360.0));});
         let mut mib=draft.quota_bytes as f64/(1024.0*1024.0);
         SettingsRow::show(ui,"Квота кэша · МиБ","Снижение ниже текущего объёма отклоняется. Исходники не удаляются.",|ui|{if ui.add(egui::DragValue::new(&mut mib).range(1.0..=1048576.0)).changed(){draft.quota_bytes=(mib*1024.0*1024.0)as u64;}});
        });
       }
       ui.add_space(16.0);
       if ui.button("Сохранить настройки").clicked()&&let Some(settings)=self.draft.clone(){let r=call(api,Command::SetSettings{settings});self.error=if r["ok"]==true{"Настройки сохранены".into()}else{r["error"]["message"].as_str().unwrap_or("Ошибка сохранения").into()};}
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
            self.editor = None;
        }
    }
}

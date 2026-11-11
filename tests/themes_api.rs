use katran::{Service,Command,Request,theme::{Preset,ThemePreferences}};
use serde_json::Value;
fn call(api:&mut Service,c:Command)->Value{let r=api.execute(Request{api_version:1,request_id:"themes".into(),command:c});assert_eq!(r["ok"],true,"{r}");r["result"].clone()}
#[test]fn theme_is_optional_persistent_and_does_not_change_behavior_preferences(){
 let root=tempfile::tempdir().unwrap();let mut api=Service::open(root.path(),10000).unwrap();let original=call(&mut api,Command::GetSettings);let interaction=call(&mut api,Command::Interaction);
 let catalog=call(&mut api,Command::Themes);assert_eq!(catalog["current"]["preset"],"standard");assert_eq!(catalog["themes"][1]["experimental"],true);
 let pref=ThemePreferences{preset:Preset::BlueExperimental,accent:Some([98,146,240]),decoration:true, lily_background:None,custom_background:None,glass:false,compact_header:false,};call(&mut api,Command::SetTheme{theme:pref.clone()});let mut changed=call(&mut api,Command::GetSettings);changed["theme"]=original["theme"].clone();assert_eq!(changed,original);assert_eq!(call(&mut api,Command::Interaction),interaction);
 drop(api);let mut api=Service::open(root.path(),10000).unwrap();assert_eq!(call(&mut api,Command::Themes)["current"],serde_json::to_value(pref).unwrap());call(&mut api,Command::SetTheme{theme:Default::default()});assert_eq!(call(&mut api,Command::GetSettings),original);
 let old:katran::settings::Settings=serde_json::from_value(serde_json::json!({})).unwrap();assert_eq!(old.theme.preset,Preset::Standard);
 assert!(serde_json::from_value::<ThemePreferences>(serde_json::json!({"preset":"remote_url"})).is_err());
}

#[test]fn lily_and_notice_preferences_are_validated_and_persisted(){
 let root=tempfile::tempdir().unwrap();let mut api=Service::open(root.path(),10000).unwrap();
 let mut settings:katran::settings::Settings=serde_json::from_value(call(&mut api,Command::GetSettings)).unwrap();
 settings.theme.lily_background=Some(5);settings.sort_notifications=false;
 call(&mut api,Command::SetSettings{settings:settings.clone()});drop(api);
 let mut api=Service::open(root.path(),10000).unwrap();assert_eq!(call(&mut api,Command::GetSettings)["sort_notifications"],false);
 assert_eq!(call(&mut api,Command::Themes)["current"]["lily_background"],5);
 settings.theme.lily_background=Some(6);assert!(settings.validate(0).is_err());
}

#[test]fn experimental_window_preferences_roundtrip_through_api(){
 let root=tempfile::tempdir().unwrap();let mut api=Service::open(root.path(),10000).unwrap();
 let theme=ThemePreferences{preset:Preset::BlueExperimental,glass:true,lily_background:Some(2),compact_header:true,..Default::default()};
 call(&mut api,Command::SetTheme{theme:theme.clone()});drop(api);
 let mut api=Service::open(root.path(),10000).unwrap();assert_eq!(call(&mut api,Command::Themes)["current"],serde_json::to_value(theme).unwrap());
}

#[test]fn restart_is_a_host_intent_and_preserves_settings_and_interaction(){
 let root=tempfile::tempdir().unwrap();let mut api=Service::open(root.path(),10000).unwrap();
 let before=call(&mut api,Command::GetSettings);let interaction=call(&mut api,Command::Interaction);
 assert_eq!(call(&mut api,Command::RestartApplication)["restart_requested"],true);
 assert_eq!(call(&mut api,Command::GetSettings),before);assert_eq!(call(&mut api,Command::Interaction),interaction);
}

#[test]fn custom_background_import_select_and_reopen_preserve_managed_copy_and_presets(){
 let root=tempfile::tempdir().unwrap();let source=tempfile::tempdir().unwrap();let path=source.path().join("my-background.png");
 image::RgbaImage::new(8,6).save(&path).unwrap();let mut api=Service::open(root.path(),10000).unwrap();
 let item=call(&mut api,Command::ImportThemeBackground{path:path.clone()});let id=item["id"].as_str().unwrap().to_owned();
 assert_eq!(call(&mut api,Command::ImportThemeBackground{path:path.clone()})["id"],id);
 let theme=ThemePreferences{preset:Preset::BlueExperimental,custom_background:Some(id.clone()),glass:true,..Default::default()};
 call(&mut api,Command::SetTheme{theme});std::fs::remove_file(path).unwrap();drop(api);
 let mut api=Service::open(root.path(),10000).unwrap();assert_eq!(call(&mut api,Command::Themes)["current"]["custom_background"],id);
 assert!(std::path::Path::new(call(&mut api,Command::GetThemeBackground{id:id.clone()})["path"].as_str().unwrap()).exists());
 assert_eq!(call(&mut api,Command::ListThemeBackgrounds)["backgrounds"].as_array().unwrap().len(),1);
 call(&mut api,Command::SetTheme{theme:ThemePreferences{preset:Preset::BlueExperimental,lily_background:Some(3),..Default::default()}});
 assert_eq!(call(&mut api,Command::Themes)["current"]["lily_background"],3);
 let r=api.execute(Request{api_version:1,request_id:"invalid".into(),command:Command::GetThemeBackground{id:"../outside".into()}});assert_eq!(r["ok"],false);
}

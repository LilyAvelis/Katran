use katran::{Service,Command,Request,theme::{Preset,ThemePreferences}};
use serde_json::Value;
fn call(api:&mut Service,c:Command)->Value{let r=api.execute(Request{api_version:1,request_id:"themes".into(),command:c});assert_eq!(r["ok"],true,"{r}");r["result"].clone()}
#[test]fn theme_is_optional_persistent_and_does_not_change_behavior_preferences(){
 let root=tempfile::tempdir().unwrap();let mut api=Service::open(root.path(),10000).unwrap();let original=call(&mut api,Command::GetSettings);let interaction=call(&mut api,Command::Interaction);
 let catalog=call(&mut api,Command::Themes);assert_eq!(catalog["current"]["preset"],"standard");assert_eq!(catalog["themes"][1]["experimental"],true);
 let pref=ThemePreferences{preset:Preset::BlueExperimental,accent:Some([98,146,240]),decoration:true};call(&mut api,Command::SetTheme{theme:pref.clone()});let mut changed=call(&mut api,Command::GetSettings);changed["theme"]=original["theme"].clone();assert_eq!(changed,original);assert_eq!(call(&mut api,Command::Interaction),interaction);
 drop(api);let mut api=Service::open(root.path(),10000).unwrap();assert_eq!(call(&mut api,Command::Themes)["current"],serde_json::to_value(pref).unwrap());call(&mut api,Command::SetTheme{theme:Default::default()});assert_eq!(call(&mut api,Command::GetSettings),original);
 let old:katran::settings::Settings=serde_json::from_value(serde_json::json!({})).unwrap();assert_eq!(old.theme.preset,Preset::Standard);
 assert!(serde_json::from_value::<ThemePreferences>(serde_json::json!({"preset":"remote_url"})).is_err());
}

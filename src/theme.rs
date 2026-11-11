//! Persistent presentation preferences; domain behavior is independent of theme.
use serde::{Deserialize,Serialize};
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Preset{#[default] Standard,BlueExperimental}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(default,deny_unknown_fields)]
pub struct ThemePreferences{pub preset:Preset,pub accent:Option<[u8;3]>,pub decoration:bool}
impl Default for ThemePreferences{fn default()->Self{Self{preset:Preset::Standard,accent:None,decoration:true}}}
pub fn catalogue(current:&ThemePreferences)->serde_json::Value{serde_json::json!({"current":current,"themes":[{"id":"standard","label":"Стандартная","experimental":false},{"id":"blue_experimental","label":"Синяя · экспериментальная","experimental":true}]})}

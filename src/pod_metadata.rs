//! Read-only Unicode metadata for full, revision-checked Pods content.
use crate::{ApiError, Result, Service};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::OnceLock};
#[derive(Deserialize)]
struct Names {
    emoji: HashMap<String, String>,
    codepoints: HashMap<String, String>,
}
fn names() -> &'static Names {
    static DATA: OnceLock<Names> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/unicode/names.json"))
            .expect("bundled Unicode names")
    })
}
impl Service {
    pub(crate) fn describe_pod(&self, id: i64) -> Result<Value> {
        let value = self.get_text(id)?;
        let text = value["text"].as_str().unwrap_or("");
        if !crate::pods::is_pod(text) {
            return Err(ApiError::new("not_pod", "Объект не является Pods"));
        }
        let data = names();
        let points:Vec<Value>=text.chars().take(256).map(|ch|{
    let hex=format!("{:04X}",ch as u32);let mut bytes=[0;4];let encoded=ch.encode_utf8(&mut bytes);
    json!({"codepoint":format!("U+{hex}"),"name":data.codepoints.get(&hex).map(String::as_str).unwrap_or("Unicode character"),"utf8":encoded.as_bytes().iter().map(|b|format!("{b:02X}")).collect::<Vec<_>>().join(" ")})
  }).collect();
        let name = data.emoji.get(text).cloned().unwrap_or_else(|| {
            if text.chars().count() == 1 {
                points
                    .first()
                    .and_then(|v| v["name"].as_str())
                    .unwrap_or("Символ")
                    .to_owned()
            } else {
                "Комбинация emoji и символов".into()
            }
        });
        Ok(
            json!({"id":id,"revision":value["revision"],"text":text,"name":name,"unicode_version":"16.0","codepoints":points,"codepoint_count":text.chars().count(),"utf8_bytes":text.len(),"utf16_units":text.encode_utf16().count(),"metadata_truncated":text.chars().count()>256}),
        )
    }
}

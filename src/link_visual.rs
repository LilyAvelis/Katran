//! Link identity and optional favicon resolution through the dispatcher.
//! The command is synchronous; UI clients must call it on a bounded worker.
use crate::{ApiError, EventKind, Result, Service, favicon_fetch, link_fragments};
use base64::Engine;
use serde_json::{Value, json};
use url::Url;

fn identity(url: &str) -> Result<(String, String, String, [u8; 3])> {
    let parsed = Url::parse(url).map_err(|_| ApiError::new("invalid_link", "Invalid link URL"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| ApiError::new("invalid_link", "Link has no host"))?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let labels: Vec<_> = host.split('.').collect();
    let first = labels.first().copied().unwrap_or("");
    let site = if matches!(first, "www" | "app" | "m" | "mobile") && labels.len() > 1 {
        labels[1]
    } else {
        first
    };
    let site = site.to_owned();
    let mark = site
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    let mark = if mark.is_empty() { "↗".into() } else { mark };
    let hash = host.bytes().fold(2_166_136_261u32, |value, byte| {
        (value ^ u32::from(byte)).wrapping_mul(16_777_619)
    });
    let color = [
        55 + (hash & 0x7f) as u8,
        65 + ((hash >> 8) & 0x7f) as u8,
        75 + ((hash >> 16) & 0x7f) as u8,
    ];
    Ok((host, site, mark, color))
}

impl Service {
    /// Enumerate only stable IDs; URL contents stay in the catalogue.
    pub(crate) fn list_link_visual_ids(&self, after_id: i64, limit: u32) -> Result<Value> {
        if after_id < 0 || !(1..=500).contains(&limit) {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected nonnegative cursor and limit 1..500",
            ));
        }
        let mut stmt = self.db.prepare(
            "SELECT event_id FROM link_fragments WHERE event_id > ?1 ORDER BY event_id LIMIT ?2",
        )?;
        let mut ids = stmt
            .query_map(rusqlite::params![after_id, i64::from(limit) + 1], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let has_more = ids.len() > limit as usize;
        if has_more {
            ids.pop();
        }
        let next_after_id = if has_more { ids.last().copied() } else { None };
        Ok(json!({"ids": ids, "next_after_id": next_after_id}))
    }
    pub(crate) fn get_link_visual(&self, id: i64) -> Result<Value> {
        let event = self.get(id)?;
        if event.kind != EventKind::LinkFragment {
            return Err(ApiError::new("not_link", "Item is not a link"));
        }
        let url = link_fragments::read_event(&event)?;
        let (host, site, mark, color) = identity(&url)?;
        let mut favicon = None;
        let mut warning = None;
        if self.settings.link_favicons_online {
            match favicon_fetch::fetch(&url) {
                Ok(Some(bytes)) => {
                    favicon = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
                }
                Ok(None) => warning = Some("favicon_unavailable".to_owned()),
                Err(error) => warning = Some(format!("favicon_unavailable: {error}")),
            }
        }
        Ok(json!({
            "id": id,
            "host": host,
            "site": site,
            "mark": mark,
            "color": color,
            "favicon_base64": favicon,
            "warning": warning,
            "online": self.settings.link_favicons_online,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_keeps_host_but_uses_readable_site_mark() {
        let (host, site, mark, color) = identity("https://app.notion.com/p/123").unwrap();
        assert_eq!(host, "app.notion.com");
        assert_eq!(site, "notion");
        assert_eq!(mark, "NO");
        assert!(color.iter().all(|component| (55..=202).contains(component)));
    }
}

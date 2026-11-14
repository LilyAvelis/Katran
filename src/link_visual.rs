//! Link identity and optional favicon resolution through the dispatcher.
//! The command is synchronous; UI clients must call it on a bounded worker.
use crate::{ApiError, EventKind, Result, Service, favicon_fetch, link_fragments};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;

const CACHE_ENTRIES: i64 = 256;
const CACHE_BYTES: i64 = 256 * 1024;
const NEGATIVE_TTL: i64 = 24 * 60 * 60;

// Separate derived-data database: a read-only catalogue worker may safely update
// it. SQLite transactions make replacements and eviction atomic across workers.
fn cache_connection(root: &Path) -> rusqlite::Result<rusqlite::Connection> {
    let db = rusqlite::Connection::open(root.join("favicon-cache.sqlite3"))?;
    db.busy_timeout(Duration::from_millis(500))?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS icons (host TEXT PRIMARY KEY, image BLOB NOT NULL);
         CREATE TABLE IF NOT EXISTS missing_icons (host TEXT PRIMARY KEY, checked_at INTEGER NOT NULL)",
    )?;
    Ok(db)
}

fn cached_fetch<F>(
    root: &Path,
    url: &str,
    force: bool,
    fetch: F,
) -> std::result::Result<Option<Vec<u8>>, favicon_fetch::Error>
where
    F: FnOnce(&str) -> std::result::Result<Option<Vec<u8>>, favicon_fetch::Error>,
{
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64;
    cached_fetch_at(root, url, force, now, fetch)
}

fn cached_fetch_at<F>(
    root: &Path,
    url: &str,
    force: bool,
    now: i64,
    fetch: F,
) -> std::result::Result<Option<Vec<u8>>, favicon_fetch::Error>
where
    F: FnOnce(&str) -> std::result::Result<Option<Vec<u8>>, favicon_fetch::Error>,
{
    let host = favicon_fetch::cache_host(url)?;
    let mut db = cache_connection(root).ok();
    if !force && let Some(db) = db.as_ref() {
        let bytes: rusqlite::Result<Vec<u8>> = db.query_row(
            "SELECT image FROM icons WHERE host = ?1 AND length(image) <= ?2",
            rusqlite::params![host, CACHE_BYTES],
            |row| row.get(0),
        );
        if let Ok(bytes) = bytes
            && let Some(image) = favicon_fetch::image_bytes(&bytes)
        {
            return Ok(Some(image));
        }
        let missing: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM missing_icons WHERE host = ?1 AND checked_at <= ?2 AND checked_at > ?3)",
            rusqlite::params![host, now, now.saturating_sub(NEGATIVE_TTL)], |row| row.get(0)
        ).unwrap_or(false);
        if missing {
            return Ok(None);
        }
    }
    let result = fetch(url)?;
    // Store only fully decoded, bounded positive results. Persistence failure is
    // cosmetic and never changes the result returned to the caller.
    if let Some(image) = result
        .as_ref()
        .and_then(|bytes| favicon_fetch::image_bytes(bytes))
        && let Some(db) = db.as_mut()
        && let Ok(tx) = db.transaction()
    {
        let written = tx.execute("DELETE FROM missing_icons WHERE host = ?1", [&host])
            .and_then(|_| tx.execute("DELETE FROM icons WHERE host = ?1", [&host]))
            .and_then(|_| tx.execute("INSERT INTO icons(host, image) VALUES (?1, ?2)", rusqlite::params![host, image]))
            .and_then(|_| tx.execute("DELETE FROM icons WHERE rowid IN (SELECT rowid FROM icons ORDER BY rowid DESC LIMIT -1 OFFSET ?1)", [CACHE_ENTRIES]));
        if written.is_ok() {
            let _ = tx.commit();
        }
    }
    if result.is_none()
        && let Some(db) = db.as_mut()
        && let Ok(tx) = db.transaction()
    {
        let written = tx.execute("DELETE FROM icons WHERE host = ?1", [&host])
            .and_then(|_| tx.execute("DELETE FROM missing_icons WHERE host = ?1 OR checked_at <= ?2 OR checked_at > ?3", rusqlite::params![host, now.saturating_sub(NEGATIVE_TTL), now]))
            .and_then(|_| tx.execute("INSERT INTO missing_icons(host, checked_at) VALUES (?1, ?2)", rusqlite::params![host, now]))
            .and_then(|_| tx.execute("DELETE FROM missing_icons WHERE rowid IN (SELECT rowid FROM missing_icons ORDER BY rowid DESC LIMIT -1 OFFSET ?1)", [CACHE_ENTRIES]));
        if written.is_ok() {
            let _ = tx.commit();
        }
    }
    Ok(result)
}

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
    pub(crate) fn get_link_visual(&self, id: i64, force_refresh: bool) -> Result<Value> {
        let event = self.get(id)?;
        if event.kind != EventKind::LinkFragment {
            return Err(ApiError::new("not_link", "Item is not a link"));
        }
        let url = link_fragments::read_event(&event)?;
        let (host, site, mark, color) = identity(&url)?;
        let mut favicon = None;
        let mut warning = None;
        if self.settings.link_favicons_online {
            match cached_fetch(&self.root, &url, force_refresh, favicon_fetch::fetch) {
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
    fn cache_survives_reopen_and_force_refresh_replaces_positive_entry() {
        use std::{cell::Cell, io::Cursor};
        fn png(color: u8) -> Vec<u8> {
            let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                2,
                2,
                image::Rgba([color, 0, 0, 255]),
            ));
            let mut output = Cursor::new(Vec::new());
            image
                .write_to(&mut output, image::ImageFormat::Png)
                .unwrap();
            output.into_inner()
        }
        let root = tempfile::tempdir().unwrap();
        let count = Cell::new(0);
        let a = png(1);
        let b = png(2);
        let url = "https://Example.COM/path?private=yes";
        let fetch_a = |_: &str| {
            count.set(count.get() + 1);
            Ok(Some(a.clone()))
        };
        assert_eq!(
            cached_fetch(root.path(), url, false, fetch_a).unwrap(),
            Some(a.clone())
        );
        let no_fetch = |_: &str| -> std::result::Result<Option<Vec<u8>>, favicon_fetch::Error> {
            panic!("cache should avoid network")
        };
        assert_eq!(
            cached_fetch(root.path(), "https://example.com/another", false, no_fetch).unwrap(),
            Some(a)
        );
        assert_eq!(count.get(), 1);
        assert_eq!(
            cached_fetch(root.path(), url, true, |_| Ok(Some(b.clone()))).unwrap(),
            Some(b.clone())
        );
        assert_eq!(
            cached_fetch(root.path(), url, false, no_fetch).unwrap(),
            Some(b)
        );
        assert!(
            cached_fetch(
                root.path(),
                "https://user:secret@example.com/",
                false,
                no_fetch
            )
            .is_err()
        );
    }

    #[test]
    fn missing_icon_cache_expires_and_force_bypasses_it() {
        let root = tempfile::tempdir().unwrap();
        let url = "https://missing.example/path";
        let calls = std::cell::Cell::new(0);
        let missing = |_: &str| {
            calls.set(calls.get() + 1);
            Ok(None)
        };
        assert_eq!(
            cached_fetch_at(root.path(), url, false, 100_000, missing).unwrap(),
            None
        );
        assert_eq!(
            cached_fetch_at(root.path(), url, false, 100_001, missing).unwrap(),
            None
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(
            cached_fetch_at(root.path(), url, true, 100_002, missing).unwrap(),
            None
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(
            cached_fetch_at(root.path(), url, false, 100_002 + NEGATIVE_TTL, missing).unwrap(),
            None
        );
        assert_eq!(calls.get(), 3);
        let err_url = "https://error.example/";
        for _ in 0..2 {
            assert!(
                cached_fetch_at(root.path(), err_url, false, 100_000, |_| Err(
                    favicon_fetch::Error::UnsafeAddress
                ))
                .is_err()
            );
        }
        assert_eq!(
            cached_fetch_at(root.path(), err_url, false, 100_000, missing).unwrap(),
            None
        );
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn cache_rejects_corruption_and_evicts_old_entries() {
        let root = tempfile::tempdir().unwrap();
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut output, image::ImageFormat::Png)
            .unwrap();
        let icon = output.into_inner();
        let db = cache_connection(root.path()).unwrap();
        db.execute("INSERT INTO icons VALUES ('broken.example', x'010203')", [])
            .unwrap();
        drop(db);
        assert_eq!(
            cached_fetch(root.path(), "https://broken.example/", false, |_| Ok(Some(
                icon.clone()
            )))
            .unwrap(),
            Some(icon.clone())
        );
        for index in 0..CACHE_ENTRIES {
            let url = format!("https://host{index}.example/");
            cached_fetch(root.path(), &url, false, |_| Ok(Some(icon.clone()))).unwrap();
        }
        let db = cache_connection(root.path()).unwrap();
        let count: i64 = db
            .query_row("SELECT count(*) FROM icons", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, CACHE_ENTRIES);
        let oldest: i64 = db
            .query_row(
                "SELECT count(*) FROM icons WHERE host = 'broken.example'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(oldest, 0);
    }

    #[test]
    fn identity_keeps_host_but_uses_readable_site_mark() {
        let (host, site, mark, color) = identity("https://app.notion.com/p/123").unwrap();
        assert_eq!(host, "app.notion.com");
        assert_eq!(site, "notion");
        assert_eq!(mark, "NO");
        assert!(color.iter().all(|component| (55..=202).contains(component)));
    }
}

//! Read-only Notion source backed by the official ntn executable.
use crate::{ApiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(8);

const FOLDER_PAGE_BASE: i64 = -(1_i64 << 61) + 1;
const FOLDER_PAGE_LIMIT: i64 = FOLDER_PAGE_BASE + (1_i64 << 40);

pub fn is_folder_page_id(id: i64) -> bool {
    (FOLDER_PAGE_BASE..FOLDER_PAGE_LIMIT).contains(&id)
}

pub fn next_folder_page_id() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static NEXT: AtomicI64 = AtomicI64::new(FOLDER_PAGE_BASE);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Page {
    pub id: String,
    pub title: String,
}

impl Page {
    pub(crate) fn browser_url(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "https://www.notion.so/{}",
            self.id.replace('-', "")
        ))
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if !page_id(&self.id) || self.title.len() > 8192 {
            return Err(ApiError::new("notion_page", "Invalid Notion page"));
        }
        Ok(())
    }

    pub(crate) fn filename(&self) -> String {
        let title: String = self
            .title
            .chars()
            .map(|ch| {
                if ch.is_control() || "\\/:*?\"<>|".contains(ch) {
                    ' '
                } else {
                    ch
                }
            })
            .take(45)
            .collect();
        let title = title.trim_matches([' ', '.']);
        if title.is_empty() {
            format!("Notion-{}.md", &self.id[..8])
        } else {
            format!("Notion - {title}.md")
        }
    }
}

fn page_library(root: &std::path::Path) -> Result<rusqlite::Connection> {
    let db = rusqlite::Connection::open(root.join("notion-pages.sqlite3"))?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS pages (
            id TEXT PRIMARY KEY, title TEXT NOT NULL, visited INTEGER, pinned INTEGER
        );",
    )?;
    Ok(db)
}

pub(crate) fn saved_pages(root: &std::path::Path, favorites: bool) -> Result<Vec<Page>> {
    let db = page_library(root)?;
    let sql = if favorites {
        "SELECT id,title FROM pages WHERE pinned IS NOT NULL ORDER BY pinned DESC, rowid DESC"
    } else {
        "SELECT id,title FROM pages WHERE visited IS NOT NULL ORDER BY visited DESC, rowid DESC LIMIT 100"
    };
    let mut query = db.prepare(sql)?;
    let pages = query
        .query_map([], |row| {
            Ok(Page {
                id: row.get(0)?,
                title: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(pages)
}

pub(crate) fn toggle_favorite(root: &std::path::Path, page: &Page) -> Result<bool> {
    page.validate()?;
    let db = page_library(root)?;
    let pinned = db.query_row(
        "INSERT INTO pages(id,title,pinned) VALUES(?1,?2,?3)
            ON CONFLICT(id) DO UPDATE SET title=excluded.title,
            pinned=CASE WHEN pages.pinned IS NULL THEN excluded.pinned ELSE NULL END
            RETURNING pinned IS NOT NULL",
        rusqlite::params![page.id, page.title, crate::now_ms()],
        |row| row.get(0),
    )?;
    db.execute(
        "DELETE FROM pages WHERE pinned IS NULL AND visited IS NULL",
        [],
    )?;
    Ok(pinned)
}

pub(crate) fn record_visit(root: &std::path::Path, page: &Page) -> Result<()> {
    let mut db = page_library(root)?;
    let tx = db.transaction()?;
    tx.execute(
        "INSERT INTO pages(id,title,visited) VALUES(?1,?2,?3)
            ON CONFLICT(id) DO UPDATE SET title=COALESCE(NULLIF(excluded.title,''),pages.title),
            visited=excluded.visited",
        rusqlite::params![page.id, page.title, crate::now_ms()],
    )?;
    tx.execute_batch(
        "UPDATE pages SET visited=NULL WHERE id IN
            (SELECT id FROM pages WHERE visited IS NOT NULL ORDER BY visited DESC, rowid DESC LIMIT -1 OFFSET 100);
         DELETE FROM pages WHERE pinned IS NULL AND visited IS NULL;",
    )?;
    tx.commit()?;
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct Token(pub String);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

pub(crate) fn configured() -> bool {
    credentials::read().ok().flatten().is_some()
}

pub(crate) fn looks_like_token(text: &str) -> bool {
    let text = text.trim();
    (text.starts_with("ntn_") || text.starts_with("secret_"))
        && (32..=512).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub(crate) fn set_token(token: Option<Token>) -> Result<()> {
    let value = token.as_ref().map(|token| token.0.trim());
    if value.is_some_and(|s| !looks_like_token(s)) {
        return Err(ApiError::new(
            "notion_token",
            "Enter an internal Notion integration token",
        ));
    }
    credentials::write(value)
}

#[cfg(windows)]
mod credentials {
    use super::*;
    use windows_sys::Win32::{Foundation::ERROR_NOT_FOUND, Security::Credentials::*};

    fn target() -> Vec<u16> {
        "Katran/Notion/Read".encode_utf16().chain([0]).collect()
    }

    fn store_error() -> ApiError {
        ApiError::new(
            "notion_credentials",
            "Windows Credential Manager is unavailable",
        )
    }

    fn absent() -> bool {
        std::io::Error::last_os_error().raw_os_error() == Some(ERROR_NOT_FOUND as i32)
    }

    pub(super) fn read() -> Result<Option<String>> {
        let mut ptr = std::ptr::null_mut();
        if unsafe { CredReadW(target().as_ptr(), CRED_TYPE_GENERIC, 0, &mut ptr) } == 0 {
            return if absent() {
                Ok(None)
            } else {
                Err(store_error())
            };
        }
        let result = unsafe {
            let credential = &*ptr;
            let len = credential.CredentialBlobSize as usize;
            if credential.CredentialBlob.is_null() || !(16..=512).contains(&len) {
                Err(store_error())
            } else {
                std::str::from_utf8(std::slice::from_raw_parts(credential.CredentialBlob, len))
                    .map(|token| Some(token.to_owned()))
                    .map_err(|_| store_error())
            }
        };
        unsafe { CredFree(ptr.cast()) };
        result
    }

    pub(super) fn write(token: Option<&str>) -> Result<()> {
        let mut target = target();
        let Some(token) = token else {
            return if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 || absent()
            {
                Ok(())
            } else {
                Err(store_error())
            };
        };
        let mut blob = token.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(store_error());
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod credentials {
    use super::*;

    pub(super) fn read() -> Result<Option<String>> {
        Ok(None)
    }

    pub(super) fn write(_: Option<&str>) -> Result<()> {
        Err(ApiError::new(
            "notion_credentials",
            "Notion credential storage requires Windows",
        ))
    }
}

fn page_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn page_from_result(value: &Value) -> Option<Page> {
    let id = value["id"].as_str().filter(|id| page_id(id))?;
    if value["object"] != "page" || value["in_trash"] == true || value["archived"] == true {
        return None;
    }
    let fragments = value["properties"]
        .as_object()?
        .values()
        .find_map(|property| {
            (property["type"] == "title")
                .then(|| property["title"].as_array())
                .flatten()
        })?;
    let title: String = fragments
        .iter()
        .filter_map(|item| item["plain_text"].as_str())
        .collect();
    Some(Page {
        id: id.into(),
        title: if title.trim().is_empty() {
            "Untitled".into()
        } else {
            title
        },
    })
}

pub(crate) fn search(query: &str, check: impl Fn() -> Result<()>) -> Result<Value> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > 256 {
        return Err(ApiError::new(
            "notion_query",
            "Enter a page title of up to 256 characters",
        ));
    }
    let body = json!({
        "query": query,
        "page_size": 25,
        "filter": {"property": "object", "value": "page"}
    });
    let response = request("v1/search", Some(body), &[], check)?;
    let results = response["results"].as_array().ok_or_else(response_error)?;
    let pages: Vec<_> = results
        .iter()
        .take(25)
        .filter_map(page_from_result)
        .collect();
    Ok(json!({
        "pages": pages,
        "more": response["has_more"] == true
            || response["request_status"]["type"] == "incomplete",
    }))
}

const DATABASE_PAGE_SIZE: usize = 30;
const DATABASE_SEARCH_SIZE: usize = 25;
const MAX_NOTION_CURSOR: usize = 2048;

fn checked_database_query(query: &str) -> Result<&str> {
    let query = query.trim();
    if query.chars().count() > 256 || query.chars().any(char::is_control) {
        return Err(ApiError::new(
            "notion_query",
            "Search must contain at most 256 characters",
        ));
    }
    Ok(query)
}
fn valid_cursor(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_NOTION_CURSOR && !value.chars().any(char::is_control)
}
fn rich_text(value: &Value, limit: usize) -> String {
    let mut result = String::new();
    let mut count = 0usize;
    if let Some(fragments) = value.as_array() {
        for fragment in fragments {
            if let Some(text) = fragment["plain_text"].as_str() {
                for ch in text.chars().take(limit.saturating_sub(count)) {
                    result.push(ch);
                    count += 1;
                }
                if count >= limit {
                    break;
                }
            }
        }
    }
    result
}
fn normalized_text(value: &Value, limit: usize) -> String {
    value.as_str().unwrap_or("").chars().take(limit).collect()
}
fn database_source(value: &Value) -> Option<Value> {
    let id = value["id"].as_str().filter(|id| page_id(id))?;
    if value["object"] != "data_source"
        || value["in_trash"] == true
        || value["archived"] == true
        || value["is_archived"] == true
    {
        return None;
    }
    let title = rich_text(&value["title"], 1024);
    Some(json!({"id":id,"title":if title.trim().is_empty() { "Untitled" } else { &title }}))
}
fn search_databases_with(
    query: &str,
    check: &impl Fn() -> Result<()>,
    mut send: impl FnMut(&str, Option<Value>) -> Result<Value>,
) -> Result<Value> {
    let query = checked_database_query(query)?;
    let mut body = json!({"page_size":DATABASE_SEARCH_SIZE,"filter":{"property":"object","value":"data_source"}});
    if !query.is_empty() {
        body["query"] = json!(query);
    }
    check()?;
    let response = send("v1/search", Some(body))?;
    check()?;
    let results = response["results"].as_array().ok_or_else(response_error)?;
    if results.len() > DATABASE_SEARCH_SIZE {
        return Err(response_error());
    }
    let sources: Vec<_> = results.iter().filter_map(database_source).collect();
    Ok(
        json!({"sources":sources,"more":response["has_more"] == true || response["request_status"]["type"] == "incomplete"}),
    )
}
pub(crate) fn search_databases(query: &str, check: impl Fn() -> Result<()>) -> Result<Value> {
    search_databases_with(query, &check, |path, body| request(path, body, &[], &check))
}

fn title_property(schema: &Value, source_id: &str) -> Result<String> {
    if schema["object"] != "data_source"
        || !schema["id"]
            .as_str()
            .is_some_and(|id| id.eq_ignore_ascii_case(source_id))
    {
        return Err(response_error());
    }
    let properties = schema["properties"]
        .as_object()
        .ok_or_else(response_error)?;
    let mut titles = properties
        .iter()
        .filter(|(_, value)| value["type"] == "title");
    let (name, _) = titles.next().ok_or_else(response_error)?;
    if titles.next().is_some()
        || name.is_empty()
        || name.len() > 1024
        || name.chars().any(char::is_control)
    {
        return Err(response_error());
    }
    Ok(name.clone())
}
fn property_text(property: &Value) -> Option<String> {
    let kind = property["type"].as_str()?;
    let text = match kind {
        "title" | "rich_text" => rich_text(&property[kind], 1024),
        "select" | "status" => normalized_text(&property[kind]["name"], 1024),
        "multi_select" => property[kind]
            .as_array()?
            .iter()
            .take(32)
            .map(|entry| normalized_text(&entry["name"], 128))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        "date" => {
            let start = normalized_text(&property[kind]["start"], 128);
            let end = normalized_text(&property[kind]["end"], 128);
            if !start.is_empty() && !end.is_empty() {
                format!("{start} – {end}")
            } else {
                start
            }
        }
        "number" => property[kind]
            .as_number()
            .map(ToString::to_string)
            .unwrap_or_default(),
        "checkbox" => property[kind]
            .as_bool()
            .map(|value| if value { "✓" } else { "☐" })
            .unwrap_or("")
            .to_owned(),
        "url" | "email" | "phone_number" => normalized_text(&property[kind], 1024),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}
fn database_row(value: &Value) -> Option<Value> {
    let id = value["id"].as_str().filter(|id| page_id(id))?;
    if value["object"] != "page"
        || value["in_trash"] == true
        || value["archived"] == true
        || value["is_archived"] == true
    {
        return None;
    }
    let props = value["properties"].as_object()?;
    let mut title = props
        .values()
        .find(|prop| prop["type"] == "title")
        .map(|prop| rich_text(&prop["title"], 1024))
        .unwrap_or_default();
    let mut summary = String::new();
    let mut properties = Vec::new();
    for (name, prop) in props.iter().take(128) {
        let kind = prop["type"].as_str().unwrap_or("");
        let Some(text) = property_text(prop) else {
            continue;
        };
        if kind == "title" {
            continue;
        }
        if kind == "rich_text" && summary.is_empty() {
            summary = text.chars().take(512).collect();
        }
        if properties.len() < 32 {
            properties
                .push(json!({"name":name.chars().take(256).collect::<String>(),"value":text}));
        }
    }
    if title.trim().is_empty() {
        title = "Untitled".into();
    }
    Some(
        json!({"id":id,"title":title,"summary":summary,"properties":properties,
        "created_at":normalized_text(&value["created_time"],128),"edited_at":normalized_text(&value["last_edited_time"],128)}),
    )
}
fn query_database_with(
    source_id: &str,
    query: &str,
    cursor: Option<&str>,
    sort: Option<&str>,
    descending: bool,
    check: &impl Fn() -> Result<()>,
    mut send: impl FnMut(&str, Option<Value>) -> Result<Value>,
) -> Result<Value> {
    if !page_id(source_id) {
        return Err(ApiError::new(
            "notion_source_id",
            "Invalid Notion data source ID",
        ));
    }
    let query = checked_database_query(query)?;
    if cursor.is_some_and(|cursor| !valid_cursor(cursor)) {
        return Err(ApiError::new("notion_cursor", "Invalid Notion cursor"));
    }
    if sort.is_some_and(|sort| !matches!(sort, "edited" | "created" | "title")) {
        return Err(ApiError::new("notion_sort", "Invalid Notion sort"));
    }
    let source_path = format!("v1/data_sources/{source_id}");
    let title = if !query.is_empty() || sort == Some("title") {
        check()?;
        let schema = send(&source_path, None)?;
        check()?;
        Some(title_property(&schema, source_id)?)
    } else {
        None
    };
    let mut body = json!({"page_size":DATABASE_PAGE_SIZE});
    if let Some(cursor) = cursor {
        body["start_cursor"] = json!(cursor);
    }
    if !query.is_empty() {
        body["filter"] =
            json!({"property":title.as_ref().expect("title resolved"),"title":{"contains":query}});
    }
    if let Some(sort) = sort {
        let direction = if descending {
            "descending"
        } else {
            "ascending"
        };
        body["sorts"] = match sort {
            "title" => {
                json!([{"property":title.as_ref().expect("title resolved"),"direction":direction}])
            }
            "created" => json!([{"timestamp":"created_time","direction":direction}]),
            "edited" => json!([{"timestamp":"last_edited_time","direction":direction}]),
            _ => unreachable!(),
        };
    }
    check()?;
    let response = send(&format!("{source_path}/query"), Some(body))?;
    check()?;
    let results = response["results"].as_array().ok_or_else(response_error)?;
    if results.len() > DATABASE_PAGE_SIZE {
        return Err(response_error());
    }
    let rows: Vec<_> = results.iter().filter_map(database_row).collect();
    let next = match response.get("next_cursor") {
        Some(Value::Null) | None if response["has_more"] != true => None,
        Some(Value::String(cursor)) if valid_cursor(cursor) && response["has_more"] == true => {
            Some(cursor.clone())
        }
        _ => return Err(response_error()),
    };
    Ok(
        json!({"rows":rows,"next_cursor":next,"incomplete":response["request_status"]["type"] == "incomplete"}),
    )
}
pub(crate) fn query_database(
    source_id: &str,
    query: &str,
    cursor: Option<&str>,
    sort: Option<&str>,
    descending: bool,
    check: impl Fn() -> Result<()>,
) -> Result<Value> {
    query_database_with(
        source_id,
        query,
        cursor,
        sort,
        descending,
        &check,
        |path, body| request(path, body, &[], &check),
    )
}

pub(crate) fn markdown(id: &str, check: impl Fn() -> Result<()>) -> Result<String> {
    fetch_markdown(id, &[], check)
}

pub(crate) fn preview_markdown(id: &str, check: impl Fn() -> Result<()>) -> Result<String> {
    fetch_markdown(id, &["markdown_version==v2"], check)
}

pub(crate) fn page_excerpt(id: &str, check: impl Fn() -> Result<()>) -> Result<String> {
    if !page_id(id) {
        return Err(ApiError::new("notion_page_id", "Invalid Notion page ID"));
    }
    let response = request(
        &format!("v1/blocks/{id}/children"),
        None,
        &["page_size==6"],
        check,
    )?;
    excerpt_from_response(&response)
}

fn excerpt_from_response(response: &Value) -> Result<String> {
    let blocks = response["results"].as_array().ok_or_else(response_error)?;
    let text = blocks
        .iter()
        .take(6)
        .filter_map(|block| {
            let content = block.get(block["type"].as_str()?)?;
            let line = rich_text(&content["rich_text"], 224);
            (!line.trim().is_empty()).then_some(line)
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(text.trim().chars().take(224).collect())
}

#[cfg(test)]
#[test]
fn notion_excerpt_uses_plain_text_and_limits_length() {
    let response = json!({"results":[
        {"type":"image","image":{"external":{"url":"https://example.com"}}},
        {"type":"paragraph","paragraph":{"rich_text":[{"plain_text":"Раздел "},{"plain_text":"🐳".repeat(300)}]}},
    ]});
    let excerpt = excerpt_from_response(&response).unwrap();
    assert!(excerpt.starts_with("Раздел 🐳"));
    assert_eq!(excerpt.chars().count(), 224);
}

fn fetch_markdown(id: &str, query: &[&str], check: impl Fn() -> Result<()>) -> Result<String> {
    if !page_id(id) {
        return Err(ApiError::new("notion_page_id", "Invalid Notion page ID"));
    }
    let response = request(&format!("v1/pages/{id}/markdown"), None, query, check)?;
    read_markdown(&response)
}

fn read_markdown(response: &Value) -> Result<String> {
    let markdown = response["markdown"].as_str().ok_or_else(response_error)?;
    if response["truncated"] == true {
        return Err(ApiError::new(
            "notion_truncated",
            "Notion returned only part of this page",
        ));
    }
    if markdown.len() > crate::text_fragments::MAX_TEXT_BYTES {
        return Err(ApiError::new(
            "notion_page_too_large",
            "Notion page exceeds 1 MiB",
        ));
    }
    Ok(markdown.into())
}

fn response_error() -> ApiError {
    ApiError::new("notion_response", "Could not read the Notion CLI response")
}

#[cfg(windows)]
fn cli_executable() -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_ProgramFiles};

    // Raycast and Explorer can retain the PATH they received at login.
    let user_root =
        windows_folder(&FOLDERID_LocalAppData).map(|path| path.join("Microsoft").join("WinGet"));
    let machine_root = windows_folder(&FOLDERID_ProgramFiles).map(|path| path.join("WinGet"));
    let path = std::env::var_os("PATH").unwrap_or_default();
    find_windows_cli(
        std::env::split_paths(&path),
        user_root.into_iter().chain(machine_root),
    )
}

#[cfg(windows)]
fn windows_folder(id: &windows::core::GUID) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    unsafe {
        let value = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let path = std::ffi::OsString::from_wide(value.as_wide()).into();
        CoTaskMemFree(Some(value.0.cast()));
        Some(path)
    }
}

#[cfg(windows)]
fn find_windows_cli(
    path: impl IntoIterator<Item = std::path::PathBuf>,
    winget_roots: impl IntoIterator<Item = std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    if let Some(executable) = path
        .into_iter()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("ntn.exe"))
        .find(|path| path.is_file())
    {
        return Some(executable);
    }
    let architecture = format!("ntn-{}-pc-windows-msvc", std::env::consts::ARCH);
    for root in winget_roots {
        let link = root.join("Links").join("ntn.exe");
        if link.is_file() {
            return Some(link);
        }
        let Ok(packages) = std::fs::read_dir(root.join("Packages")) else {
            continue;
        };
        for package in packages.flatten() {
            if !package
                .file_name()
                .to_string_lossy()
                .starts_with("Notion.ntn_")
            {
                continue;
            }
            let path = package.path();
            for executable in [
                path.join(&architecture).join("ntn.exe"),
                path.join("ntn.exe"),
            ] {
                if executable.is_file() {
                    return Some(executable);
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn cli_executable() -> Option<std::path::PathBuf> {
    Some("ntn".into())
}

fn request(
    path: &str,
    body: Option<Value>,
    query: &[&str],
    check: impl Fn() -> Result<()>,
) -> Result<Value> {
    check()?;
    let token = credentials::read()?.ok_or_else(|| {
        ApiError::new("notion_config", "Connect Notion in Settings → Integrations")
    })?;
    let executable = cli_executable().ok_or_else(|| {
        ApiError::new(
            "notion_cli_missing",
            "Notion CLI (ntn) was not found in PATH or WinGet",
        )
    })?;
    let mut command = Command::new(executable);
    command
        .args(["api", path, "--notion-version", "2026-03-11"])
        .env("NOTION_API_TOKEN", token.trim())
        .env("NOTION_API_BASE_URL", "https://api.notion.com")
        .env("NOTION_API_DOCS_BASE_URL", "https://developers.notion.com")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if body.is_some() {
        command
            .args(["-X", "POST", "--data", "@-"])
            .stdin(Stdio::piped());
    } else {
        command.args(["-X", "GET"]).stdin(Stdio::null());
    }
    command.args(query);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|error| {
        ApiError::new(
            "notion_cli_start",
            format!("Could not start Notion CLI (ntn): {error}"),
        )
    })?;
    let result = (|| {
        if let Some(body) = body {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(body.to_string().as_bytes())
                .map_err(|_| response_error())?;
        }
        let stdout = child.stdout.take().unwrap();
        let (send, receive) = mpsc::channel();
        std::thread::Builder::new()
            .spawn(move || {
                let mut bytes = Vec::new();
                let result = stdout
                    .take((MAX_RESPONSE + 1) as u64)
                    .read_to_end(&mut bytes);
                let _ = send.send(result.map(|_| bytes));
            })
            .map_err(|_| response_error())?;
        let deadline = Instant::now() + DEADLINE;
        let mut output = None;
        loop {
            check()?;
            if output.is_none() {
                match receive.try_recv() {
                    Ok(Ok(bytes)) if bytes.len() <= MAX_RESPONSE => output = Some(bytes),
                    Ok(_) | Err(mpsc::TryRecvError::Disconnected) => return Err(response_error()),
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if let Some(bytes) = output.as_ref()
                && let Some(status) = child.try_wait().map_err(|_| response_error())?
            {
                if !status.success() {
                    return Err(ApiError::new(
                        "notion_request",
                        "Notion request failed. Check the token, page access and connection",
                    ));
                }
                return serde_json::from_slice(bytes).map_err(|_| response_error());
            }
            if Instant::now() >= deadline {
                return Err(ApiError::new(
                    "notion_timeout",
                    "Notion did not respond within 8 seconds",
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_history_and_favorites_are_independent_and_markdown_is_a_file() {
        let root = tempfile::tempdir().unwrap();
        let page = Page {
            id: "12345678-0000-0000-0000-000000000000".into(),
            title: "../Baldur: notes".into(),
        };
        assert!(toggle_favorite(root.path(), &page).unwrap());
        assert!(saved_pages(root.path(), false).unwrap().is_empty());
        record_visit(root.path(), &page).unwrap();
        assert_eq!(saved_pages(root.path(), true).unwrap()[0].id, page.id);
        assert!(!toggle_favorite(root.path(), &page).unwrap());
        assert!(saved_pages(root.path(), true).unwrap().is_empty());
        assert_eq!(
            saved_pages(root.path(), false).unwrap()[0].title,
            page.title
        );

        let markdown = "# Текст\r\n\r\n- 頁\n".as_bytes();
        let filename = page.filename();
        assert!(crate::validate_name(&filename).is_ok());
        assert!(filename.ends_with(".md"));
        let plan =
            crate::copy_out::prepare_named_file(root.path(), &filename, markdown, 1024).unwrap();
        assert_eq!(plan.paths.len(), 1);
        assert_eq!(plan.paths[0].file_name().unwrap(), filename.as_str());
        assert_eq!(std::fs::read(&plan.paths[0]).unwrap(), markdown);
        assert!(plan.text_payload.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn finds_portable_winget_ntn_when_gui_path_is_stale() {
        let root = tempfile::tempdir().unwrap();
        let packages = root.path().join("Packages");
        let test_executable = |package: &str| {
            let dir = packages
                .join(package)
                .join(format!("ntn-{}-pc-windows-msvc", std::env::consts::ARCH));
            std::fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("ntn.exe");
            std::fs::write(&executable, []).unwrap();
            executable
        };
        test_executable("Another.Tool_Microsoft.Winget.Source");
        let roots = [root.path().to_owned()];
        assert_eq!(find_windows_cli([], roots.clone()), None);
        let executable = test_executable("Notion.ntn_Microsoft.Winget.Source");
        assert_eq!(find_windows_cli([], roots), Some(executable));
    }

    #[test]
    fn parses_only_page_results_and_titles_without_a_fixed_property_name() {
        let mut value = json!({
            "id": "12345678-0000-0000-0000-000000000000",
            "object": "page",
            "properties": {"Custom name": {"type":"title", "title":[
                {"plain_text":"中文"}, {"plain_text":" notes"}
            ]}}
        });
        assert_eq!(page_from_result(&value).unwrap().title, "中文 notes");
        value["object"] = json!("data_source");
        assert!(page_from_result(&value).is_none());
        assert!(!page_id("../../v1/users"));
    }

    #[test]
    fn preserves_markdown_and_rejects_truncated_pages() {
        let source = "# Title\n\n- text\n";
        assert_eq!(read_markdown(&json!({"markdown":source})).unwrap(), source);
        assert!(read_markdown(&json!({"markdown":source,"truncated":true})).is_err());
    }

    #[test]
    fn notion_token_is_excluded_from_automatic_capture_and_command_diagnostics() {
        let token = "ntn_synthetic_fixture_0123456789012345";
        assert!(looks_like_token(token));
        assert!(!looks_like_token(
            "a sentence containing ntn_synthetic_fixture_0123456789012345"
        ));
        let command = crate::Command::SetNotionToken {
            token: Some(Token(token.into())),
        };
        assert!(!format!("{command:?}").contains(token));
        assert_eq!(
            crate::diagnostics::summarize_command(&command),
            json!({"type":"set_notion_token"})
        );
    }
}

#[cfg(test)]
mod database_tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    const SOURCE: &str = "11111111-1111-4111-8111-111111111111";
    const PAGE: &str = "22222222-2222-4222-8222-222222222222";
    fn ok() -> Result<()> {
        Ok(())
    }
    #[test]
    fn search_is_bounded_and_only_returns_data_sources() {
        let calls = RefCell::new(Vec::new());
        let result = search_databases_with("", &ok, |path, body| {
            calls.borrow_mut().push((path.to_owned(), body.unwrap()));
            Ok(json!({"results":[
                {"object":"data_source","id":SOURCE,"title":[{"plain_text":"Новости"},{"plain_text":" 🐄"}]},
                {"object":"page","id":PAGE},
                {"object":"data_source","id":"bad","title":[]},
                {"object":"data_source","id":PAGE,"in_trash":true}
            ],"has_more":true}))
        }).unwrap();
        assert_eq!(result["sources"].as_array().unwrap().len(), 1);
        assert_eq!(result["sources"][0]["title"], "Новости 🐄");
        assert_eq!(result["more"], true);
        assert_eq!(calls.borrow()[0].0, "v1/search");
        assert_eq!(calls.borrow()[0].1["filter"]["value"], "data_source");
        assert!(calls.borrow()[0].1.get("query").is_none());
    }
    #[test]
    fn query_uses_schema_title_and_preserves_pagination_and_property_values() {
        let calls = RefCell::new(Vec::new());
        let result = query_database_with(SOURCE, "  cows  ", Some("opaque+cursor"), Some("title"), true, &ok, |path, body| {
            calls.borrow_mut().push((path.to_owned(), body.clone()));
            if body.is_none() {
                return Ok(json!({"object":"data_source","id":SOURCE,"properties":{"Имя":{"type":"title","id":"title"}}}));
            }
            Ok(json!({"results":[{"object":"page","id":PAGE,"created_time":"2026-01-01T00:00:00Z","last_edited_time":"2026-01-02T00:00:00Z","properties":{
                "Имя":{"type":"title","title":[{"plain_text":"Статья"}]},
                "Описание":{"type":"rich_text","rich_text":[{"plain_text":"Кратко"}]},
                "Статус":{"type":"status","status":{"name":"Готово"}},
                "Теги":{"type":"multi_select","multi_select":[{"name":"A"},{"name":"B"}]},
                "Число":{"type":"number","number":42},
                "Да":{"type":"checkbox","checkbox":true},
                "Сайт":{"type":"url","url":"https://example.test"},
                "Дата":{"type":"date","date":{"start":"2026-01-01","end":"2026-01-02"}}
            }},{"object":"data_source","id":SOURCE}],"has_more":true,"next_cursor":"next-token","request_status":{"type":"incomplete"}}))
        }).unwrap();
        assert_eq!(result["rows"].as_array().unwrap().len(), 1);
        assert_eq!(result["rows"][0]["id"], PAGE);
        assert_eq!(result["rows"][0]["title"], "Статья");
        assert_eq!(result["rows"][0]["summary"], "Кратко");
        assert_eq!(result["next_cursor"], "next-token");
        assert_eq!(result["incomplete"], true);
        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        let body = calls[1].1.as_ref().unwrap();
        assert_eq!(body["filter"]["property"], "Имя");
        assert_eq!(body["filter"]["title"]["contains"], "cows");
        assert_eq!(body["sorts"][0]["property"], "Имя");
        assert_eq!(body["sorts"][0]["direction"], "descending");
        assert_eq!(body["start_cursor"], "opaque+cursor");
        assert_eq!(body["page_size"], 30);
    }
    #[test]
    fn default_order_does_not_fetch_schema_and_timestamp_sorts_are_remote() {
        for (sort, expected) in [
            (None, None),
            (Some("edited"), Some("last_edited_time")),
            (Some("created"), Some("created_time")),
        ] {
            let calls = RefCell::new(Vec::new());
            let result = query_database_with(SOURCE, "", None, sort, false, &ok, |path, body| {
                calls.borrow_mut().push((path.to_owned(), body.unwrap()));
                Ok(json!({"results":[],"has_more":false,"next_cursor":null}))
            })
            .unwrap();
            assert!(result["next_cursor"].is_null());
            let calls = calls.borrow();
            assert_eq!(calls.len(), 1);
            if let Some(expected) = expected {
                assert_eq!(calls[0].1["sorts"][0]["timestamp"], expected);
            } else {
                assert!(calls[0].1.get("sorts").is_none());
            }
        }
    }
    #[test]
    fn reject_invalid_inputs_and_cancellation_before_external_work() {
        let calls = Cell::new(0);
        let cancel = || Err(ApiError::new("job_cancelled", "cancelled"));
        let result = query_database_with(SOURCE, "", None, None, false, &cancel, |_, _| {
            calls.set(calls.get() + 1);
            Ok(json!({}))
        });
        assert_eq!(result.unwrap_err().code, "job_cancelled");
        assert_eq!(calls.get(), 0);
        for (id, cursor, sort) in [
            ("bad", None, None),
            (SOURCE, Some(""), None),
            (SOURCE, None, Some("unknown")),
        ] {
            assert!(
                query_database_with(id, "", cursor, sort, false, &ok, |_, _| panic!(
                    "must reject before request"
                ))
                .is_err()
            );
        }
        let result = query_database_with(SOURCE, "", None, None, false, &ok, |_, _| {
            Ok(json!({"results":[],"has_more":true,"next_cursor":null}))
        });
        assert!(result.is_err());
    }
}

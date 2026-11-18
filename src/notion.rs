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

#[derive(Deserialize, Serialize)]
pub struct Page {
    pub id: String,
    pub title: String,
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
    let response = request("v1/search", Some(body), check)?;
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

pub(crate) fn markdown(id: &str, check: impl Fn() -> Result<()>) -> Result<String> {
    if !page_id(id) {
        return Err(ApiError::new("notion_page_id", "Invalid Notion page ID"));
    }
    let response = request(&format!("v1/pages/{id}/markdown"), None, check)?;
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

fn request(path: &str, body: Option<Value>, check: impl Fn() -> Result<()>) -> Result<Value> {
    check()?;
    let token = credentials::read()?.ok_or_else(|| {
        ApiError::new("notion_config", "Connect Notion in Settings → Integrations")
    })?;
    let mut command = Command::new(if cfg!(windows) { "ntn.exe" } else { "ntn" });
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|_| {
        ApiError::new(
            "notion_cli",
            "Install the official Notion CLI (ntn) and restart Katran",
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

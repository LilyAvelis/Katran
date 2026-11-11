//! Read-only folder searches using the optional voidtools ES helper.
//! Adapted from Afiocira's `everything.rs`; see `AFIO-LICENSE`.
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_RESULTS: usize = 100;
const MAX_OUTPUT: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EverythingFolder {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EverythingFolders {
    pub items: Vec<EverythingFolder>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EverythingError {
    pub code: String,
    pub message: String,
}

fn error(code: &str, message: impl Into<String>) -> EverythingError {
    EverythingError {
        code: code.into(),
        message: message.into(),
    }
}

/// Search the existing Everything index. Missing helper/index is an error, not an empty result.
/// Run off the UI thread. Results are index snapshots and must be revalidated on use.
pub fn search_folders(query: &str, limit: usize) -> Result<EverythingFolders, EverythingError> {
    let executable =
        std::env::current_exe().map_err(|e| error("helper_unavailable", e.to_string()))?;
    search_with_runner(query, limit, &executable.with_file_name("es.exe"), run)
}

fn search_with_runner<F>(
    query: &str,
    limit: usize,
    helper: &Path,
    runner: F,
) -> Result<EverythingFolders, EverythingError>
where
    F: FnOnce(&Path, &str, usize) -> Result<Vec<u8>, EverythingError>,
{
    if query.contains('\0')
        || query.encode_utf16().count() > 16_000
        || !(1..=MAX_RESULTS).contains(&limit)
    {
        return Err(error(
            "invalid_argument",
            "Invalid query or result limit (1–100)",
        ));
    }
    if query.trim().is_empty() {
        return Ok(EverythingFolders { items: Vec::new() });
    }
    // Group the user's native expression and request folders from the index.
    // The parser independently rejects files even if the expression changes the filter.
    let expression = format!("folder: <{}>", query);
    parse_results(&runner(helper, &expression, limit)?, limit)
}

fn parse_results(output: &[u8], limit: usize) -> Result<EverythingFolders, EverythingError> {
    if output.len() as u64 > MAX_OUTPUT {
        return Err(error(
            "output_limit",
            "Everything output exceeded its limit",
        ));
    }
    let output = output.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(output);
    let values: Vec<serde_json::Value> = if output.iter().all(u8::is_ascii_whitespace) {
        Vec::new()
    } else {
        serde_json::from_slice(output).map_err(|e| error("invalid_response", e.to_string()))?
    };
    if values.len() > limit {
        return Err(error(
            "invalid_response",
            "Everything exceeded the requested result limit",
        ));
    }
    let mut items = Vec::new();
    for value in values {
        let path = value
            .get("filename")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty() && !v.contains('\0'))
            .ok_or_else(|| {
                error(
                    "invalid_response",
                    "Everything result has no valid filename",
                )
            })?;
        let attributes = value
            .get("attributes")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| error("invalid_response", "Everything result has no attributes"))?;
        if attributes & 16 != 0 {
            items.push(EverythingFolder {
                path: Path::new(path).components().collect(),
            });
        }
    }
    Ok(EverythingFolders { items })
}

fn run(helper: &Path, query: &str, limit: usize) -> Result<Vec<u8>, EverythingError> {
    if !helper.is_file() {
        return Err(error(
            "helper_unavailable",
            "Компонент поиска Everything отсутствует в этой сборке",
        ));
    }
    let mut command = Command::new(helper);
    command
        .args([
            "-argv",
            "-timeout",
            "1500",
            "-no-pause",
            "-no-highlight",
            "-code-page",
            "65001",
            "-json",
            "-n",
        ])
        .arg(limit.to_string())
        .arg("-attributes")
        .arg("--")
        .arg(query)
        .env(
            "APPDATA",
            helper
                .parent()
                .unwrap_or(Path::new("."))
                .join("katran-everything-config"),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| error("helper_unavailable", e.to_string()))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(MAX_OUTPUT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => break Err(error("timeout", "Everything не ответил вовремя")),
            Err(e) => break Err(error("helper_failed", e.to_string())),
        }
    };
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let bytes = reader
        .join()
        .map_err(|_| error("helper_failed", "Everything output reader failed"))?
        .map_err(|e| error("helper_failed", e.to_string()))?;
    let status = status?;
    if !status.success() {
        return Err(match status.code() {
            Some(8) => error("everything_unavailable", "Everything не запущен"),
            Some(7) => error(
                "everything_unavailable",
                "Нужна запущенная полная версия Everything с IPC",
            ),
            code => error("helper_failed", format!("Everything helper exit {code:?}")),
        });
    }
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(error(
            "output_limit",
            "Everything output exceeded its limit",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_files_and_parses_unicode_and_empty() {
        let output = "[{\"filename\":\"C:\\\\папка\\\\\",\"attributes\":16},{\"filename\":\"C:\\\\file\",\"attributes\":32}]";
        let result = parse_results(output.as_bytes(), 2).unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(result.items[0].path.to_string_lossy().contains("папка"));
        assert!(parse_results(b" \r\n", 2).unwrap().items.is_empty());
    }
    #[test]
    fn rejects_bad_data_and_propagates_helper_errors() {
        assert!(parse_results(br#"[{"filename":"x"}]"#, 1).is_err());
        assert!(parse_results(br#"[{},{}]"#, 1).is_err());
        let e = search_with_runner("test", 4, Path::new("unused"), |_, expression, limit| {
            assert_eq!(expression, "folder: <test>");
            assert_eq!(limit, 4);
            Err(error("everything_unavailable", "offline"))
        })
        .unwrap_err();
        assert_eq!(e.code, "everything_unavailable");
        assert_eq!(
            search_with_runner("\0", 1, Path::new("unused"), |_, _, _| unreachable!())
                .unwrap_err()
                .code,
            "invalid_argument"
        );
        assert!(
            search_with_runner(" ", 1, Path::new("unused"), |_, _, _| unreachable!())
                .unwrap()
                .items
                .is_empty()
        );
    }
}

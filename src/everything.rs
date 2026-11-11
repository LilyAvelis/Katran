//! Read-only folder searches using the optional voidtools ES helper.
//! Adapted from Afiocira's `everything.rs`; see `AFIO-LICENSE`.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
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

/// Identify a full path without touching the filesystem or the Everything index.
/// Trims outside whitespace and removes one matched pair of outer double quotes.
pub fn direct_folder_query(query: &str) -> Result<Option<PathBuf>, EverythingError> {
    if query.contains('\0') || query.encode_utf16().count() > 16_000 {
        return Err(error(
            "invalid_argument",
            "Invalid query or result limit (1–100)",
        ));
    }
    let query = query.trim();
    let text = query
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(query);
    let bytes = text.as_bytes();
    let slash = |b| b == b'\\' || b == b'/';
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && slash(bytes[2]);
    let mut unc = false;
    if bytes.len() >= 2 && slash(bytes[0]) && slash(bytes[1]) {
        let rest = &text[2..];
        if let Some(extended) = rest.strip_prefix("?\\").or_else(|| rest.strip_prefix("?/")) {
            let b = extended.as_bytes();
            unc = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && slash(b[2]);
            if b.len() >= 4 && b[..3].eq_ignore_ascii_case(b"UNC") && slash(b[3]) {
                let server_share = &extended[4..];
                unc = server_share
                    .split(['\\', '/'])
                    .filter(|part| !part.is_empty())
                    .count()
                    >= 2;
            }
        } else {
            unc = rest
                .split(['\\', '/'])
                .filter(|part| !part.is_empty())
                .count()
                >= 2;
        }
    }
    let native = cfg!(not(windows)) && Path::new(text).is_absolute();
    Ok((drive || unc || native).then(|| PathBuf::from(text)))
}

fn direct_result(path: &Path) -> Result<EverythingFolders, EverythingError> {
    let path = fs::canonicalize(path)
        .map_err(|e| error("invalid_folder", format!("Папка недоступна: {e}")))?;
    if !path.is_dir() {
        return Err(error("invalid_folder", "Указанный путь не является папкой"));
    }
    Ok(EverythingFolders {
        items: vec![EverythingFolder { path }],
    })
}

/// Search the existing Everything index. Missing helper/index is an error, not an empty result.
/// Run off the UI thread. Results are index snapshots and must be revalidated on use.
pub fn search_folders(query: &str, limit: usize) -> Result<EverythingFolders, EverythingError> {
    if !(1..=MAX_RESULTS).contains(&limit) {
        return Err(error(
            "invalid_argument",
            "Invalid query or result limit (1–100)",
        ));
    }
    if let Some(path) = direct_folder_query(query)? {
        return direct_result(&path);
    }
    if query.trim().is_empty() {
        return Ok(EverythingFolders { items: Vec::new() });
    }
    let executable =
        std::env::current_exe().map_err(|e| error("helper_unavailable", e.to_string()))?;
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| error("helper_unavailable", "LOCALAPPDATA unavailable"))?;
    let helper = resolve_helper(&executable, &PathBuf::from(local))?;
    search_with_runner(query, limit, &helper, run)
}

/// Resolve an installed helper or materialize the verified embedded fallback.
/// The explicit cache root also supports isolated updater validation.
pub fn resolve_helper(executable: &Path, cache_root: &Path) -> Result<PathBuf, EverythingError> {
    let adjacent = executable.with_file_name("es.exe");
    if adjacent.is_file() {
        return Ok(adjacent);
    }
    extract_helper(cache_root, include_bytes!("../assets/everything/es.exe"))
}

// Never accept a link or Windows reparse point as a cache component.
fn check_directory(path: &Path) -> Result<(), EverythingError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| error("helper_unavailable", e.to_string()))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(error(
                "helper_unavailable",
                "Helper cache contains a reparse point",
            ));
        }
    }
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(error(
            "helper_unavailable",
            "Invalid helper cache directory",
        ));
    }
    Ok(())
}
fn extract_helper(base: &Path, payload: &[u8]) -> Result<PathBuf, EverythingError> {
    use sha2::{Digest, Sha256};
    if payload.is_empty() || payload.len() > 4 * 1024 * 1024 {
        return Err(error("helper_unavailable", "Invalid helper size"));
    }
    let digest = format!("{:x}", Sha256::digest(payload));
    check_directory(base)?;
    let mut dir = base.to_path_buf();
    for component in ["Katran", "helpers", digest.as_str()] {
        dir.push(component);
        match fs::create_dir(&dir) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(error("helper_unavailable", e.to_string())),
        }
        check_directory(&dir)?;
    }
    let target = dir.join("es.exe");
    if !target.exists() {
        let temp = dir.join(format!(
            ".es-{}-{:?}.tmp",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| error("helper_unavailable", e.to_string()))?;
        let result = (|| {
            file.write_all(payload)?;
            file.sync_all()?;
            fs::hard_link(&temp, &target)
        })();
        drop(file);
        let _ = fs::remove_file(&temp);
        if let Err(e) = result
            && e.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(error("helper_unavailable", e.to_string()));
        }
    }
    let metadata =
        fs::symlink_metadata(&target).map_err(|e| error("helper_unavailable", e.to_string()))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(error("helper_unavailable", "Helper is a reparse point"));
        }
    }
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != payload.len() as u64
    {
        return Err(error("helper_unavailable", "Invalid cached helper"));
    }
    let bytes = fs::read(&target).map_err(|e| error("helper_unavailable", e.to_string()))?;
    if Sha256::digest(&bytes).as_slice() != Sha256::digest(payload).as_slice() {
        return Err(error("helper_unavailable", "Cached helper hash mismatch"));
    }
    Ok(target)
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
    if let Some(path) = direct_folder_query(query)? {
        return direct_result(&path);
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
    fn literal_directory_never_invokes_the_helper_runner() {
        let folder = tempfile::tempdir().unwrap();
        let query = format!("\"{}\"", folder.path().display());
        let value = search_with_runner(&query, 1, Path::new("unavailable"), |_, _, _| {
            panic!("literal paths bypass the helper")
        })
        .unwrap();
        assert_eq!(value.items[0].path, folder.path().canonicalize().unwrap());
        let absent = folder.path().join("missing");
        let err = search_with_runner(
            &absent.to_string_lossy(),
            1,
            Path::new("unavailable"),
            |_, _, _| panic!("invalid literal paths bypass the helper"),
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid_folder");
    }
    #[test]
    fn embedded_helper_cache_rejects_tampering() {
        let root = tempfile::tempdir().unwrap();
        let path = extract_helper(root.path(), b"synthetic helper").unwrap();
        assert_eq!(
            extract_helper(root.path(), b"synthetic helper").unwrap(),
            path
        );
        fs::write(&path, b"synthetic tamper").unwrap();
        assert!(extract_helper(root.path(), b"synthetic helper").is_err());
        assert!(extract_helper(root.path(), b"").is_err());
    }
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

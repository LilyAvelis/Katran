//! Clipboard provenance. Metadata is evidence supplied by the clipboard, never
//! inferred from the focused window. Full URLs and executable paths are not kept.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_domain: Option<String>,
}

impl Origin {
    /// Presentation helper; the fields remain independently available to filters.
    pub fn label(&self) -> Option<String> {
        match (&self.source_app, &self.source_domain) {
            (Some(app), Some(domain)) => Some(format!("{app} · {domain}")),
            (Some(app), None) => Some(app.clone()),
            (None, Some(domain)) => Some(domain.clone()),
            (None, None) => None,
        }
    }
}

/// Extract only a hostname from CF_HTML's header SourceURL. HTML body links,
/// plain clipboard text, local paths, credentials and URL paths are never stored.
pub fn domain_from_cf_html(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    for line in text.lines().take(64) {
        // The first markup starts the body; a SourceURL inside HTML is not evidence.
        if line.contains('<') {
            break;
        }
        if let Some(value) = line.strip_prefix("SourceURL:") {
            let url = url::Url::parse(value.trim().trim_end_matches('\0')).ok()?;
            if !matches!(url.scheme(), "http" | "https") {
                return None;
            }
            return url.host_str().map(str::to_owned);
        }
    }
    None
}

/// Map the clipboard owner's executable basename to a stable, readable app tag.
/// This function does not inspect processes or the desktop.
pub fn app_from_executable(path: &str) -> Option<String> {
    let basename = path.rsplit(['\\', '/']).next()?.trim();
    let stem = basename
        .strip_suffix(".exe")
        .or_else(|| basename.strip_suffix(".EXE"))
        .unwrap_or(basename);
    if stem.is_empty() {
        return None;
    }
    Some(match stem.to_ascii_lowercase().as_str() {
        "zen" => "Zen".into(),
        "explorer" => "Explorer".into(),
        "telegram" => "Telegram".into(),
        "chrome" => "Chrome".into(),
        "msedge" => "Edge".into(),
        "firefox" => "Firefox".into(),
        "opera" => "Opera".into(),
        "brave" => "Brave".into(),
        _ => stem.to_owned(),
    })
}

/// Read metadata for an already-open clipboard snapshot. The caller MUST hold a
/// `clipboard_win::Clipboard` lock throughout this call and file-list reading.
/// `None` means the sequence changed: discard/retry the entire capture. Missing
/// or inaccessible metadata produces `Some(Origin::default())`, never a guess.
/// If image reading subsequently releases/reopens the clipboard, check the same
/// sequence again before persisting either the image or this metadata.
#[cfg(windows)]
pub fn snapshot_locked(expected_sequence: u32) -> Option<Origin> {
    use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
    if expected_sequence == 0 || unsafe { GetClipboardSequenceNumber() } != expected_sequence {
        return None;
    }
    let source_app = clipboard_owner_app();
    let source_domain = clipboard_win::register_format("HTML Format").and_then(|format| {
        let data: Vec<u8> =
            clipboard_win::get(clipboard_win::formats::RawData(format.get())).ok()?;
        domain_from_cf_html(&data)
    });
    (unsafe { GetClipboardSequenceNumber() } == expected_sequence).then_some(Origin {
        source_app,
        source_domain,
    })
}

#[cfg(windows)]
fn clipboard_owner_app() -> Option<String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            DataExchange::GetClipboardOwner,
            Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
        UI::WindowsAndMessaging::GetWindowThreadProcessId,
    };
    unsafe {
        let owner = GetClipboardOwner();
        if owner.is_null() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(owner, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let success = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length);
        CloseHandle(process);
        if success == 0 {
            return None;
        }
        app_from_executable(&String::from_utf16_lossy(&path[..length as usize]))
    }
}

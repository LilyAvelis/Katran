//! Validated Windows Shell operations for an existing shelf file.
use crate::{ApiError, Event, Result, preview};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Open,
    Reveal,
    OpenWith,
}

pub trait Launcher: Send {
    fn authorize(&self) -> Result<()> {
        Ok(())
    }
    fn launch(&self, path: &Path, operation: Operation) -> Result<()>;
}
pub struct NativeLauncher;

pub(crate) fn execute_link(event: &Event, launcher: &dyn Launcher) -> Result<Value> {
    let url = crate::link_fragments::read_event(event)?;
    launcher.authorize()?;
    launcher.launch(Path::new(&url), Operation::Open)?;
    Ok(
        json!({"id":event.id,"revision":event.revision,"operation":"open","requested":true,"link":true}),
    )
}

/// The caller resolves the event from the catalogue and checks any selection revision.
/// The receipt confirms the Shell request, not that an external app loaded the file.
pub fn execute(event: &Event, operation: Operation, launcher: &dyn Launcher) -> Result<Value> {
    validate(event)?;
    launcher.authorize()?;
    launcher.launch(&event.path, operation)?;
    Ok(json!({"id":event.id,"revision":event.revision,"operation":operation,"requested":true}))
}

pub(crate) fn validate(event: &Event) -> Result<()> {
    if !event.path.is_file() {
        return Err(ApiError::new("file_unavailable", "Source file unavailable"));
    }
    let expected = event
        .revision
        .strip_prefix("ref:")
        .or_else(|| event.revision.strip_prefix("blob:"));
    let matches = if let Some(expected) = expected {
        preview::content_hash(&event.path)
            .map_err(|e| ApiError::new("file_unavailable", e.message))?
            == expected
    } else {
        preview::revision(&event.path, None) == event.revision
    };
    if !matches {
        return Err(ApiError::new(
            "source_changed",
            "Source contents changed since this arrival",
        ));
    }
    Ok(())
}

/// Cached blobs are copied to retained independent snapshots with their original name.
/// Snapshots survive restarts and shelf deletion; quota exhaustion requires explicit cleanup.
pub fn execute_with_cache(
    event: &Event,
    operation: Operation,
    launcher: &dyn Launcher,
    root: &Path,
    quota: u64,
) -> Result<Value> {
    if event.storage != "cached" || operation == Operation::Reveal {
        return execute(event, operation, launcher);
    }
    validate(event)?;
    static PREPARATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = PREPARATION.lock().unwrap_or_else(|e| e.into_inner());
    let plan = crate::copy_out::prepare_with_budget(
        &root.join("open-cache"),
        std::slice::from_ref(event),
        quota,
    )
    .map_err(|e| ApiError::new("open_cache_failed", e))?;
    if let Some(failure) = plan.failures.first() {
        return Err(ApiError::new("open_cache_failed", &failure.error));
    }
    let path = plan
        .paths
        .first()
        .ok_or_else(|| ApiError::new("open_cache_failed", "No snapshot was prepared"))?
        .clone();
    launcher.authorize()?;
    plan.retain_without_retiring();
    drop(guard);
    launcher.launch(&path, operation)?;
    Ok(
        json!({"id":event.id,"revision":event.revision,"operation":operation,"requested":true,"snapshot":true}),
    )
}

#[cfg(not(windows))]
impl Launcher for NativeLauncher {
    fn launch(&self, _: &Path, _: Operation) -> Result<()> {
        Err(ApiError::new(
            "open_failed",
            "Windows Shell is unavailable on this platform",
        ))
    }
}

#[cfg(windows)]
impl Launcher for NativeLauncher {
    fn launch(&self, path: &Path, operation: Operation) -> Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::{
                System::Com::{
                    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                },
                UI::{
                    Shell::{
                        OAIF_EXEC, OPENASINFO, SHOpenFolderAndSelectItems, SHOpenWithDialog,
                        SHParseDisplayName, ShellExecuteW,
                    },
                    WindowsAndMessaging::SW_SHOWNORMAL,
                },
            },
            core::PCWSTR,
        };
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        // Explorer and Shell verbs expect ordinary drive/UNC paths rather than
        // the extended-length prefix produced by Windows canonicalize().
        let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
        let extended: Vec<u16> = r"\\?\".encode_utf16().collect();
        if wide.starts_with(&unc) {
            wide.splice(..unc.len(), [b'\\' as u16, b'\\' as u16]);
        } else if wide.starts_with(&extended)
            && wide.get(extended.len() + 1) == Some(&(b':' as u16))
        {
            wide.drain(..extended.len());
        }
        if wide.contains(&0) {
            return Err(ApiError::new(
                "invalid_argument",
                "Path contains a null character",
            ));
        }
        wide.push(0);
        // Run on a fresh STA thread so Shell extension COM requirements do not depend on the caller.
        std::thread::scope(|scope| {
            scope
                .spawn(move || unsafe {
                    let file = PCWSTR(wide.as_ptr());
                    CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                        .ok()
                        .map_err(|e| ApiError::new("open_failed", e))?;
                    let result = match operation {
                        Operation::Open => {
                            let result = ShellExecuteW(
                                None,
                                PCWSTR::null(),
                                file,
                                PCWSTR::null(),
                                PCWSTR::null(),
                                SW_SHOWNORMAL,
                            );
                            if (result.0 as isize) <= 32 {
                                Err(ApiError::new(
                                    "open_failed",
                                    format!("ShellExecuteW failed ({})", result.0 as isize),
                                ))
                            } else {
                                Ok(())
                            }
                        }
                        Operation::Reveal => {
                            let mut pidl = std::ptr::null_mut();
                            match SHParseDisplayName(file, None, &mut pidl, 0, None) {
                                Ok(()) => {
                                    let result = SHOpenFolderAndSelectItems(pidl, None, 0)
                                        .map_err(|e| ApiError::new("open_failed", e));
                                    CoTaskMemFree(Some(pidl.cast()));
                                    result
                                }
                                Err(e) => Err(ApiError::new("open_failed", e)),
                            }
                        }
                        Operation::OpenWith => {
                            let info = OPENASINFO {
                                pcszFile: file,
                                pcszClass: PCWSTR::null(),
                                oaifInFlags: OAIF_EXEC,
                            };
                            SHOpenWithDialog(None, &info)
                                .map_err(|e| ApiError::new("open_failed", e))
                        }
                    };
                    CoUninitialize();
                    result
                })
                .join()
                .map_err(|_| ApiError::new("open_failed", "Windows Shell worker failed"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Fake(Mutex<Vec<(std::path::PathBuf, Operation)>>);
    impl Launcher for Fake {
        fn launch(&self, path: &Path, op: Operation) -> Result<()> {
            self.0.lock().unwrap().push((path.to_owned(), op));
            Ok(())
        }
    }
    fn event(path: &Path) -> Event {
        Event {
            kind: Default::default(),
            text_preview: None,
            delete_policy: Default::default(),
            revision: format!("ref:{}", preview::content_hash(path).unwrap()),
            origin: Default::default(),
            id: 7,
            detected_at_ms: 0,
            modified_at_ms: None,
            source: "test".into(),
            name: "synthetic.txt".into(),
            storage: "reference".into(),
            path: path.into(),
            size: 3,
            available: true,
        }
    }
    #[test]
    fn validates_before_calling_fake_shell() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Unicode проба & name.txt");
        std::fs::write(&path, b"one").unwrap();
        let event = event(&path);
        let fake = Fake(Mutex::new(vec![]));
        for op in [Operation::Open, Operation::Reveal, Operation::OpenWith] {
            assert_eq!(execute(&event, op, &fake).unwrap()["requested"], true);
        }
        assert_eq!(fake.0.lock().unwrap().len(), 3);
        std::fs::write(&path, b"two").unwrap();
        assert_eq!(
            execute(&event, Operation::Open, &fake).unwrap_err().code,
            "source_changed"
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            execute(&event, Operation::Open, &fake).unwrap_err().code,
            "file_unavailable"
        );
        assert_eq!(fake.0.lock().unwrap().len(), 3);
    }
    #[test]
    fn cached_open_retains_independent_named_snapshot_and_enforces_budget() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("source");
        std::fs::write(&original, b"one").unwrap();
        let hash = preview::content_hash(&original).unwrap();
        let blob = temp.path().join(&hash);
        std::fs::rename(original, &blob).unwrap();
        let mut event = event(&blob);
        event.storage = "cached".into();
        event.revision = format!("blob:{hash}");
        event.name = "image.png".into();
        let fake = Fake(Mutex::new(vec![]));
        execute_with_cache(&event, Operation::Open, &fake, temp.path(), 3).unwrap();
        let snapshot = fake.0.lock().unwrap()[0].0.clone();
        assert_eq!(snapshot.extension().unwrap(), "png");
        std::fs::write(&snapshot, b"changed").unwrap();
        assert_eq!(std::fs::read(&blob).unwrap(), b"one");
        assert_eq!(
            execute_with_cache(&event, Operation::OpenWith, &fake, temp.path(), 3)
                .unwrap_err()
                .code,
            "open_cache_failed"
        );
        execute_with_cache(&event, Operation::Reveal, &fake, temp.path(), 3).unwrap();
        assert_eq!(fake.0.lock().unwrap()[1].0, blob);
    }

    #[test]
    fn cancellation_before_shell_releases_prepared_snapshot() {
        struct Cancelled;
        impl Launcher for Cancelled {
            fn authorize(&self) -> Result<()> {
                Err(ApiError::new("job_cancelled", "Cancelled before Shell"))
            }
            fn launch(&self, _: &Path, _: Operation) -> Result<()> {
                panic!("Cancelled preparation must not reach Shell")
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source.png");
        std::fs::write(&path, b"one").unwrap();
        let mut item = event(&path);
        item.storage = "cached".into();
        item.name = "image.png".into();
        assert_eq!(
            execute_with_cache(&item, Operation::Open, &Cancelled, temp.path(), 100)
                .unwrap_err()
                .code,
            "job_cancelled"
        );
        assert_eq!(
            std::fs::read_dir(temp.path().join("open-cache/exports"))
                .unwrap()
                .count(),
            0
        );
    }
}

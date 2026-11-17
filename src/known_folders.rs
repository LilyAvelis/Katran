//! OS-resolved user folders. Paths are snapshots; validate again when opening.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnownFolder {
    Desktop,
    Downloads,
    Documents,
    Pictures,
    Videos,
    Music,
}

pub const ALL: [KnownFolder; 6] = [
    KnownFolder::Desktop,
    KnownFolder::Downloads,
    KnownFolder::Documents,
    KnownFolder::Pictures,
    KnownFolder::Videos,
    KnownFolder::Music,
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub folder: KnownFolder,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

pub trait Provider: Send {
    fn resolve(&self, folder: KnownFolder) -> std::result::Result<PathBuf, String>;
}

pub struct NativeProvider;

pub fn list(provider: &dyn Provider) -> Vec<Entry> {
    ALL.into_iter()
        .map(|folder| match provider.resolve(folder) {
            Ok(path) => Entry {
                folder,
                path: Some(path),
                error: None,
            },
            Err(error) => Entry {
                folder,
                path: None,
                error: Some(error),
            },
        })
        .collect()
}

#[cfg(windows)]
impl Provider for NativeProvider {
    fn resolve(&self, folder: KnownFolder) -> std::result::Result<PathBuf, String> {
        use std::os::windows::ffi::OsStringExt;
        use windows::Win32::{
            System::Com::CoTaskMemFree,
            UI::Shell::{
                FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music,
                FOLDERID_Pictures, FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
            },
        };
        let id = match folder {
            KnownFolder::Desktop => &FOLDERID_Desktop,
            KnownFolder::Downloads => &FOLDERID_Downloads,
            KnownFolder::Documents => &FOLDERID_Documents,
            KnownFolder::Pictures => &FOLDERID_Pictures,
            KnownFolder::Videos => &FOLDERID_Videos,
            KnownFolder::Music => &FOLDERID_Music,
        };
        unsafe {
            let value =
                SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(|e| e.to_string())?;
            let path = PathBuf::from(std::ffi::OsString::from_wide(value.as_wide()));
            CoTaskMemFree(Some(value.0.cast()));
            Ok(path)
        }
    }
}
#[cfg(not(windows))]
impl Provider for NativeProvider {
    fn resolve(&self, _: KnownFolder) -> std::result::Result<PathBuf, String> {
        Err("Windows Known Folders are unavailable on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    impl Provider for Fake {
        fn resolve(&self, folder: KnownFolder) -> std::result::Result<PathBuf, String> {
            if folder == KnownFolder::Videos {
                Err("unavailable".into())
            } else {
                Ok(PathBuf::from(format!("redirected/{folder:?}")))
            }
        }
    }
    #[test]
    fn preserves_redirected_paths_and_individual_errors() {
        let entries = list(&Fake);
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].path, Some(PathBuf::from("redirected/Desktop")));
        assert_eq!(entries[4].folder, KnownFolder::Videos);
        assert_eq!(entries[4].error.as_deref(), Some("unavailable"));
        assert!(entries[4].path.is_none());
        assert!(entries[5].path.is_some());
    }
}

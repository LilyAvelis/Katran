//! Local emoji AI runtime location and input-integrity contract.
use crate::{ApiError, Result, domain_jobs::Cancellation};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const WORKER: &str = "local-experiments/emoji-ai";
const REQUIRED: &[&str] = &[
    "local-experiments/emoji-ai/worker.py",
    "local-experiments/emoji-ai/semantic_search.py",
    "local-experiments/emoji-ai/model/onnx/model.onnx",
    "local-experiments/emoji-ai/model/onnx/tokenizer.json",
    "local-experiments/emoji-ai/cache-cldr/manifest.json",
    "local-experiments/emoji-ai/cache-cldr/passages.npy",
    "assets/unicode/emoji-test.txt",
    "local-experiments/emoji-ai/cldr/cldr-annotations-full-en.json",
    "local-experiments/emoji-ai/cldr/cldr-annotations-derived-full-en.json",
    "local-experiments/emoji-ai/cldr/cldr-annotations-full-ru.json",
    "local-experiments/emoji-ai/cldr/cldr-annotations-derived-full-ru.json",
];
const OPTIONAL: &[&str] = &[
    "local-experiments/emoji-ai/model/onnx/model.onnx_data",
    "local-experiments/emoji-ai/model/onnx/model.onnx.data",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    protocol: u32,
    files: BTreeMap<String, String>,
}
fn invalid(message: &str) -> ApiError {
    ApiError::new("emoji_ai_unavailable", message)
}
fn missing_model() -> ApiError {
    ApiError::new(
        "model_unavailable",
        "Emoji AI model or runtime is not installed. Installation: https://github.com/LilyAvelis/Katran/blob/main/docs/emoji-ai.md",
    )
}

pub(crate) struct Runtime {
    pub python: PathBuf,
    pub worker: PathBuf,
}
fn selected_root(profile_root: &Path) -> Result<PathBuf> {
    let executable =
        std::env::current_exe().map_err(|_| invalid("Cannot locate Katran executable"))?;
    let adjacent = executable
        .parent()
        .ok_or_else(|| invalid("Cannot locate Katran executable"))?
        .join("katran-e-runtime");
    let dev = if cfg!(debug_assertions)
        || std::env::var_os("KATRAN_E_DEV_RUNTIME").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    } else {
        None
    };
    choose_root(
        std::env::var_os("KATRAN_EMOJI_AI_RUNTIME_ROOT"),
        std::env::var_os("KATRAN_E_RUNTIME_ROOT"),
        &profile_root.join("emoji-ai-runtime"),
        &adjacent,
        dev,
    )
}
/// Cheap availability check; checksums and worker startup are verified on search.
pub(super) fn status(profile_root: &Path) -> Result<()> {
    check_present(&selected_root(profile_root)?)
}
fn check_present(root: &Path) -> Result<()> {
    if !root.is_absolute() {
        return Err(invalid("AI runtime path must be absolute"));
    }
    if !root.join(WORKER).join("katran-e-runtime.json").is_file()
        || REQUIRED.iter().any(|path| !root.join(path).is_file())
    {
        return Err(missing_model());
    }
    #[cfg(windows)]
    let python = root.join(WORKER).join(".venv/Scripts/python.exe");
    #[cfg(not(windows))]
    let python = root.join(WORKER).join(".venv/bin/python");
    if !python.is_file() {
        return Err(missing_model());
    }
    Ok(())
}
pub(crate) fn resolve(profile_root: &Path, cancel: Option<&Cancellation>) -> Result<Runtime> {
    let root = selected_root(profile_root)?;
    check_present(&root)?;
    validate(&root, cancel)
}
fn choose_root(
    explicit: Option<std::ffi::OsString>,
    legacy_explicit: Option<std::ffi::OsString>,
    profile: &Path,
    adjacent: &Path,
    dev: Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(path) = explicit.or(legacy_explicit) {
        if path.is_empty() {
            return Err(invalid("AI runtime path is empty"));
        }
        Ok(PathBuf::from(path))
    } else if profile.exists() {
        Ok(profile.to_path_buf())
    } else if adjacent.exists() {
        Ok(adjacent.to_path_buf())
    } else {
        dev.ok_or_else(missing_model)
    }
}
fn validate(root: &Path, cancel: Option<&Cancellation>) -> Result<Runtime> {
    if !root.is_absolute() {
        return Err(invalid("Emoji AI runtime path must be absolute"));
    }
    let root =
        fs::canonicalize(root).map_err(|_| invalid("Emoji AI runtime directory is unavailable"))?;
    let manifest_path = root.join(WORKER).join("katran-e-runtime.json");
    let manifest_path = fs::canonicalize(manifest_path)
        .map_err(|_| invalid("Emoji AI runtime manifest is missing"))?;
    if !manifest_path.starts_with(&root) {
        return Err(invalid("Emoji AI runtime manifest escapes its directory"));
    }
    let file = fs::File::open(manifest_path)
        .map_err(|_| invalid("Emoji AI runtime manifest is missing"))?;
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("Emoji AI runtime manifest is unreadable"))?;
    if bytes.len() > 16 * 1024 {
        return Err(invalid("Emoji AI runtime manifest is invalid"));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("Emoji AI runtime manifest is invalid"))?;
    if manifest.version != 1
        || manifest.protocol != 1
        || REQUIRED.iter().any(|p| !manifest.files.contains_key(*p))
        || manifest
            .files
            .keys()
            .any(|p| !REQUIRED.contains(&p.as_str()) && !OPTIONAL.contains(&p.as_str()))
        || OPTIONAL
            .iter()
            .any(|p| root.join(p).exists() != manifest.files.contains_key(*p))
    {
        return Err(invalid("Emoji AI runtime manifest is incompatible"));
    }
    for (path, expected) in &manifest.files {
        if expected.len() != 64
            || !expected
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("Emoji AI runtime manifest has an invalid checksum"));
        }
        let actual_path = fs::canonicalize(root.join(path))
            .map_err(|_| invalid("Emoji AI runtime input is missing"))?;
        if !actual_path.starts_with(&root) {
            return Err(invalid("Emoji AI runtime input escapes its directory"));
        }
        let mut file = fs::File::open(actual_path)
            .map_err(|_| invalid("Emoji AI runtime input is unavailable"))?;
        let mut digest = Sha256::new();
        let mut block = [0u8; 64 * 1024];
        loop {
            if let Some(cancel) = cancel {
                cancel.check()?;
            }
            let n = file
                .read(&mut block)
                .map_err(|_| invalid("Emoji AI runtime input could not be read"))?;
            if n == 0 {
                break;
            }
            digest.update(&block[..n]);
        }
        if format!("{:x}", digest.finalize()) != *expected {
            return Err(invalid("Emoji AI runtime checksum mismatch"));
        }
    }
    #[cfg(windows)]
    let python = root.join(WORKER).join(".venv/Scripts/python.exe");
    #[cfg(not(windows))]
    let python = root.join(WORKER).join(".venv/bin/python");
    let python =
        fs::canonicalize(python).map_err(|_| invalid("Emoji AI Python runtime is missing"))?;
    if !python.starts_with(&root) || !python.is_file() {
        return Err(invalid("Emoji AI Python runtime escapes its directory"));
    }
    Ok(Runtime {
        python,
        worker: root.join(WORKER).join("worker.py"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn empty_runtime_reports_model_installation_instruction() {
        let root = tempfile::tempdir().unwrap();
        let error = check_present(root.path()).unwrap_err();
        assert_eq!(error.code, "model_unavailable");
        assert!(error.message.contains("docs/emoji-ai.md"));
    }
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let mut hashes = BTreeMap::new();
        for path in REQUIRED {
            let file = root.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, path.as_bytes()).unwrap();
            hashes.insert(*path, format!("{:x}", Sha256::digest(path.as_bytes())));
        }
        #[cfg(windows)]
        let python = root.path().join(WORKER).join(".venv/Scripts/python.exe");
        #[cfg(not(windows))]
        let python = root.path().join(WORKER).join(".venv/bin/python");
        fs::create_dir_all(python.parent().unwrap()).unwrap();
        fs::write(python, b"synthetic interpreter").unwrap();
        fs::write(
            root.path().join(WORKER).join("katran-e-runtime.json"),
            serde_json::to_vec(&json!({"version":1,"protocol":1,"files":hashes})).unwrap(),
        )
        .unwrap();
        root
    }
    #[test]
    fn selects_new_override_then_legacy_override_then_profile_then_adjacent() {
        let root = fixture();
        let profile = root.path().join("profile");
        let adjacent = root.path().join("adjacent");
        let dev = root.path().join("dev");
        let explicit = root.path().join("explicit");
        let legacy = root.path().join("legacy");
        assert_eq!(
            choose_root(
                Some(explicit.clone().into_os_string()),
                Some(legacy.clone().into_os_string()),
                &profile,
                &adjacent,
                Some(dev.clone())
            )
            .unwrap(),
            explicit
        );
        assert_eq!(
            choose_root(
                None,
                Some(legacy.clone().into_os_string()),
                &profile,
                &adjacent,
                Some(dev.clone())
            )
            .unwrap(),
            legacy
        );
        assert_eq!(
            choose_root(None, None, &profile, &adjacent, Some(dev.clone())).unwrap(),
            dev
        );
        assert!(choose_root(None, None, &profile, &adjacent, None).is_err());
        fs::create_dir(&adjacent).unwrap();
        assert_eq!(
            choose_root(None, None, &profile, &adjacent, Some(dev.clone())).unwrap(),
            adjacent
        );
        fs::create_dir(&profile).unwrap();
        assert_eq!(
            choose_root(None, None, &profile, &adjacent, Some(dev)).unwrap(),
            profile
        );
        assert!(validate(&profile, None).is_err()); // Invalid selection does not fall back.
    }
    #[test]
    fn validates_runtime_and_rejects_missing_tampered_and_incompatible_inputs() {
        let root = fixture();
        assert!(validate(root.path(), None).is_ok());
        fs::write(root.path().join(REQUIRED[0]), b"changed").unwrap();
        assert_eq!(
            validate(root.path(), None).err().unwrap().code,
            "emoji_ai_unavailable"
        );
        fs::write(root.path().join(REQUIRED[0]), REQUIRED[0]).unwrap();
        fs::remove_file(root.path().join(REQUIRED[2])).unwrap();
        assert!(validate(root.path(), None).is_err());
        let manifest = root.path().join(WORKER).join("katran-e-runtime.json");
        fs::write(manifest, br#"{"version":1,"protocol":2,"files":{}}"#).unwrap();
        assert!(validate(root.path(), None).is_err());
        assert!(validate(Path::new("relative"), None).is_err());
    }
    #[test]
    fn rejects_unlisted_external_model_and_oversized_manifest() {
        let root = fixture();
        fs::write(root.path().join(OPTIONAL[0]), b"unlisted").unwrap();
        assert!(validate(root.path(), None).is_err());
        fs::remove_file(root.path().join(OPTIONAL[0])).unwrap();
        fs::write(
            root.path().join(WORKER).join("katran-e-runtime.json"),
            vec![b' '; 16385],
        )
        .unwrap();
        assert!(validate(root.path(), None).is_err());
    }
}

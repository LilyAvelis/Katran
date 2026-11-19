//! Isolated optional CUDA runtime for Folder semantic search.
//! Model assets are resolved separately by the existing verified AI runtime.
use crate::{ApiError, Result};
use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const RUNTIME: &str = "folder-ai-gpu-runtime";
const FALLBACK_RUNTIME: &str = "folder-ai-gpu-runtime-owned";
const HELP: &str = "Folder GPU runtime is unavailable. Install it using scripts/folder-semantic/setup-gpu.ps1, then retry.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    backend: String,
    onnxruntime: String,
}
fn unavailable() -> ApiError {
    ApiError::new("folder_gpu_unavailable", HELP)
}

/// Resolve only the isolated interpreter. Its CUDA provider must still be
/// loaded and verified by the worker; this check does not claim GPU readiness.
pub(crate) fn resolve(profile_root: &Path) -> Result<PathBuf> {
    if !profile_root.is_absolute() {
        return Err(unavailable());
    }
    let runtime = fs::canonicalize(runtime_path(profile_root)).map_err(|_| unavailable())?;
    let profile = fs::canonicalize(profile_root).map_err(|_| unavailable())?;
    if !runtime.starts_with(&profile) {
        return Err(unavailable());
    }
    let manifest = fs::canonicalize(runtime.join("manifest.json")).map_err(|_| unavailable())?;
    if !manifest.starts_with(&runtime) {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    fs::File::open(manifest)
        .map_err(|_| unavailable())?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > 4096 {
        return Err(unavailable());
    }
    let value: Manifest = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if value.version != 1 || value.backend != "onnxruntime-cuda" || value.onnxruntime != "1.23.2" {
        return Err(unavailable());
    }
    #[cfg(windows)]
    let interpreter = runtime.join(".venv/Scripts/python.exe");
    #[cfg(not(windows))]
    let interpreter = runtime.join(".venv/bin/python");
    let interpreter = fs::canonicalize(interpreter).map_err(|_| unavailable())?;
    if !interpreter.starts_with(&runtime) || !interpreter.is_file() {
        return Err(unavailable());
    }
    Ok(interpreter)
}

pub(crate) fn runtime_path(profile_root: &Path) -> PathBuf {
    let primary = profile_root.join(RUNTIME);
    let fallback = profile_root.join(FALLBACK_RUNTIME);
    if !primary.join("manifest.json").is_file() && fallback.join("manifest.json").is_file() {
        fallback
    } else {
        primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_or_incompatible_runtime_fails_explicitly() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve(root.path()).unwrap_err().code,
            "folder_gpu_unavailable"
        );
        let base = root.path().join(RUNTIME);
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join("manifest.json"),
            r#"{"version":1,"backend":"onnxruntime-cuda","onnxruntime":"wrong"}"#,
        )
        .unwrap();
        assert_eq!(
            resolve(root.path()).unwrap_err().code,
            "folder_gpu_unavailable"
        );
        fs::write(
            base.join("manifest.json"),
            r#"{"version":1,"backend":"onnxruntime-cuda","onnxruntime":"1.23.2"}"#,
        )
        .unwrap();
        assert_eq!(
            resolve(root.path()).unwrap_err().code,
            "folder_gpu_unavailable"
        );
        #[cfg(windows)]
        let py = base.join(".venv/Scripts/python.exe");
        #[cfg(not(windows))]
        let py = base.join(".venv/bin/python");
        fs::create_dir_all(py.parent().unwrap()).unwrap();
        fs::write(&py, b"synthetic").unwrap();
        assert_eq!(resolve(root.path()).unwrap(), fs::canonicalize(py).unwrap());
    }

    #[test]
    fn fallback_preserves_incomplete_primary_and_complete_primary_has_priority() {
        let root = tempfile::tempdir().unwrap();
        let primary = root.path().join(RUNTIME);
        let fallback = root.path().join(FALLBACK_RUNTIME);
        fs::create_dir_all(&primary).unwrap();
        fs::write(primary.join("foreign"), b"unchanged").unwrap();
        #[cfg(windows)]
        let py = fallback.join(".venv/Scripts/python.exe");
        #[cfg(not(windows))]
        let py = fallback.join(".venv/bin/python");
        fs::create_dir_all(py.parent().unwrap()).unwrap();
        fs::write(&py, b"synthetic").unwrap();
        fs::write(
            fallback.join("manifest.json"),
            r#"{"version":1,"backend":"onnxruntime-cuda","onnxruntime":"1.23.2"}"#,
        )
        .unwrap();
        assert_eq!(
            resolve(root.path()).unwrap(),
            fs::canonicalize(&py).unwrap()
        );
        assert_eq!(fs::read(primary.join("foreign")).unwrap(), b"unchanged");
        fs::write(primary.join("manifest.json"), b"{}").unwrap();
        assert_eq!(runtime_path(root.path()), primary);
        assert!(resolve(root.path()).is_err());
    }
}

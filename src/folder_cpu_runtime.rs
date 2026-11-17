//! Independent Folder AI model/runtime integrity boundary.
use crate::{ApiError, Result, domain_jobs::Cancellation};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const RUNTIME: &str = "folder-ai-runtime";
const MODEL_HASH: &str = "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665";
const TOKENIZER_HASH: &str = "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39";
const HELP: &str = "Folder AI runtime is incomplete or invalid. Repair it from Settings → AI.";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    backend: String,
    onnxruntime: String,
}
pub(crate) struct Runtime {
    pub python: PathBuf,
    pub model: PathBuf,
    pub tokenizer: PathBuf,
}
fn invalid() -> ApiError {
    ApiError::new("folder_ai_runtime_invalid", HELP)
}
fn contained_file(root: &Path, path: &str) -> Result<PathBuf> {
    let file = fs::canonicalize(root.join(path)).map_err(|_| invalid())?;
    if !file.starts_with(root) || !file.is_file() {
        return Err(invalid());
    }
    Ok(file)
}
fn check_hash(path: &Path, expected: &str, cancel: Option<&Cancellation>) -> Result<()> {
    let mut file = fs::File::open(path).map_err(|_| invalid())?;
    let mut digest = Sha256::new();
    let mut block = [0u8; 64 * 1024];
    loop {
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        let n = file.read(&mut block).map_err(|_| invalid())?;
        if n == 0 {
            break;
        }
        digest.update(&block[..n]);
    }
    if format!("{:x}", digest.finalize()) != expected {
        return Err(invalid());
    }
    Ok(())
}
pub(crate) fn resolve(profile: &Path, cancel: Option<&Cancellation>) -> Result<Option<Runtime>> {
    resolve_with_hashes(profile, cancel, MODEL_HASH, TOKENIZER_HASH)
}
fn resolve_with_hashes(
    profile: &Path,
    cancel: Option<&Cancellation>,
    model_hash: &str,
    tokenizer_hash: &str,
) -> Result<Option<Runtime>> {
    if !profile.is_absolute() {
        return Err(invalid());
    }
    let candidate = profile.join(RUNTIME);
    match fs::symlink_metadata(&candidate) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid()),
    }
    let profile = fs::canonicalize(profile).map_err(|_| invalid())?;
    let root = fs::canonicalize(candidate).map_err(|_| invalid())?;
    if !root.starts_with(&profile) || !root.is_dir() {
        return Err(invalid());
    }
    let manifest = contained_file(&root, "manifest.json")?;
    let mut bytes = Vec::new();
    fs::File::open(manifest)
        .map_err(|_| invalid())?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > 4096 {
        return Err(invalid());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if manifest.version != 1
        || manifest.backend != "onnxruntime-cpu"
        || manifest.onnxruntime != "1.23.2"
    {
        return Err(invalid());
    }
    #[cfg(windows)]
    let python = contained_file(&root, ".venv/Scripts/python.exe")?;
    #[cfg(not(windows))]
    let python = contained_file(&root, ".venv/bin/python")?;
    let model = contained_file(&root, "model/model.onnx")?;
    let tokenizer = contained_file(&root, "model/tokenizer.json")?;
    check_hash(&model, model_hash, cancel)?;
    check_hash(&tokenizer, tokenizer_hash, cancel)?;
    Ok(Some(Runtime {
        python,
        model,
        tokenizer,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(RUNTIME);
        fs::create_dir_all(root.join("model")).unwrap();
        #[cfg(windows)]
        let py = root.join(".venv/Scripts/python.exe");
        #[cfg(not(windows))]
        let py = root.join(".venv/bin/python");
        fs::create_dir_all(py.parent().unwrap()).unwrap();
        fs::write(py, b"fake interpreter").unwrap();
        fs::write(root.join("model/model.onnx"), b"fake model").unwrap();
        fs::write(root.join("model/tokenizer.json"), b"fake tokenizer").unwrap();
        fs::write(
            root.join("manifest.json"),
            r#"{"version":1,"backend":"onnxruntime-cpu","onnxruntime":"1.23.2"}"#,
        )
        .unwrap();
        (
            temp,
            format!("{:x}", Sha256::digest(b"fake model")),
            format!("{:x}", Sha256::digest(b"fake tokenizer")),
        )
    }
    #[test]
    fn absent_is_only_fallback_case() {
        let t = tempfile::tempdir().unwrap();
        assert!(resolve(t.path(), None).unwrap().is_none());
        fs::create_dir(t.path().join(RUNTIME)).unwrap();
        assert_eq!(
            resolve(t.path(), None).err().unwrap().code,
            "folder_ai_runtime_invalid"
        );
    }
    #[test]
    fn valid_synthetic_manifest_and_integrity_fail_closed() {
        let (t, m, k) = fixture();
        let runtime = resolve_with_hashes(t.path(), None, &m, &k)
            .unwrap()
            .unwrap();
        assert!(runtime.python.is_file() && runtime.model.is_file() && runtime.tokenizer.is_file());
        fs::write(t.path().join(RUNTIME).join("model/model.onnx"), b"changed").unwrap();
        assert_eq!(
            resolve_with_hashes(t.path(), None, &m, &k)
                .err()
                .unwrap()
                .code,
            "folder_ai_runtime_invalid"
        );
        fs::write(
            t.path().join(RUNTIME).join("manifest.json"),
            vec![b'x'; 4097],
        )
        .unwrap();
        assert!(resolve_with_hashes(t.path(), None, &m, &k).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;
        let (t, m, k) = fixture();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("escape");
        fs::write(&target, b"fake tokenizer").unwrap();
        let file = t.path().join(RUNTIME).join("model/tokenizer.json");
        fs::remove_file(&file).unwrap();
        symlink(target, file).unwrap();
        assert!(resolve_with_hashes(t.path(), None, &m, &k).is_err());
    }
}

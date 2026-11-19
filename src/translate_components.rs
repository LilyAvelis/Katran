//! Metadata-only discovery for local translation components. No subprocess or download.
//!
//! `KATRAN_TRANSLATE_ROOT` may point at an existing absolute OCR root containing
//! `venv`, `paddlex/official_models`, and `manga-model`. When unset, the active
//! profile's `translate-runtime` is checked. A local developer checkout is used
//! only when the current executable matches its immutable runtime manifest.
//! Managed vision Python is preferred, with portable Python as a fallback.
//! `KATRAN_OLLAMA_EXE` can name an existing Ollama executable. `OLLAMA_MODELS`
//! selects Ollama's model store. File presence is not an inference test.
use crate::{ApiError, ImageTranslationSource, Result};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub struct Status {
    pub paddle_python: Option<PathBuf>,
    pub paddle_models: Option<PathBuf>,
    pub manga_python: Option<PathBuf>,
    pub manga_model: Option<PathBuf>,
    pub chinese_model: bool,
    pub korean_model: bool,
    pub ollama_exe: Option<PathBuf>,
    pub ollama_models: Option<PathBuf>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Components {
    pub paddle_python: PathBuf,
    pub paddle_models: PathBuf,
    pub manga_python: PathBuf,
    pub manga_model: Option<PathBuf>,
    pub ollama_exe: PathBuf,
    pub ollama_models: PathBuf,
}
fn file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0)
}
fn complete(root: &Path, names: &[&str]) -> bool {
    names.iter().all(|name| file(&root.join(name)))
}
fn env_path(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name)?);
    path.is_absolute().then_some(path)
}
fn local_checkout_probe() -> Option<PathBuf> {
    use sha2::{Digest, Sha256};
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let version = exe.parent()?;
    let id = version.file_name()?.to_str()?;
    if id.len() != 12 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let versions = version.parent()?;
    if versions.file_name()? != "versions" {
        return None;
    }
    let runtime = versions.parent()?;
    if runtime.file_name()? != "runtime" {
        return None;
    }
    let project = runtime.parent()?;
    if !project.join("Cargo.toml").is_file() {
        return None;
    }
    let manifest = fs::File::open(version.join("build-manifest.json")).ok()?;
    let mut bytes = Vec::new();
    manifest
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let expected = manifest["executable_sha256"].as_str()?;
    if expected.len() != 64 || !expected.starts_with(id) {
        return None;
    }
    let mut input = fs::File::open(&exe).ok()?;
    if input.metadata().ok()?.len() > 512 * 1024 * 1024 {
        return None;
    }
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = input.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    if format!("{:x}", hasher.finalize()) != expected {
        return None;
    }
    Some(project.join("target/build/translate-probe/ocr"))
}
pub(crate) fn ocr_root(profile: &Path) -> Result<PathBuf> {
    if std::env::var_os("KATRAN_TRANSLATE_ROOT").is_some() {
        return env_path("KATRAN_TRANSLATE_ROOT").ok_or_else(|| {
            ApiError::new(
                "invalid_translate_root",
                "KATRAN_TRANSLATE_ROOT must be an absolute path",
            )
        });
    }
    let root = profile.join("translate-runtime");
    if root.is_dir() {
        return Ok(root);
    }
    // Cache hash verification; it applies to this process's immutable executable.
    static PROBE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    Ok(PROBE
        .get_or_init(local_checkout_probe)
        .clone()
        .unwrap_or(root))
}
fn ollama_executable(profile: &Path) -> Option<PathBuf> {
    if std::env::var_os("KATRAN_OLLAMA_EXE").is_some() {
        let path = env_path("KATRAN_OLLAMA_EXE")?;
        return file(&path).then_some(path);
    }
    let private = profile.join("translate-ollama-runtime/ollama.exe");
    if file(&private) {
        return Some(private);
    }
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let path = PathBuf::from(local).join("Programs/Ollama/ollama.exe");
        if file(&path) {
            return Some(path);
        }
    }
    let name = if cfg!(windows) {
        "ollama.exe"
    } else {
        "ollama"
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .take(64)
        .filter(|path| path.is_absolute())
        .map(|path| path.join(name))
        .find(|path| file(path))
}
fn tag_manifest(tag: &str) -> Option<PathBuf> {
    if tag.len() > 128 {
        return None;
    }
    let (name, version) = tag.split_once(':').unwrap_or((tag, "latest"));
    fn valid(s: &str) -> bool {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    }
    if !valid(name) || !valid(version) {
        return None;
    }
    Some(
        PathBuf::from("manifests/registry.ollama.ai/library")
            .join(name)
            .join(version),
    )
}
fn find_ollama_models(profile: &Path, manifest: &Path) -> Option<PathBuf> {
    let default = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|home| PathBuf::from(home).join(".ollama/models"));
    [
        Some(profile.join("translate-ollama")),
        env_path("OLLAMA_MODELS"),
        default,
    ]
    .into_iter()
    .flatten()
    .find(|root| file(&root.join(manifest)))
}
/// Report local files only. A model manifest is not evidence of valid blobs or inference.
pub fn discover(profile: &Path, ollama_tag: &str) -> Result<Status> {
    let manifest = tag_manifest(ollama_tag).ok_or_else(|| {
        ApiError::new("invalid_translate_model", "Invalid local Ollama model tag")
    })?;
    let root = ocr_root(profile)?;
    let venv = root.join("venv");
    let python = venv.join("Scripts/python.exe");
    let paddle_python = if venv.join("katran-setup").exists() {
        crate::upscale_components::managed_python(&venv)
    } else {
        file(&python).then_some(python)
    };
    let models = root.join("paddlex/official_models");
    let weights = ["inference.json", "inference.pdiparams", "inference.yml"];
    let paddle_ok = complete(&models.join("PP-OCRv5_server_det"), &weights);
    let manga = root.join("manga-model");
    let manga_ok = complete(
        &manga,
        &[
            "config.json",
            "preprocessor_config.json",
            "pytorch_model.bin",
            "vocab.txt",
            "special_tokens_map.json",
            "tokenizer_config.json",
        ],
    );
    Ok(Status {
        paddle_python,
        chinese_model: complete(&models.join("PP-OCRv5_server_rec"), &weights),
        korean_model: complete(&models.join("korean_PP-OCRv5_mobile_rec"), &weights),
        paddle_models: paddle_ok.then_some(models),
        manga_python: crate::upscale_components::managed_python(
            &profile.join("translate-vision-runtime/venv"),
        )
        .or_else(crate::upscale_components::portable_python),
        manga_model: manga_ok.then_some(manga),
        ollama_exe: ollama_executable(profile),
        ollama_models: find_ollama_models(profile, &manifest),
    })
}
/// Resolve all files for a local page job; no component is installed implicitly.
impl Status {
    pub(crate) fn resolve(&self, source: ImageTranslationSource) -> Result<Components> {
        let missing = |message| ApiError::new("translate_component_missing", message);
        match source {
            ImageTranslationSource::Japanese if self.manga_model.is_none() => {
                return Err(missing("Manga OCR model is missing"));
            }
            ImageTranslationSource::Chinese | ImageTranslationSource::TraditionalChinese
                if !self.chinese_model =>
            {
                return Err(missing("Chinese OCR model PP-OCRv5_server_rec is missing"));
            }
            ImageTranslationSource::Korean if !self.korean_model => {
                return Err(missing(
                    "Korean OCR model korean_PP-OCRv5_mobile_rec is missing",
                ));
            }
            _ => {}
        }
        Ok(Components {
        paddle_python: self
            .paddle_python.clone()
            .ok_or_else(|| missing("Paddle OCR Python is missing"))?,
        paddle_models: self
            .paddle_models.clone()
            .ok_or_else(|| missing("Paddle OCR models are missing"))?,
        manga_python: self
            .manga_python.clone()
            .ok_or_else(|| missing("Translation image runtime is missing. Open Settings → AI."))?,
        manga_model: self.manga_model.clone(),
        ollama_exe: self
            .ollama_exe.clone()
            .ok_or_else(|| missing("Ollama executable is missing"))?,
        ollama_models: self.ollama_models.clone().ok_or_else(|| {
            ApiError::new("translate_component_missing", format!("Ollama model {MODEL} was not found in the Katran model store or the configured Ollama store"))
        })?,
    })
    }
}
pub(crate) const MODEL: &str = "qwen3:4b-instruct";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn required_files_are_nonempty_and_complete() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!complete(dir.path(), &["a", "b"]));
        fs::write(dir.path().join("a"), b"a").unwrap();
        fs::write(dir.path().join("b"), b"").unwrap();
        assert!(!complete(dir.path(), &["a", "b"]));
        fs::write(dir.path().join("b"), b"b").unwrap();
        assert!(complete(dir.path(), &["a", "b"]));
    }
    #[test]
    fn rejects_path_and_registry_in_tags() {
        for tag in ["../bad:latest", "registry/x:tag", "x:../../y", "x:", ""] {
            assert!(tag_manifest(tag).is_none(), "{tag}");
        }
    }
    #[test]
    fn profile_model_store_wins_over_inherited_ollama_configuration() {
        let profile = tempfile::tempdir().unwrap();
        let store = profile.path().join("translate-ollama");
        let manifest = tag_manifest(MODEL).unwrap();
        let destination = store.join(&manifest);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"fixture").unwrap();
        assert_eq!(find_ollama_models(profile.path(), &manifest), Some(store));
    }
}

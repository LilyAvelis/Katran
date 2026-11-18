//! Bounded discovery of manually installed image-upscale components.
//!
//! `KATRAN_COMFYUI_ROOT` may name a portable ComfyUI directory containing
//! `ComfyUI/extra_model_paths.yaml` and `python_embeded/python.exe`. When unset,
//! Windows also checks the conventional portable location on the system drive.
//! This module never downloads, executes Python, scans drives, or verifies model
//! bytes. Discovery is metadata only; inference requires a separate verification.
use crate::{ApiError, Result};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MODELS: [&str; 3] = [
    "4x_foolhardy_Remacri.pth",
    "RealESRGAN_x2plus.pth",
    "RealESRGAN_x4.pth",
];
const CONFIG_LIMIT: u64 = 64 * 1024;
const ROOT_LIMIT: usize = 32;

fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file() && m.len() > 0)
}

fn portable_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("KATRAN_COMFYUI_ROOT") {
        let path = PathBuf::from(root);
        return path.is_absolute().then_some(path);
    }
    #[cfg(windows)]
    {
        let drive = std::env::var_os("SystemDrive")?;
        let mut root = PathBuf::from(drive);
        root.push("\\");
        Some(root.join("ComfyUI").join("ComfyUI_windows_portable"))
    }
    #[cfg(not(windows))]
    None
}

/// Returns an existing portable Python executable, without claiming it is usable.
pub fn resolve_python() -> Option<PathBuf> {
    let path = portable_root()?.join("python_embeded").join("python.exe");
    regular(&path).then_some(path)
}

/// File presence only: neither value proves the runtime/model is functional.
pub struct Discovery {
    pub python: Option<PathBuf>,
    pub model: Option<PathBuf>,
}
pub struct Resolved {
    pub python: PathBuf,
    pub model: PathBuf,
}
pub fn discover(profile: &Path, filename: &str) -> Result<Discovery> {
    if !MODELS.contains(&filename) {
        return Err(ApiError::new(
            "invalid_upscale_model",
            "Unknown upscale model",
        ));
    }
    let mut roots = vec![profile.join("ai-models").join("upscale_models")];
    if let Some(portable) = portable_root() {
        let app = portable.join("ComfyUI");
        roots.push(app.join("models").join("upscale_models"));
        roots.extend(config_roots(&app.join("extra_model_paths.yaml"), &app));
    }
    let model = roots
        .into_iter()
        .take(ROOT_LIMIT)
        .map(|root| root.join(filename))
        .find(|path| regular(path));
    Ok(Discovery {
        python: resolve_python(),
        model,
    })
}
/// Resolve both files or return a distinct, actionable missing-component error.
pub fn resolve(profile: &Path, filename: &str) -> Result<Resolved> {
    let found = discover(profile, filename)?;
    let model = found.model.ok_or_else(|| {
        ApiError::new(
            "upscale_model_missing",
            format!("Upscale model {filename} was not found"),
        )
    })?;
    let python = found.python.ok_or_else(|| {
        ApiError::new(
            "upscale_runtime_missing",
            "ComfyUI portable Python was not found",
        )
    })?;
    Ok(Resolved { python, model })
}

// Deliberately supports plain/quoted scalar mapping values and the common YAML
// block scalar for path lists, rather than interpreting arbitrary YAML features.
// Unsupported constructs are ignored, so they cannot expand a discovery scope.
fn scalar(input: &str) -> Option<&str> {
    let text = input.trim();
    if text.is_empty()
        || matches!(
            text.chars().next(),
            Some('#' | '&' | '*' | '!' | '[' | '{' | '|' | '>')
        )
    {
        return None;
    }
    if matches!(text.chars().next(), Some('\'' | '"')) {
        let quote = text.as_bytes()[0] as char;
        let rest = text.strip_prefix(quote)?;
        let end = rest.find(quote)?;
        if !rest[end + 1..].trim().is_empty() && !rest[end + 1..].trim().starts_with('#') {
            return None;
        }
        let value = &rest[..end];
        // Escaping is deliberately unsupported rather than misinterpreted.
        return (!value.contains('\\') && !value.contains(quote)).then_some(value);
    }
    let value = text.split(" #").next()?.trim();
    (!value.is_empty() && !value.contains(&['{', '}', '[', ']'][..])).then_some(value)
}

#[derive(Default)]
struct Section {
    base: Option<PathBuf>,
    paths: Vec<PathBuf>,
}
fn add_section(section: &mut Section, app: &Path, out: &mut Vec<PathBuf>) {
    let base = section.base.take().unwrap_or_else(|| app.to_owned());
    let base = if base.is_absolute() {
        base
    } else {
        app.join(base)
    };
    for path in section.paths.drain(..) {
        if out.len() >= ROOT_LIMIT {
            break;
        }
        out.push(if path.is_absolute() {
            path
        } else {
            base.join(path)
        });
    }
}
fn config_roots(path: &Path, app: &Path) -> Vec<PathBuf> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut data = Vec::new();
    if file.take(CONFIG_LIMIT + 1).read_to_end(&mut data).is_err()
        || data.len() as u64 > CONFIG_LIMIT
    {
        return Vec::new();
    }
    let Ok(text) = std::str::from_utf8(&data) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut section = Section::default();
    let mut block_indent = None;
    for line in text.lines().take(4096) {
        if line.contains('\t') {
            return Vec::new();
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            add_section(&mut section, app, &mut out);
            block_indent = None;
            continue;
        }
        if let Some(parent_indent) = block_indent {
            if indent > parent_indent {
                if let Some(value) = scalar(trimmed) {
                    section.paths.push(PathBuf::from(value));
                }
                continue;
            }
            block_indent = None;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        match key.trim() {
            "base_path" => section.base = scalar(value).map(PathBuf::from),
            "upscale_models" => {
                if matches!(value.trim(), "|" | "|-" | "|+") {
                    block_indent = Some(indent);
                } else if let Some(value) = scalar(value) {
                    section.paths.push(PathBuf::from(value));
                }
            }
            _ => {}
        }
        if section.paths.len() >= ROOT_LIMIT {
            break;
        }
    }
    add_section(&mut section, app, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_reads_only_explicit_upscale_paths() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("extra_model_paths.yaml");
        fs::write(&config, "local:\n  base_path: models\n  checkpoints: ignored\n  upscale_models: |\n    upscale\n    second\nother:\n  upscale_models: other\n").unwrap();
        assert_eq!(
            config_roots(&config, dir.path()),
            vec![
                dir.path().join("models/upscale"),
                dir.path().join("models/second"),
                dir.path().join("other")
            ]
        );
    }
    #[test]
    fn rejects_unknown_and_finds_profile_model() {
        let dir = tempfile::tempdir().unwrap();
        assert!(discover(dir.path(), "../other.pth").is_err());
        let models = dir.path().join("ai-models/upscale_models");
        fs::create_dir_all(&models).unwrap();
        let path = models.join(MODELS[0]);
        fs::write(&path, b"fixture").unwrap();
        assert_eq!(discover(dir.path(), MODELS[0]).unwrap().model, Some(path));
    }
    #[test]
    fn oversized_and_unsupported_config_are_not_interpreted() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("extra_model_paths.yaml");
        fs::write(&config, vec![b'x'; CONFIG_LIMIT as usize + 1]).unwrap();
        assert!(config_roots(&config, dir.path()).is_empty());
        assert_eq!(scalar("[a, b]"), None);
        assert_eq!(scalar("*alias"), None);
    }
}

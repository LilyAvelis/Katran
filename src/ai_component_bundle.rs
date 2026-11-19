//! Version-bound setup resources travel with the executable, including updates.
use crate::{ApiError, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
const FILES: &[(&str, &[u8])] = &[
    (
        "scripts/ai-components/terminal.ps1",
        include_bytes!("../scripts/ai-components/terminal.ps1"),
    ),
    (
        "scripts/ai-components/ensure-python.ps1",
        include_bytes!("../scripts/ai-components/ensure-python.ps1"),
    ),
    (
        "scripts/ai-components/verify.py",
        include_bytes!("../scripts/ai-components/verify.py"),
    ),
    (
        "scripts/ai-components/image-setup.py",
        include_bytes!("../scripts/ai-components/image-setup.py"),
    ),
    (
        "scripts/image-upscale/worker.py",
        include_bytes!("../scripts/image-upscale/worker.py"),
    ),
    (
        "scripts/image-translate/ocr_page.py",
        include_bytes!("../scripts/image-translate/ocr_page.py"),
    ),
    (
        "scripts/image-translate/manga_recognize.py",
        include_bytes!("../scripts/image-translate/manga_recognize.py"),
    ),
    (
        "scripts/image-translate/translate_page.py",
        include_bytes!("../scripts/image-translate/translate_page.py"),
    ),
    (
        "scripts/folder-semantic/setup.ps1",
        include_bytes!("../scripts/folder-semantic/setup.ps1"),
    ),
    (
        "scripts/folder-semantic/setup-gpu.ps1",
        include_bytes!("../scripts/folder-semantic/setup-gpu.ps1"),
    ),
    (
        "scripts/folder-semantic/worker.py",
        include_bytes!("../scripts/folder-semantic/worker.py"),
    ),
    (
        "scripts/install-emoji-ai-model.ps1",
        include_bytes!("../scripts/install-emoji-ai-model.ps1"),
    ),
    (
        "scripts/create-emoji-ai-runtime-manifest.py",
        include_bytes!("../scripts/create-emoji-ai-runtime-manifest.py"),
    ),
    (
        "scripts/emoji-ai/worker.py",
        include_bytes!("../scripts/emoji-ai/worker.py"),
    ),
    (
        "scripts/emoji-ai/semantic_search.py",
        include_bytes!("../scripts/emoji-ai/semantic_search.py"),
    ),
    (
        "assets/unicode/emoji-test.txt",
        include_bytes!("../assets/unicode/emoji-test.txt"),
    ),
    (
        "assets/emoji-ai/cldr/LICENSE",
        include_bytes!("../assets/emoji-ai/cldr/LICENSE"),
    ),
    (
        "assets/emoji-ai/cldr/cldr-annotations-full-en.json",
        include_bytes!("../assets/emoji-ai/cldr/cldr-annotations-full-en.json"),
    ),
    (
        "assets/emoji-ai/cldr/cldr-annotations-derived-full-en.json",
        include_bytes!("../assets/emoji-ai/cldr/cldr-annotations-derived-full-en.json"),
    ),
    (
        "assets/emoji-ai/cldr/cldr-annotations-full-ru.json",
        include_bytes!("../assets/emoji-ai/cldr/cldr-annotations-full-ru.json"),
    ),
    (
        "assets/emoji-ai/cldr/cldr-annotations-derived-full-ru.json",
        include_bytes!("../assets/emoji-ai/cldr/cldr-annotations-derived-full-ru.json"),
    ),
];
pub(crate) fn materialize(profile: &Path) -> Result<PathBuf> {
    let profile = fs::canonicalize(profile)?;
    let mut digest = Sha256::new();
    for (name, bytes) in FILES {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(Sha256::digest(bytes));
    }
    let directory = profile
        .join("ai-installers")
        .join(format!("{:x}", digest.finalize()));
    fs::create_dir_all(&directory)?;
    let directory = fs::canonicalize(directory)?;
    if !directory.starts_with(&profile) {
        return Err(ApiError::new(
            "ai_bundle_invalid",
            "Installer bundle escapes the profile",
        ));
    }
    for (name, bytes) in FILES {
        let file = directory.join(name);
        let parent = file.parent().expect("fixed bundle path");
        fs::create_dir_all(parent)?;
        if !fs::canonicalize(parent)?.starts_with(&directory) {
            return Err(ApiError::new(
                "ai_bundle_invalid",
                "Installer directory escapes the bundle",
            ));
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
        {
            Ok(mut output) => output.write_all(bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        if !fs::canonicalize(&file)?.starts_with(&directory)
            || Sha256::digest(fs::read(file)?) != Sha256::digest(bytes)
        {
            return Err(ApiError::new(
                "ai_bundle_invalid",
                "Installer files were changed; use a new clean bundle or inspect the log",
            ));
        }
    }
    Ok(directory)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundle_is_version_bound_and_refuses_modified_helpers() {
        let root = tempfile::tempdir().unwrap();
        let path = materialize(root.path()).unwrap();
        assert_eq!(path, materialize(root.path()).unwrap());
        assert!(path.join("assets/unicode/emoji-test.txt").is_file());
        fs::write(path.join("scripts/ai-components/terminal.ps1"), "changed").unwrap();
        assert_eq!(
            materialize(root.path()).unwrap_err().code,
            "ai_bundle_invalid"
        );
    }
}

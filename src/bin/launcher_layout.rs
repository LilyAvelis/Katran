//! Installed/portable layout resolution, independent of the caller's directory.
use std::{
    io,
    path::{Path, PathBuf},
};
pub fn root(executable: &Path) -> io::Result<PathBuf> {
    let parent = executable
        .parent()
        .ok_or_else(|| io::Error::other("Missing launcher directory"))?;
    if parent.join("runtime/current-version.txt").is_file() {
        return Ok(parent.to_path_buf());
    }
    if parent.join("current-version.txt").is_file()
        && parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("runtime"))
    {
        return parent
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| io::Error::other("Missing application directory"));
    }
    Err(io::Error::other(
        "Katran runtime is missing; extract the complete portable archive or reinstall Katran",
    ))
}
pub fn data(
    root: &Path,
    explicit: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
) -> io::Result<PathBuf> {
    if let Some(explicit) = explicit {
        return Ok(explicit);
    }
    if root.join("installed.flag").is_file() {
        return local_app_data
            .map(|path| path.join("Katran/data"))
            .ok_or_else(|| io::Error::other("LOCALAPPDATA is missing"));
    }
    Ok(root.join("katran-data"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn root_and_data_follow_layout_instead_of_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Portable with spaces");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::write(root.join("runtime/current-version.txt"), "012345abcdef").unwrap();
        assert_eq!(super::root(&root.join("Katran.exe")).unwrap(), root);
        assert_eq!(super::root(&root.join("runtime/Katran.exe")).unwrap(), root);
        assert_eq!(data(&root, None, None).unwrap(), root.join("katran-data"));
        std::fs::write(root.join("installed.flag"), "").unwrap();
        let local = dir.path().join("LocalAppData");
        assert_eq!(
            data(&root, None, Some(local.clone())).unwrap(),
            local.join("Katran/data")
        );
        let custom = dir.path().join("Explicit shelf");
        assert_eq!(data(&root, Some(custom.clone()), None).unwrap(), custom);
        assert!(data(&root, None, None).is_err());
        assert!(super::root(&dir.path().join("Incomplete/Katran.exe")).is_err());
    }
}

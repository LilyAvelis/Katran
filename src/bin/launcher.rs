#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Stable entry point for Start Menu/Raycast. No shell expansion or UI storage.
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
};
mod app_registration;
mod launch_args;
mod launcher_layout;

fn target(runtime: &Path) -> io::Result<PathBuf> {
    let version = std::fs::read_to_string(runtime.join("current-version.txt"))?;
    let version = version.trim();
    if version.len() != 12 || !version.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid Katran build pointer",
        ));
    }
    let executable = runtime.join("versions").join(version).join("katran-ui.exe");
    if !executable.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Katran build is missing",
        ));
    }
    Ok(executable)
}
fn run() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let root = launcher_layout::root(&executable)?;
    let runtime = root.join("runtime");
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--register")) {
        let programs =
            std::env::var_os("APPDATA").ok_or_else(|| io::Error::other("APPDATA is missing"))?;
        let programs = PathBuf::from(programs).join("Microsoft/Windows/Start Menu/Programs");
        std::fs::create_dir_all(&programs)?;
        return app_registration::write_link(&root, &programs.join("Katran.lnk"));
    }
    let args = launch_args::parse(std::env::args_os().skip(1))?;
    let data = launcher_layout::data(
        &root,
        args.profile,
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
    )?;
    let mut command = Command::new(target(&runtime)?);
    command.arg(data);
    if let Some(folder) = args.folder {
        command.arg("--open-folder").arg(folder);
    }
    command.current_dir(root).spawn()?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Cannot launch Katran: {error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_resolves_current_build_and_rejects_traversal() {
        let root = tempfile::tempdir().unwrap();
        let version = "012345abcdef";
        let path = root
            .path()
            .join("versions")
            .join(version)
            .join("katran-ui.exe");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"synthetic").unwrap();
        std::fs::write(root.path().join("current-version.txt"), version).unwrap();
        assert_eq!(target(root.path()).unwrap(), path);
        std::fs::write(root.path().join("current-version.txt"), "../../source").unwrap();
        assert!(target(root.path()).is_err());
    }
}

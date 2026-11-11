//! Explicit launch intent keeps a folder separate from the application's profile.
use std::{
    ffi::{OsStr, OsString},
    io,
    path::PathBuf,
};
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Arguments {
    pub profile: Option<PathBuf>,
    pub folder: Option<PathBuf>,
}
pub(super) fn parse(args: impl IntoIterator<Item = OsString>) -> io::Result<Arguments> {
    let args: Vec<_> = args.into_iter().collect();
    let flag = OsStr::new("--open-folder");
    match args.as_slice() {
        [] => Ok(Arguments::default()),
        [profile] if !profile.to_string_lossy().starts_with("--") => Ok(Arguments {
            profile: Some(profile.into()),
            folder: None,
        }),
        [arg, folder] if arg == flag && !folder.is_empty() => Ok(Arguments {
            profile: None,
            folder: Some(folder.into()),
        }),
        [profile, arg, folder]
            if arg == flag
                && !folder.is_empty()
                && !profile.to_string_lossy().starts_with("--") =>
        {
            Ok(Arguments {
                profile: Some(profile.into()),
                folder: Some(folder.into()),
            })
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Usage: Katran [profile] [--open-folder <directory>]",
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> io::Result<Arguments> {
        parse(values.iter().map(OsString::from))
    }
    #[test]
    fn folder_intent_never_becomes_a_profile_or_shell_command() {
        let path = r"C:\Катран & work\folder\.";
        let value = args(&["--open-folder", path]).unwrap();
        assert_eq!(value.profile, None);
        assert_eq!(value.folder, Some(PathBuf::from(path)));
        let value = args(&["profile", "--open-folder", path]).unwrap();
        assert_eq!(value.profile, Some(PathBuf::from("profile")));
        assert_eq!(value.folder, Some(PathBuf::from(path)));
        for value in [
            vec!["--open-folder"],
            vec!["--unknown"],
            vec!["--open-folder", "a", "b"],
            vec!["--open-folder", ""],
        ] {
            assert!(args(&value).is_err());
        }
    }
}

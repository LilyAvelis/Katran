//! Shared choice of portable collection format for ordinary files.
use crate::{ApiError, Event, Result, file_types};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Images,
    Links,
    Text,
    Files(String),
    Bs,
}

impl Kind {
    pub(crate) fn extension(&self) -> &'static str {
        if *self == Self::Bs { "bs" } else { "bull" }
    }
}

pub(crate) fn choose_files(events: &[Event]) -> Kind {
    let Some(first) = events.first().map(|event| classify(&event.name)) else {
        return Kind::Bs;
    };
    if events.iter().all(|event| classify(&event.name) == first) {
        first
    } else {
        Kind::Bs
    }
}

fn classify(name: &str) -> Kind {
    let path = Path::new(name);
    if file_types::is_native_image(path) {
        return Kind::Images;
    }
    let suffix = extension(path).unwrap_or_default();
    if file_types::is_native_video(path) {
        return Kind::Files("video".into());
    }
    if matches!(
        suffix.as_str(),
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "wma"
    ) {
        return Kind::Files("audio".into());
    }
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_alphanumeric()) || suffix.len() > 24
    {
        return Kind::Bs;
    }
    Kind::Files(format!("ext-{suffix}"))
}

pub(crate) fn extension(path: &Path) -> Option<String> {
    path.extension()?.to_str().map(str::to_ascii_lowercase)
}

pub(crate) fn is_archive(event: &Event) -> bool {
    matches!(
        extension(Path::new(&event.name)).as_deref(),
        Some("bs" | "bull")
    )
}

pub(crate) fn external(event: &Event) -> bool {
    event.storage == "reference"
        && matches!(event.source.as_str(), "work_folder" | "files")
        && is_archive(event)
}

fn family_valid(family: &str) -> bool {
    matches!(family, "video" | "audio")
        || family.strip_prefix("ext-").is_some_and(|suffix| {
            (1..=24).contains(&suffix.len())
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

pub(crate) fn probe(event: &Event) -> Result<Kind> {
    let mut reader = File::open(&event.path)?;
    let mut header = [0; 16];
    reader.read_exact(&mut header)?;
    let expected = extension(Path::new(&event.name));
    let kind = match (&header[..8], &header[8..12], &header[12..16]) {
        (b"BALOHBS\0", [1, 0, 0, 0], _) if expected.as_deref() == Some("bs") => Kind::Bs,
        (b"BALOHCOL", [1, 0, 0, 0], _) => Kind::Images,
        (b"BALOHCOL", [2, 0, 0, 0], b"LINK") => Kind::Links,
        (b"BALOHCOL", [2, 0, 0, 0], b"TEXT") => Kind::Text,
        (b"BALOHCOL", [2, 0, 0, 0], b"FILE") => Kind::Files(read_family(&mut reader)?),
        _ => {
            return Err(ApiError::new(
                "invalid_collection",
                "Invalid collection header",
            ));
        }
    };
    if expected.as_deref() != Some(kind.extension()) {
        return Err(ApiError::new(
            "invalid_collection",
            "Collection suffix does not match its format",
        ));
    }
    Ok(kind)
}

fn read_family(mut reader: impl Read) -> Result<String> {
    let mut size = [0];
    reader.read_exact(&mut size)?;
    if !(1..=28).contains(&size[0]) {
        return Err(ApiError::new("invalid_collection", "Invalid file family"));
    }
    let mut bytes = vec![0; size[0] as usize];
    reader.read_exact(&mut bytes)?;
    let family =
        String::from_utf8(bytes).map_err(|error| ApiError::new("invalid_collection", error))?;
    if !family_valid(&family) {
        return Err(ApiError::new(
            "invalid_collection",
            "Unsupported file family",
        ));
    }
    Ok(family)
}

pub(crate) fn read_raw(
    mut reader: impl Read,
    kind: &Kind,
) -> Result<Vec<baloh_collection::bs::FileEntry>> {
    if let Kind::Files(expected) = kind {
        let mut header = [0; 16];
        reader.read_exact(&mut header)?;
        if &header != b"BALOHCOL\x02\0\0\0FILE" || read_family(&mut reader)? != *expected {
            return Err(ApiError::new(
                "invalid_collection",
                "File BULL kind changed",
            ));
        }
    }
    let entries = baloh_collection::bs::read(reader)?;
    if matches!(kind, Kind::Files(_)) {
        validate_entries(&entries, kind)?;
    }
    Ok(entries)
}

fn validate_entries(entries: &[baloh_collection::bs::FileEntry], kind: &Kind) -> Result<()> {
    if entries.iter().any(|entry| classify(&entry.name) != *kind) {
        return Err(ApiError::new(
            "invalid_collection",
            "File BULL contains a file outside its declared family",
        ));
    }
    Ok(())
}

pub(crate) fn write_raw(
    mut writer: impl Write,
    entries: &[baloh_collection::bs::FileEntry],
    kind: &Kind,
) -> Result<()> {
    if let Kind::Files(family) = kind {
        if !family_valid(family) {
            return Err(ApiError::new(
                "invalid_collection",
                "Unsupported file family",
            ));
        }
        validate_entries(entries, kind)?;
        writer.write_all(b"BALOHCOL\x02\0\0\0FILE")?;
        writer.write_all(&[family.len() as u8])?;
        writer.write_all(family.as_bytes())?;
    }
    Ok(baloh_collection::bs::write(writer, entries)?)
}

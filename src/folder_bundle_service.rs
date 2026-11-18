use crate::{
    ApiError, Event, Result, bs_collection, collection,
    collection_kind::{self, Kind},
    domain_jobs::Cancellation,
    folder_collection, image_collection, media_service, open_file,
};
use baloh_collection::bs::FileEntry;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub(crate) struct Prepared {
    pub(crate) folder: PathBuf,
    pub(crate) sources: Vec<(Event, media_service::Stamp)>,
    pub(crate) output: folder_collection::PreparedOutput,
    pub(crate) kind: Kind,
    pub(crate) count: usize,
    pub(crate) merged: bool,
    pub(crate) unpacked: bool,
}

pub(crate) fn prepare(
    events: Vec<Event>,
    folder: PathBuf,
    quota: u64,
    cancel: &Cancellation,
) -> Result<Prepared> {
    let mut sources = Vec::with_capacity(events.len());
    for event in &events {
        cancel.check()?;
        if event.source != "work_folder" || event.path.parent() != Some(folder.as_path()) {
            return Err(ApiError::new(
                "stale_folder",
                "Select files from the current Folder",
            ));
        }
        open_file::validate(event)?;
        sources.push((event.clone(), media_service::stamp(&event.path)?));
    }
    let archives = events.iter().all(collection_kind::external);
    if !archives && !events.iter().all(bs_collection::is_packable_file) {
        return Err(ApiError::new(
            "invalid_selection",
            "Select loose files or collections of the same type",
        ));
    }
    let kind = if archives {
        let kind = collection_kind::probe(&events[0])?;
        for event in &events[1..] {
            if collection_kind::probe(event)? != kind {
                return Err(ApiError::new(
                    "invalid_selection",
                    "Merge collections of the same type",
                ));
            }
        }
        kind
    } else {
        collection_kind::choose_files(&events)
    };

    let unpacked = archives && events.len() == 1;
    let (output, count) = if unpacked {
        let event = &events[0];
        let entries = entries(event, &kind, cancel)?;
        let count = entries.len();
        let stem = Path::new(&event.name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Collection");
        (
            folder_collection::prepare_extraction(&folder, stem, entries)?,
            count,
        )
    } else {
        let (bytes, count) = match &kind {
            Kind::Images => {
                let (_, bytes, count) = image_collection::encode(events, quota, cancel)?;
                (bytes, count)
            }
            Kind::Bs | Kind::Files(_) => {
                let (_, bytes, count) = bs_collection::encode(events, &kind, quota, cancel)?;
                (bytes, count)
            }
            Kind::Text => {
                let mut values = Vec::new();
                for event in &events {
                    cancel.check()?;
                    values.extend(collection::read_texts(event)?);
                    if values.len() > 32 {
                        return Err(too_many());
                    }
                }
                let mut bytes = Vec::new();
                baloh_collection::text::write(&mut bytes, &values)?;
                (bytes, values.len())
            }
            Kind::Links => {
                let mut values = Vec::new();
                for event in &events {
                    cancel.check()?;
                    values.extend(collection::read_links(event)?);
                    if values.len() > 32 {
                        return Err(too_many());
                    }
                }
                let mut bytes = Vec::new();
                baloh_collection::links::write(&mut bytes, &values)?;
                (bytes, values.len())
            }
        };
        if bytes.len() as u64 > quota {
            return Err(ApiError::new(
                "collection_size_limit",
                "Collection exceeds the size budget",
            ));
        }
        (
            folder_collection::prepare(&folder, kind.extension(), &bytes)?,
            count,
        )
    };
    cancel.check()?;
    Ok(Prepared {
        folder,
        sources,
        output,
        kind,
        count,
        merged: archives && !unpacked,
        unpacked,
    })
}

fn too_many() -> ApiError {
    ApiError::new(
        "collection_size_limit",
        "A collection can hold up to 32 entries",
    )
}

fn entries(event: &Event, kind: &Kind, cancel: &Cancellation) -> Result<Vec<FileEntry>> {
    open_file::validate(event)?;
    let result = match kind {
        Kind::Bs | Kind::Files(_) => return bs_collection::read_collection(event, cancel),
        Kind::Images => baloh_collection::codec::read(File::open(&event.path)?)?
            .into_iter()
            .enumerate()
            .map(|(index, bytes)| FileEntry {
                name: format!("{:02}.png", index + 1),
                bytes,
            })
            .collect(),
        Kind::Text => collection::read_texts(event)?
            .into_iter()
            .enumerate()
            .map(|(index, value)| FileEntry {
                name: format!("{:02}.txt", index + 1),
                bytes: value.into_bytes(),
            })
            .collect(),
        Kind::Links => collection::read_links(event)?
            .into_iter()
            .enumerate()
            .map(|(index, link)| FileEntry {
                name: format!("{:02}.url", index + 1),
                bytes: format!("[InternetShortcut]\r\nURL={}\r\n", link.url).into_bytes(),
            })
            .collect(),
    };
    cancel.check()?;
    Ok(result)
}

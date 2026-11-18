//! Shelf integration for portable BULL entries. Metadata is saved in the file.
use crate::{
    ApiError, Event, EventKind, Result, domain_jobs::Cancellation, image_collection,
    interaction::ShelfTab, link_fragments, media_service, open_file, origin::Origin, storage,
};
use baloh_collection::links::{self, Link};
use base64::Engine;
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) const LINK_SOURCE: &str = "link_collection";

pub(crate) fn is_link_collection(event: &Event) -> bool {
    event.storage == "cached"
        && event.source == LINK_SOURCE
        && Path::new(&event.name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bull"))
}

pub(crate) fn is_managed(event: &Event) -> bool {
    image_collection::is_managed(event) || is_link_collection(event)
}

fn read_links(event: &Event) -> Result<Vec<Link>> {
    open_file::validate(event)?;
    let entries = links::read(File::open(&event.path)?)?;
    if entries
        .iter()
        .any(|entry| link_fragments::recognize(&entry.url).is_none())
    {
        return Err(ApiError::new(
            "invalid_link",
            "BULL contains an invalid HTTP(S) URL",
        ));
    }
    Ok(entries)
}

fn link_at(event: &Event, index: usize, cancel: &Cancellation) -> Result<(Link, usize)> {
    cancel.check()?;
    let entries = read_links(event)?;
    cancel.check()?;
    let count = entries.len();
    let entry = entries
        .into_iter()
        .nth(index)
        .ok_or_else(|| ApiError::new("invalid_page", "Page is outside the collection"))?;
    Ok((entry, count))
}

pub(crate) fn link_url(event: &Event, index: usize, cancel: &Cancellation) -> Result<String> {
    Ok(link_at(event, index, cancel)?.0.url)
}

pub(crate) fn link_page(
    event: &Event,
    index: usize,
    cancel: &Cancellation,
) -> Result<serde_json::Value> {
    let (link, count) = link_at(event, index, cancel)?;
    let preview = link
        .preview_png
        .map(|png| base64::engine::general_purpose::STANDARD.encode(png));
    Ok(serde_json::json!({ "index":index, "count":count, "link":{
        "url":link.url, "title":link.title, "description":link.description, "preview_png":preview,
    }}))
}

pub(crate) fn pack_links(
    events: Vec<Event>,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<storage::CollectionPack> {
    let merge = events.first().is_some_and(is_link_collection);
    let mut entries = Vec::with_capacity(events.len());
    let mut inputs = Vec::with_capacity(events.len());
    for event in events {
        cancel.check()?;
        let stamp = media_service::stamp(&event.path)?;
        let offset = entries.len();
        if merge {
            entries.extend(read_links(&event)?);
        } else {
            let url = link_fragments::read_event(&event)?;
            if url.len() > links::MAX_URL_BYTES {
                return Err(ApiError::new(
                    "collection_size_limit",
                    "BULL supports URLs up to 8 KiB",
                ));
            }
            entries.push(Link {
                url,
                title: String::new(),
                description: String::new(),
                preview_png: None,
            });
        }
        if entries.len() > baloh_collection::codec::MAX_PAGES {
            return Err(ApiError::new(
                "collection_size_limit",
                "BULL supports up to 32 entries",
            ));
        }
        if media_service::stamp(&event.path)? != stamp {
            return Err(ApiError::new(
                "source_changed",
                "Link or collection changed while reading",
            ));
        }
        inputs.push(storage::CollectionInput {
            event,
            stamp,
            offset,
            count: entries.len() - offset,
        });
    }
    if !merge && !entries.is_empty() {
        let deadline = Instant::now() + Duration::from_secs(7);
        let chunk_size = entries.len().div_ceil(4);
        std::thread::scope(|scope| {
            for chunk in entries.chunks_mut(chunk_size) {
                scope.spawn(move || {
                    for link in chunk {
                        if cancel.is_cancelled() || Instant::now() >= deadline {
                            break;
                        }
                        if let Some(metadata) =
                            crate::favicon_fetch::link_metadata(&link.url, deadline)
                        {
                            link.title = metadata.title;
                            link.description = metadata.description;
                            link.preview_png = metadata.preview_png;
                        }
                    }
                });
            }
        });
    }
    cancel.check()?;
    let mut portable = Vec::new();
    links::write(&mut portable, &entries)?;
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: "Links collection.bull",
            source: LINK_SOURCE,
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )?;
    Ok(storage::CollectionPack {
        inputs,
        blob,
        tab: ShelfTab::Links,
        count: entries.len(),
    })
}

pub(crate) fn unpack_links(
    event: &Event,
    members: &storage::CollectionMembers,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Vec<Option<storage::PreparedBlob>>> {
    cancel.check()?;
    let links = read_links(event)?;
    if members
        .last_key_value()
        .is_some_and(|(&position, _)| position >= links.len())
    {
        return Err(ApiError::new(
            "invalid_collection",
            "Saved collection membership is out of range",
        ));
    }
    links
        .into_iter()
        .enumerate()
        .map(|(index, link)| {
            cancel.check()?;
            if members.contains_key(&index) {
                return Ok(None);
            }
            link_fragments::prepare(
                root,
                quota,
                &link.url,
                "link_collection_page",
                &Origin::default(),
                references,
            )
            .map(Some)
        })
        .collect()
}

pub(crate) fn packable_link(event: &Event) -> bool {
    event.kind == EventKind::LinkFragment
}

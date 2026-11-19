use crate::{
    ApiError, Event, Result, bs_collection, collection,
    collection_kind::{self, Kind},
    delete_selection,
    domain_jobs::Cancellation,
    downloads::Trash,
    folder_collection, image_collection, media_service, open_file, preview, safe_recycle,
};
use baloh_collection::bs::FileEntry;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::PathBuf};

pub(crate) struct Prepared {
    pub(crate) folder: PathBuf,
    pub(crate) sources: Vec<(Event, media_service::Stamp)>,
    output: Output,
    pub(crate) kind: Kind,
    pub(crate) count: usize,
    pub(crate) merged: bool,
    pub(crate) unpacked: bool,
    archive_hash: Option<String>,
    locked_sources: Vec<LockedSource>,
}

enum Output {
    Archive(folder_collection::PreparedOutput),
    Extraction(folder_collection::PreparedFlatExtraction),
}

struct LockedSource {
    event: Event,
    stamp: media_service::Stamp,
    hash: String,
    locked: delete_selection::LockedOriginal,
}

pub(crate) fn prepare(
    events: Vec<Event>,
    folder: PathBuf,
    quota: u64,
    pages: bool,
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
    } else if pages {
        if !events
            .iter()
            .all(|event| crate::file_types::is_markdown(&event.path))
        {
            return Err(ApiError::new(
                "invalid_selection",
                "Select Markdown pages or BULLs of the same type",
            ));
        }
        Kind::Text
    } else {
        collection_kind::choose_folder_files(&events)
    };

    let unpacked = archives && events.len() == 1;
    // Bind both packing inputs and the archive being unpacked to verified,
    // deny-write handles before reading any bytes. Keep them through recycle.
    let mut locked_sources = Vec::new();
    let root = folder.canonicalize()?;
    for (event, stamp) in &sources {
        cancel.check()?;
        let hash = preview::content_hash(&event.path)?;
        let locked = delete_selection::prepare_locked_original(&event.path, &root, &hash)?;
        open_file::validate(event)?;
        if media_service::stamp(&event.path)? != *stamp {
            return Err(ApiError::new(
                "source_changed",
                "Folder file changed before collection processing",
            ));
        }
        locked_sources.push(LockedSource {
            event: event.clone(),
            stamp: stamp.clone(),
            hash,
            locked,
        });
    }
    let mut archive_hash = None;
    let (output, count) = if unpacked {
        let event = &events[0];
        let entries = entries(event, &kind, pages, cancel)?;
        let count = entries.len();
        (
            Output::Extraction(folder_collection::prepare_flat_extraction(
                &folder, entries,
            )?),
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
                    if pages && !archives {
                        let mut bytes = Vec::new();
                        File::open(&event.path)?
                            .take((crate::text_fragments::MAX_TEXT_BYTES + 1) as u64)
                            .read_to_end(&mut bytes)?;
                        if bytes.len() > crate::text_fragments::MAX_TEXT_BYTES {
                            return Err(ApiError::new(
                                "text_too_large",
                                "Markdown exceeds the collection page size limit",
                            ));
                        }
                        values.push(String::from_utf8(bytes).map_err(|_| {
                            ApiError::new("invalid_text", "Markdown must be UTF-8")
                        })?);
                    } else {
                        values.extend(collection::read_texts(event)?);
                    }
                    if values.len() > baloh_collection::MAX_ENTRIES {
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
                    if values.len() > baloh_collection::MAX_ENTRIES {
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
        archive_hash = Some(format!("{:x}", Sha256::digest(&bytes)));
        (
            Output::Archive(folder_collection::prepare(
                &folder,
                kind.extension(),
                &bytes,
            )?),
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
        archive_hash,
        locked_sources,
    })
}

/// Publish first. A verified locked archive is required before any source is
/// recycled; every failed or recoverable item remains explicit in the receipt.
/// The caller must check current folder, selection and pinned paths immediately
/// before authorizing this irreversible phase.
pub(crate) fn finish(prepared: Prepared, trash: &dyn Trash) -> Result<Value> {
    let Prepared {
        folder,
        output,
        kind,
        count,
        merged,
        unpacked,
        archive_hash,
        locked_sources,
        ..
    } = prepared;
    let (path, published, archive_guard) = match output {
        Output::Extraction(staged) => {
            let publication = staged.publish();
            let complete = publication.error.is_none() && publication.paths.len() == count;
            if !complete {
                let error = publication.error.map(|e| json!(e)).unwrap_or_else(|| json!({"code":"incomplete_publication","message":"Not all files were published"}));
                let items: Vec<_> = locked_sources.iter().map(|source| json!({
                    "id":source.event.id,"path":source.event.path,"ok":false,"status":"retained",
                    "original_removed":false,"recovery_path":null,"error":"Extraction was incomplete"
                })).collect();
                let remaining = items.len();
                return Ok(
                    json!({"folder_collection":true,"path":folder,"count":count,"merged":merged,
                    "unpacked":true,"kind":kind.extension(),"publication_complete":false,
                    "files_published":false,"archive_unchanged":true,"publication_error":error,"published_paths":publication.paths,
                    "original_removal":items,"sources_remaining":remaining}),
                );
            }
            (folder.clone(), Some(publication), None)
        }
        Output::Archive(staged) => {
            let path = staged.publish()?;
            let expected = archive_hash.unwrap_or_default();
            let guard = folder.canonicalize().and_then(|root| {
                delete_selection::prepare_locked_original(&path, &root, &expected)
                    .map_err(|error| std::io::Error::other(error.message))
            });
            let guard = match guard {
                Ok(guard) => guard,
                Err(error) => {
                    let items: Vec<_> = locked_sources.iter().map(|source| json!({
                        "id":source.event.id,"path":source.event.path,"ok":false,"status":"retained",
                        "original_removed":false,"recovery_path":null,"error":"Archive verification failed"
                    })).collect();
                    let remaining = items.len();
                    return Ok(
                        json!({"folder_collection":true,"path":path,"count":count,"merged":merged,
                        "unpacked":false,"kind":kind.extension(),"archive_verified":false,
                        "verification_error":error.to_string(),"original_removal":items,"sources_remaining":remaining}),
                    );
                }
            };
            (path, None, Some(guard))
        }
    };
    let mut items = Vec::with_capacity(locked_sources.len());
    let mut remaining = 0usize;
    for source in locked_sources {
        let result = (|| -> Result<_> {
            open_file::validate(&source.event)?;
            if media_service::stamp(&source.event.path)? != source.stamp {
                return Err(ApiError::new(
                    "source_changed",
                    "Folder file changed before recycling",
                ));
            }
            safe_recycle::recycle_locked(
                source.locked,
                &source.event.path,
                &folder,
                &source.hash,
                trash,
            )
        })();
        match result {
            Ok(receipt) => {
                if !receipt.recycled {
                    remaining += 1;
                }
                items.push(
                    json!({"id":source.event.id,"path":source.event.path,"ok":receipt.recycled,
                    "status":receipt.status,"original_removed":receipt.original_removed,
                    "recovery_path":receipt.recovery_path,"recycle":receipt}),
                );
            }
            Err(error) => {
                remaining += 1;
                items.push(json!({"id":source.event.id,"path":source.event.path,"ok":false,
                    "status":"retained","original_removed":false,"recovery_path":null,"error":error}));
            }
        }
    }
    drop(archive_guard);
    let published_paths = published.as_ref().map(|result| result.paths.clone());
    let archive_unchanged = unpacked && items.iter().all(|item| item["original_removed"] == false);
    let publication_complete = !unpacked || remaining == 0;
    drop(published);
    Ok(
        json!({"folder_collection":true,"path":path,"count":count,"merged":merged,
        "unpacked":unpacked,"kind":kind.extension(),"archive_verified":true,
        "publication_complete":publication_complete,"files_published":true,
        "archive_unchanged":archive_unchanged,"published_paths":published_paths,
        "original_removal":items,"sources_remaining":remaining}),
    )
}

fn too_many() -> ApiError {
    ApiError::new(
        "collection_size_limit",
        format!(
            "A collection can hold up to {} entries",
            baloh_collection::MAX_ENTRIES
        ),
    )
}

fn entries(
    event: &Event,
    kind: &Kind,
    pages: bool,
    cancel: &Cancellation,
) -> Result<Vec<FileEntry>> {
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
                name: format!("{:02}.{}", index + 1, if pages { "md" } else { "txt" }),
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

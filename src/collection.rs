//! Shelf integration for portable BULL entries. Metadata is saved in the file.
use crate::{
    ApiError, Event, EventKind, Result, domain_jobs::Cancellation, image_collection,
    interaction::ShelfTab, link_fragments, media_service, open_file, origin::Origin, storage,
};
use baloh_collection::{
    links::{self, Link},
    text,
};
use base64::Engine;
use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) const LINK_SOURCE: &str = "link_collection";
pub(crate) const TEXT_SOURCE: &str = "text_collection";

pub(crate) type LinkTitles = BTreeMap<usize, String>;

pub(crate) fn link_titles(db: &rusqlite::Connection, event: &Event) -> Result<LinkTitles> {
    if packable_link(event) {
        return Ok(crate::pinned::properties(db, event.id)?
            .display_name
            .map(|name| (0, name))
            .into_iter()
            .collect());
    }
    if !is_link_collection(event) {
        return Ok(LinkTitles::new());
    }
    let mut titles = db.prepare(
        "SELECT m.position, p.display_name FROM bull_members m
         JOIN katran_properties p ON p.event_id=m.member_id
         WHERE m.collection_id=?1 AND p.display_name IS NOT NULL",
    )?;
    Ok(titles
        .query_map([event.id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn is_link_collection(event: &Event) -> bool {
    is_typed_collection(event, LINK_SOURCE)
}

pub(crate) fn is_text_collection(event: &Event) -> bool {
    is_typed_collection(event, TEXT_SOURCE)
}

fn is_typed_collection(event: &Event, source: &str) -> bool {
    event.storage == "cached"
        && event.source == source
        && Path::new(&event.name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bull"))
}

pub(crate) fn is_managed(event: &Event) -> bool {
    image_collection::is_managed(event) || is_link_collection(event) || is_text_collection(event)
}

pub(crate) fn read_links(event: &Event) -> Result<Vec<Link>> {
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
    display_name: Option<&str>,
    cancel: &Cancellation,
) -> Result<serde_json::Value> {
    let (link, count) = link_at(event, index, cancel)?;
    let preview = link
        .preview_png
        .map(|png| base64::engine::general_purpose::STANDARD.encode(png));
    Ok(serde_json::json!({ "index":index, "count":count, "link":{
        "url":link.url, "title":display_name.unwrap_or(&link.title), "description":link.description, "preview_png":preview,
    }}))
}

pub(crate) fn pack_links(
    events: Vec<(Event, LinkTitles)>,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<storage::CollectionPack> {
    let merge = events
        .first()
        .is_some_and(|(event, _)| is_link_collection(event));
    let mut entries = Vec::with_capacity(events.len());
    let mut inputs = Vec::with_capacity(events.len());
    for (event, titles) in events {
        cancel.check()?;
        let stamp = media_service::stamp(&event.path)?;
        let offset = entries.len();
        if merge {
            for (index, mut entry) in read_links(&event)?.into_iter().enumerate() {
                if let Some(title) = titles.get(&index) {
                    entry.title.clone_from(title);
                }
                entries.push(entry);
            }
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
                title: titles.into_values().next().unwrap_or_default(),
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
    cancel.check()?;
    let blob = prepare_links(&entries, "Links collection.bull", root, quota, references)?;
    Ok(storage::CollectionPack {
        inputs,
        blob,
        tab: ShelfTab::Links,
        count: entries.len(),
    })
}

fn prepare_links(
    entries: &[Link],
    name: &str,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
) -> Result<storage::PreparedBlob> {
    let mut portable = Vec::new();
    links::write(&mut portable, entries)?;
    storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name,
            source: LINK_SOURCE,
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )
}

pub(crate) fn prepare_link_metadata(
    event: &Event,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Option<storage::PreparedBlob>> {
    cancel.check()?;
    let mut entries = read_links(event)?;
    let deadline = Instant::now() + Duration::from_secs(7);
    let changed = enrich_links(
        &mut entries,
        deadline,
        cancel,
        &crate::favicon_fetch::link_metadata,
    );
    cancel.check()?;
    if !changed {
        return Ok(None);
    }
    prepare_links(&entries, &event.name, root, quota, references).map(Some)
}

fn enrich_links(
    entries: &mut [Link],
    deadline: Instant,
    cancel: &Cancellation,
    fetch: &(impl Fn(&str, Instant) -> Option<crate::favicon_fetch::LinkMetadata> + Sync),
) -> bool {
    let pending: Vec<_> = entries
        .iter_mut()
        .filter(|entry| entry.title.is_empty() || entry.description.is_empty())
        .collect();
    if pending.is_empty() {
        return false;
    }
    let mut pending = pending;
    let changed = std::sync::atomic::AtomicBool::new(false);
    let chunk_size = pending.len().div_ceil(4);
    std::thread::scope(|scope| {
        for chunk in pending.chunks_mut(chunk_size) {
            let changed = &changed;
            scope.spawn(move || {
                for link in chunk {
                    if cancel.is_cancelled() || Instant::now() >= deadline {
                        break;
                    }
                    if let Some(metadata) = fetch(&link.url, deadline) {
                        if link.title.is_empty() && !metadata.title.is_empty() {
                            link.title = metadata.title;
                            changed.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        if link.description.is_empty() && !metadata.description.is_empty() {
                            link.description = metadata.description;
                            changed.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            });
        }
    });
    changed.into_inner()
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

#[cfg(test)]
#[test]
fn link_metadata_wait_does_not_block_pack_or_read_and_commits_in_place() {
    use crate::{
        Command, Request, Service,
        domain_jobs::{JobId, JobState, Lane},
        interaction::SelectionMode,
        jobs_service::PreparedWork,
    };
    use std::sync::{Mutex, mpsc};
    let profile = tempfile::tempdir().unwrap();
    let mut api = Service::open(profile.path(), crate::DEFAULT_QUOTA).unwrap();
    let call = |api: &mut Service, command| {
        let response = api.execute(Request {
            api_version: 1,
            request_id: "background-metadata-test".into(),
            command,
        });
        assert_eq!(response["ok"], true, "{response}");
        response["result"].clone()
    };
    let finish = |api: &mut Service, job_id| {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let result = call(api, Command::PollJob { job_id });
            match result["state"].as_str() {
                Some("succeeded") => break result["result"].clone(),
                Some("queued" | "running" | "ready") if Instant::now() < deadline => {}
                _ => panic!("{result}"),
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Links,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Links,
        },
    );
    let url = "http://example.test/synthetic";
    let item = call(
        &mut api,
        Command::IngestLink {
            url: url.into(),
            source: "synthetic".into(),
        },
    );
    let id = item["id"].as_i64().unwrap();
    let custom_name = "Saved subject";
    call(&mut api, Command::TogglePins { ids: vec![id] });
    call(
        &mut api,
        Command::SetPinnedProperties {
            id,
            display_name: Some(custom_name.into()),
            opener: None,
        },
    );
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
    let (permit, gate) = mpsc::sync_channel::<Event>(1);
    let root = api.root.clone();
    let delayed = api
        .jobs
        .submit(Lane::LinkMetadata, move |cancel| {
            let event = gate.recv_timeout(Duration::from_secs(5)).unwrap();
            let mut entries = read_links(&event)?;
            let queries = Mutex::new(Vec::new());
            assert!(enrich_links(
                &mut entries,
                Instant::now() + Duration::from_secs(3),
                &cancel,
                &|url, _| {
                    queries.lock().unwrap().push(url.to_owned());
                    Some(crate::favicon_fetch::LinkMetadata {
                        title: "Title".into(),
                        description: "Description".into(),
                    })
                }
            ));
            assert_eq!(queries.into_inner().unwrap(), [url]);
            let blob = prepare_links(&entries, &event.name, &root, crate::DEFAULT_QUOTA, &[])?;
            Ok(PreparedWork::LinkCollectionMetadata {
                event,
                blob: Some(blob),
            })
        })
        .unwrap();
    let started = call(&mut api, Command::CreateImageCollection);
    let pack_id = JobId(started["job_id"].as_u64().unwrap());
    let packed = finish(&mut api, pack_id);
    let detached = JobId(packed["metadata_job_id"].as_u64().unwrap());
    call(&mut api, Command::CancelJob { job_id: detached });
    let event: Event = serde_json::from_value(packed["event"].clone()).unwrap();
    let page = call(
        &mut api,
        Command::TranslationCollectionPage {
            id: event.id,
            revision: event.revision.clone(),
            index: 0,
        },
    );
    assert_eq!(
        finish(&mut api, JobId(page["job_id"].as_u64().unwrap()))["link"]["url"],
        url
    );
    assert_eq!(read_links(&event).unwrap()[0].title, custom_name);

    permit.send(event.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while api.jobs.status(delayed).unwrap().state != JobState::Ready {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    finish(&mut api, pack_id);
    assert_eq!(api.get(event.id).unwrap().revision, event.revision);
    let receipt = finish(&mut api, delayed);
    assert_eq!(receipt["link_metadata_updated"], true);
    let enriched = api.get(event.id).unwrap();
    assert_ne!(enriched.revision, event.revision);
    let mut entries = read_links(&enriched).unwrap();
    assert_eq!(
        (
            &entries[0].url[..],
            &entries[0].title[..],
            &entries[0].description[..]
        ),
        (url, custom_name, "Description")
    );
    assert!(entries[0].preview_png.is_none());

    // Reproduce a collection packed before custom names were saved to the file.
    entries[0].title = "Website title".into();
    let old_blob =
        prepare_links(&entries, &event.name, &api.root, crate::DEFAULT_QUOTA, &[]).unwrap();
    assert!(
        storage::replace_link_collection_blob(
            &mut api.db,
            &api.root,
            api.quota,
            enriched.id,
            &enriched.revision,
            old_blob,
        )
        .unwrap()
    );
    let older = api.get(event.id).unwrap();
    let page = call(
        &mut api,
        Command::TranslationCollectionPage {
            id: older.id,
            revision: older.revision,
            index: 0,
        },
    );
    let page = finish(&mut api, JobId(page["job_id"].as_u64().unwrap()));
    assert_eq!(page["link"]["title"], custom_name);
    assert_eq!(page["link"]["description"], "Description");
}

pub(crate) fn packable_text(event: &Event) -> bool {
    event.kind == EventKind::TextFragment
}

pub(crate) fn read_texts(event: &Event) -> Result<Vec<String>> {
    open_file::validate(event)?;
    Ok(text::read(File::open(&event.path)?)?)
}

fn text_at(event: &Event, index: usize, cancel: &Cancellation) -> Result<(String, usize)> {
    cancel.check()?;
    let entries = read_texts(event)?;
    cancel.check()?;
    let count = entries.len();
    let source = entries
        .into_iter()
        .nth(index)
        .ok_or_else(|| ApiError::new("invalid_page", "Page is outside the collection"))?;
    Ok((source, count))
}

pub(crate) fn text_entry(event: &Event, index: usize, cancel: &Cancellation) -> Result<String> {
    Ok(text_at(event, index, cancel)?.0)
}

pub(crate) fn text_page(
    event: &Event,
    index: usize,
    cancel: &Cancellation,
) -> Result<serde_json::Value> {
    let (source, count) = text_at(event, index, cancel)?;
    Ok(serde_json::json!({"index":index,"count":count,"text":{"source":source}}))
}

pub(crate) fn pack_text(
    events: Vec<Event>,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<storage::CollectionPack> {
    let mut entries = Vec::with_capacity(events.len());
    let mut inputs = Vec::with_capacity(events.len());
    for event in events {
        cancel.check()?;
        let stamp = media_service::stamp(&event.path)?;
        let offset = entries.len();
        if is_text_collection(&event) {
            entries.extend(read_texts(&event)?);
        } else {
            entries.push(crate::text_fragments::read_text_event(&event)?);
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
                "Text or collection changed while reading",
            ));
        }
        inputs.push(storage::CollectionInput {
            event,
            stamp,
            offset,
            count: entries.len() - offset,
        });
    }
    cancel.check()?;
    let mut portable = Vec::new();
    text::write(&mut portable, &entries)?;
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: "Text collection.bull",
            source: TEXT_SOURCE,
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )?;
    Ok(storage::CollectionPack {
        inputs,
        blob,
        tab: ShelfTab::Text,
        count: entries.len(),
    })
}

pub(crate) fn unpack_text(
    event: &Event,
    members: &storage::CollectionMembers,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Vec<Option<storage::PreparedBlob>>> {
    cancel.check()?;
    let entries = read_texts(event)?;
    if members
        .last_key_value()
        .is_some_and(|(&position, _)| position >= entries.len())
    {
        return Err(ApiError::new(
            "invalid_collection",
            "Saved collection membership is out of range",
        ));
    }
    entries
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            cancel.check()?;
            if members.contains_key(&index) {
                return Ok(None);
            }
            crate::text_fragments::prepare(
                root,
                quota,
                &source,
                "text_collection_page",
                &Origin::default(),
                references,
            )
            .map(Some)
        })
        .collect()
}

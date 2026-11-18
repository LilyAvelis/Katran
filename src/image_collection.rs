//! Collect and extract ordered image pages using the existing BULL v1 codec.
use crate::{
    ApiError, Event, Result, domain_jobs::Cancellation, media_service, origin::Origin, storage,
};
use image::ImageDecoder;
use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

pub(crate) const SOURCE: &str = "image_collection";
const MAX_INPUT: u64 = 64 * 1024 * 1024;

pub(crate) fn is_managed(event: &Event) -> bool {
    event.storage == "cached"
        && matches!(
            event.source.as_str(),
            SOURCE | "image_translation_collection" | "file_image_collection"
        )
        && Path::new(&event.name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bull"))
}

pub(crate) fn is_image(event: &Event) -> bool {
    event.kind == crate::EventKind::ManagedImage || crate::file_types::is_native_image(&event.path)
}

fn decoded_page(bytes: &[u8]) -> Result<image::DynamicImage> {
    let invalid = |error: image::ImageError| ApiError::new("invalid_image", error);
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| ApiError::new("invalid_image", error))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(invalid)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 32_000_000 {
        return Err(ApiError::new(
            "invalid_image",
            "BULL supports pages up to 32 million pixels",
        ));
    }
    let orientation = decoder.orientation().map_err(invalid)?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(invalid)?;
    image.apply_orientation(orientation);
    Ok(image)
}

pub(crate) fn png_page(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    decoded_page(bytes)?
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| ApiError::new("invalid_image", error))?;
    Ok(output.into_inner())
}

pub(crate) fn cover(path: &Path) -> Result<image::DynamicImage> {
    let page = baloh_collection::codec::read_cover(File::open(path)?)?;
    decoded_page(&page)
}

pub(crate) fn prepare_page_copy(
    event: &Event,
    index: usize,
    cancel: &Cancellation,
) -> Result<Vec<u8>> {
    cancel.check()?;
    crate::open_file::validate(event)?;
    let pages = baloh_collection::codec::read(File::open(&event.path)?)?;
    let page = pages
        .get(index)
        .ok_or_else(|| ApiError::new("invalid_page", "Page is outside the collection"))?;
    cancel.check()?;
    let pixels = decoded_page(page)?.into_rgba8();
    cancel.check()?;
    Ok(crate::copy_out::dib(
        pixels.width(),
        pixels.height(),
        pixels.as_raw(),
    ))
}

pub(crate) fn pack(
    events: Vec<Event>,
    tab: crate::interaction::ShelfTab,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<storage::CollectionPack> {
    use crate::interaction::ShelfTab;
    let (inputs, portable, count) = encode(events, quota, cancel)?;
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: "Image collection.bull",
            source: match tab {
                ShelfTab::Translate => "image_translation_collection",
                ShelfTab::Files => "file_image_collection",
                _ => SOURCE,
            },
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )?;
    Ok(storage::CollectionPack {
        inputs,
        blob,
        tab,
        count,
    })
}

pub(crate) fn encode(
    events: Vec<Event>,
    quota: u64,
    cancel: &Cancellation,
) -> Result<(Vec<storage::CollectionInput>, Vec<u8>, usize)> {
    use baloh_collection::codec::{MAX_PAGE_BYTES, MAX_TOTAL_BYTES};
    let mut pages = Vec::with_capacity(events.len());
    let mut inputs = Vec::with_capacity(events.len());
    let mut total = 16usize;
    for (index, event) in events.into_iter().enumerate() {
        cancel.check()?;
        let page: Result<_> = (|| {
            let stamp = media_service::stamp(&event.path)?;
            if is_managed(&event)
                || (crate::collection_kind::external(&event)
                    && crate::collection_kind::probe(&event)?
                        == crate::collection_kind::Kind::Images)
            {
                crate::open_file::validate(&event)?;
                let pages = baloh_collection::codec::read(File::open(&event.path)?)?;
                if media_service::stamp(&event.path)? != stamp {
                    return Err(ApiError::new(
                        "source_changed",
                        "BULL changed while reading",
                    ));
                }
                return Ok((pages, stamp));
            }
            let file = File::open(&event.path)?;
            if file.metadata()?.len() > MAX_INPUT {
                return Err(ApiError::new(
                    "collection_size_limit",
                    "Input image exceeds 64 MiB",
                ));
            }
            let mut bytes = Vec::new();
            file.take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_INPUT {
                return Err(ApiError::new(
                    "collection_size_limit",
                    "Input image exceeds 64 MiB",
                ));
            }
            if let Some(expected) = event
                .revision
                .strip_prefix("blob:")
                .or_else(|| event.revision.strip_prefix("ref:"))
            {
                use sha2::{Digest, Sha256};
                if format!("{:x}", Sha256::digest(&bytes)) != expected {
                    return Err(ApiError::new("source_changed", "Image contents changed"));
                }
            } else {
                crate::open_file::validate(&event)?;
            }
            if media_service::stamp(&event.path)? != stamp {
                return Err(ApiError::new(
                    "source_changed",
                    "Image changed while reading",
                ));
            }
            Ok((vec![png_page(&bytes)?], stamp))
        })();
        let (input_pages, stamp) = page.map_err(|error| {
            ApiError::new(
                &error.code,
                format!("Item {} · {}", index + 1, error.message),
            )
        })?;
        let offset = pages.len();
        if offset + input_pages.len() > baloh_collection::codec::MAX_PAGES {
            return Err(ApiError::new(
                "collection_size_limit",
                "BULL supports up to 32 pages",
            ));
        }
        for png in &input_pages {
            total += 36 + png.len();
            if png.len() > MAX_PAGE_BYTES || total > MAX_TOTAL_BYTES || total as u64 > quota {
                return Err(ApiError::new(
                    "collection_size_limit",
                    "BULL exceeds the collection or cache size limit",
                ));
            }
        }
        inputs.push(storage::CollectionInput {
            event,
            stamp,
            offset,
            count: input_pages.len(),
        });
        pages.extend(input_pages);
    }
    let mut portable = Vec::with_capacity(total);
    baloh_collection::codec::write(&mut portable, &pages)?;
    cancel.check()?;
    Ok((inputs, portable, pages.len()))
}

pub(crate) fn unpack(
    event: &Event,
    members: &std::collections::BTreeMap<usize, i64>,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Vec<Option<storage::PreparedBlob>>> {
    cancel.check()?;
    crate::open_file::validate(event)?;
    let pages = baloh_collection::codec::read(File::open(&event.path)?)?;
    if members
        .last_key_value()
        .is_some_and(|(&position, _)| position >= pages.len())
    {
        return Err(ApiError::new(
            "invalid_collection",
            "Saved collection membership is out of range",
        ));
    }
    let mut prepared = Vec::with_capacity(pages.len());
    for (index, png) in pages.iter().enumerate() {
        cancel.check()?;
        if members.contains_key(&index) {
            prepared.push(None);
            continue;
        }
        decoded_page(png)?;
        let name = format!("BULL {} - {:02}.png", event.id, index + 1);
        prepared.push(Some(storage::prepare_bytes(
            root,
            quota,
            storage::ByteIngest {
                name: &name,
                source: if event.source == "file_image_collection" {
                    "bs_extracted_file"
                } else {
                    "image_collection_page"
                },
                bytes: png,
                origin: &Origin::default(),
            },
            references,
        )?));
    }
    crate::open_file::validate(event)?;
    Ok(prepared)
}

#[cfg(test)]
#[test]
fn collection_page_copy_prepares_only_the_requested_full_size_image() {
    use crate::{Command, Request, Service, domain_jobs::JobId, jobs_service::PreparedWork};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), crate::DEFAULT_QUOTA).unwrap();
    let png = |width| {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(width, 3)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    };
    let mut bull = Vec::new();
    baloh_collection::codec::write(&mut bull, &[png(10), png(2301)]).unwrap();
    let mut call = |command| {
        api.execute(Request {
            api_version: 1,
            request_id: "collection-copy-fixture".into(),
            command,
        })
    };
    let saved = call(Command::IngestBytes {
        name: "fixture.bull".into(),
        bytes: bull,
        source: SOURCE.into(),
    });
    let id = saved["result"]["id"].as_i64().unwrap();
    let revision = saved["result"]["revision"].as_str().unwrap().to_owned();
    let stale = call(Command::CopyCollectionPage {
        id,
        revision: "old".into(),
        index: 1,
    });
    assert_eq!(stale["error"]["code"], "stale_selection");
    let started = call(Command::CopyCollectionPage {
        id,
        revision,
        index: 1,
    });
    let job = JobId(started["result"]["job_id"].as_u64().unwrap());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some((ready, payload)) = api
            .jobs
            .take_ready(8)
            .into_iter()
            .find(|(ready, _)| *ready == job)
        {
            let PreparedWork::CopyCollectionPage { pixels, index, .. } = payload.unwrap() else {
                panic!("Expected a single page clipboard payload");
            };
            assert_eq!(index, 1);
            assert_eq!(u32::from_le_bytes(pixels[4..8].try_into().unwrap()), 2301);
            assert_eq!(pixels.len(), 124 + 2301 * 3 * 4);
            api.jobs.finish(ready, Ok(serde_json::Value::Null)).unwrap();
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let events = storage::query_events(&api.db, "", 10, None).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, id);
}

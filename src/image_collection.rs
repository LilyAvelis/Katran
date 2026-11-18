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
            SOURCE | "image_translation_collection"
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

fn png_page(bytes: &[u8]) -> Result<Vec<u8>> {
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

pub(crate) struct Packed {
    pub(crate) inputs: Vec<PackInput>,
    pub(crate) blob: storage::PreparedBlob,
    pub(crate) translated: bool,
    pub(crate) count: usize,
}

pub(crate) struct PackInput {
    pub(crate) event: Event,
    pub(crate) stamp: media_service::Stamp,
    pub(crate) offset: usize,
    pub(crate) count: usize,
}

pub(crate) fn pack(
    events: Vec<Event>,
    translated: bool,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Packed> {
    use baloh_collection::codec::{MAX_PAGE_BYTES, MAX_TOTAL_BYTES};
    let mut pages = Vec::with_capacity(events.len());
    let mut inputs = Vec::with_capacity(events.len());
    let mut total = 16usize;
    for (index, event) in events.into_iter().enumerate() {
        cancel.check()?;
        let page: Result<_> = (|| {
            let stamp = media_service::stamp(&event.path)?;
            if is_managed(&event) {
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
        inputs.push(PackInput {
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
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: "Image collection.bull",
            source: if translated {
                "image_translation_collection"
            } else {
                SOURCE
            },
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )?;
    Ok(Packed {
        inputs,
        blob,
        translated,
        count: pages.len(),
    })
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
                source: "image_collection_page",
                bytes: png,
                origin: &Origin::default(),
            },
            references,
        )?));
    }
    crate::open_file::validate(event)?;
    Ok(prepared)
}

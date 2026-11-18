//! Ordered single-page jobs collected into one portable artifact.
use crate::{
    ApiError, Event, ImageTranslationSource, Result, domain_jobs::Cancellation,
    image_translate::Session, media_service, origin::Origin, storage, translate_components,
};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

pub(crate) const MAX_PAGES: usize = 32;
const MAX_PAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_COLLECTION_BYTES: usize = 256 * 1024 * 1024;

pub(crate) struct Prepared {
    pub(crate) blob: storage::PreparedBlob,
    pub(crate) validated_sources: Vec<(Event, media_service::Stamp)>,
    pub(crate) receipt: Value,
}

pub(crate) fn prepare(
    events: Vec<Event>,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    language: ImageTranslationSource,
    numbered_only: bool,
    session: &mut Session,
    cancel: &Cancellation,
) -> Result<Prepared> {
    let components =
        translate_components::discover(root, translate_components::MODEL)?.resolve(language)?;
    let staging = root.join("translate-work");
    fs::create_dir_all(&staging)?;
    let total = events.len();
    let mut collection = baloh_collection::Collection::default();
    let mut included = Vec::new();
    let mut received = Vec::with_capacity(total);
    let mut bytes_count = 16usize;
    cancel.report_progress(0, total as u32);
    for (index, event) in events.into_iter().enumerate() {
        cancel.check()?;
        let result: Result<_> = (|| {
            let stamp = media_service::stamp(&event.path)?;
            let path = crate::wallpaper_engine::validate(root, &event)?;
            let mut bytes = Vec::new();
            File::open(&path)?
                .take(32 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 32 * 1024 * 1024 || media_service::stamp(&path)? != stamp {
                return Err(ApiError::new(
                    "invalid_image",
                    "Page changed or exceeds 32 MiB",
                ));
            }
            let rendered = session.translate(
                &bytes,
                &staging,
                components.clone(),
                language,
                "en",
                numbered_only,
                cancel,
            )?;
            let size = fs::metadata(rendered.path())?.len() as usize;
            if size > MAX_PAGE_BYTES
                || bytes_count.saturating_add(36).saturating_add(size) > MAX_COLLECTION_BYTES
                || (bytes_count + 36 + size) as u64 > quota
            {
                return Err(ApiError::new(
                    "collection_size_limit",
                    "This page exceeds the collection size limit",
                ));
            }
            if media_service::stamp(&path)? != stamp {
                return Err(ApiError::new(
                    "source_changed",
                    "Page changed during translation",
                ));
            }
            let png = fs::read(rendered.path())?;
            Ok((png, stamp))
        })();
        cancel.check()?;
        let entry = match result {
            Ok((png, stamp)) => {
                bytes_count += 36 + png.len();
                let page = included.len();
                included.push((event, stamp));
                received.push(json!({"index":index, "ok":true, "page":page}));
                Some(png)
            }
            Err(error) => {
                received.push(json!({"index":index, "ok":false,
                    "error":{"code":error.code,"message":error.message}}));
                None
            }
        };
        collection
            .push(baloh_collection::Entry {
                key: index,
                value: entry,
            })
            .map_err(|_| ApiError::new("collection_error", "Duplicate page index"))?;
        cancel.report_progress((index + 1) as u32, total as u32);
    }
    if included.is_empty() {
        let detail = received
            .first()
            .and_then(|item| item["error"]["message"].as_str())
            .unwrap_or("No page could be translated");
        return Err(ApiError::new("collection_empty", detail));
    }
    let pages: Vec<_> = collection
        .into_entries()
        .into_iter()
        .filter_map(|entry| entry.value)
        .collect();
    let mut portable = Vec::with_capacity(bytes_count);
    baloh_collection::codec::write(&mut portable, &pages)?;
    let name = format!("translation-{}.bull", language.code());
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: &name,
            source: "image_translation_collection",
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )?;
    cancel.check()?;
    Ok(Prepared {
        blob,
        validated_sources: included,
        receipt: json!({"collection":true, "source_language":language.code(), "target_language":"en",
            "total":total, "translated":pages.len(), "pages":received}),
    })
}

pub(crate) fn page(event: &Event, index: usize, cancel: &Cancellation) -> Result<Value> {
    cancel.check()?;
    crate::open_file::validate(event)?;
    let pages = baloh_collection::codec::read(File::open(&event.path)?)?;
    let png = pages
        .get(index)
        .ok_or_else(|| ApiError::new("invalid_page", "Page is outside the collection"))?;
    let mut reader = image::ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| ApiError::new("invalid_collection", error))?;
    cancel.check()?;
    let mut output = Cursor::new(Vec::new());
    let preview = if decoded.width() > 2048 || decoded.height() > 2048 {
        decoded.thumbnail(2048, 2048)
    } else {
        decoded
    };
    preview
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| ApiError::new("invalid_collection", error))?;
    Ok(
        json!({"index":index,"count":pages.len(),"png":base64::engine::general_purpose::STANDARD.encode(output.into_inner())}),
    )
}

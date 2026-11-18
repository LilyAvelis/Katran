//! Ordered single-page jobs collected into one portable artifact.
use crate::{
    ApiError, Event, ImageTranslationSource, Result,
    domain_jobs::Cancellation,
    image_translate::{RenderReport, Session},
    media_service,
    origin::Origin,
    storage, translate_components,
};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

pub(crate) use baloh_collection::codec::MAX_PAGES;
use baloh_collection::codec::{MAX_PAGE_BYTES, MAX_TOTAL_BYTES};
const MAX_SOURCE: usize = 32 * 1024 * 1024;

pub(crate) enum Input {
    Selection(Vec<Event>),
    Bull(Box<Event>),
}

impl Input {
    pub(crate) fn known_total(&self) -> Option<usize> {
        match self {
            Self::Selection(events) => Some(events.len()),
            Self::Bull(_) => None,
        }
    }
}

pub(crate) fn is_bull(event: &Event) -> bool {
    if crate::image_collection::is_managed(event) {
        return true;
    }
    use crate::collection_kind::{self, Kind};
    collection_kind::external(event)
        && collection_kind::extension(Path::new(&event.name)).as_deref() == Some("bull")
        && match collection_kind::probe(event) {
            Ok(Kind::Images) => true,
            Ok(Kind::Files(family)) => family == "images",
            _ => false,
        }
}

pub(crate) struct Prepared {
    pub(crate) blob: storage::PreparedBlob,
    pub(crate) validated_sources: Vec<(Event, media_service::Stamp)>,
    pub(crate) receipt: Value,
}

pub(crate) fn prepare(
    input: Input,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    (language, numbered_only): (ImageTranslationSource, bool),
    session: &mut Session,
    cancel: &Cancellation,
) -> Result<Prepared> {
    let components =
        translate_components::discover(root, translate_components::MODEL)?.resolve(language)?;
    let staging = root.join("translate-work");
    fs::create_dir_all(&staging)?;
    let name = format!("translation-{}.bull", language.code());
    let mut prepared = transform(
        input,
        root,
        quota,
        references,
        (&name, "image_translation_collection"),
        cancel,
        |bytes| {
            let rendered = session.translate(
                bytes,
                &staging,
                components.clone(),
                language,
                numbered_only,
                cancel,
            )?;
            Ok(RenderedPage {
                png: read_output(rendered.path())?,
                report: Some(rendered.report),
            })
        },
    )?;
    prepared.receipt["translated"] = json!(prepared.included());
    prepared.receipt["source_language"] = json!(language.code());
    prepared.receipt["target_language"] = json!("en");
    Ok(prepared)
}

impl Prepared {
    pub(crate) fn included(&self) -> usize {
        self.receipt["pages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|page| page["ok"] == true)
            .count()
    }
}

pub(crate) fn prepare_upscale(
    input: Input,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    operation: crate::UpscaleOperation,
    cancel: &Cancellation,
) -> Result<Prepared> {
    let runtime = crate::upscale_components::resolve(root, operation.weights())?;
    let staging = root.join("upscale-work");
    fs::create_dir_all(&staging)?;
    let name = format!(
        "{}-{}x.bull",
        operation.weights().trim_end_matches(".pth"),
        operation.factor()
    );
    let mut prepared = transform(
        input,
        root,
        quota,
        references,
        (&name, crate::image_collection::SOURCE),
        cancel,
        |bytes| {
            let output = crate::image_upscale::upscale(
                bytes,
                &staging,
                &runtime.python,
                &runtime.model,
                operation.factor(),
                cancel,
            )?;
            Ok(RenderedPage {
                png: read_output(output.path())?,
                report: None,
            })
        },
    )?;
    prepared.receipt["upscaled"] = json!(prepared.included());
    prepared.receipt["operation"] = json!(operation);
    Ok(prepared)
}

fn read_output(path: &Path) -> Result<Vec<u8>> {
    if fs::metadata(path)?.len() > MAX_PAGE_BYTES as u64 {
        return Err(ApiError::new(
            "collection_size_limit",
            "This page exceeds the collection size limit",
        ));
    }
    Ok(fs::read(path)?)
}

enum Page {
    File(Box<Event>),
    Embedded { bytes: Vec<u8>, already_png: bool },
}

struct RenderedPage {
    png: Vec<u8>,
    report: Option<RenderReport>,
}

fn read_bull(event: &Event) -> Result<Vec<Page>> {
    use crate::collection_kind::{self, Kind};
    let kind = collection_kind::probe(event)?;
    let file = File::open(&event.path)?;
    let bytes = match kind {
        Kind::Images => baloh_collection::codec::read(file)?,
        Kind::Files(ref family) if family == "images" => collection_kind::read_raw(file, &kind)?
            .into_iter()
            .map(|entry| entry.bytes)
            .collect(),
        _ => return Err(ApiError::new("invalid_image", "Select a BULL of images")),
    };
    let already_png = kind == Kind::Images;
    Ok(bytes
        .into_iter()
        .map(|bytes| Page::Embedded { bytes, already_png })
        .collect())
}

fn transform(
    input: Input,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    (name, source): (&str, &str),
    cancel: &Cancellation,
    mut render: impl FnMut(&[u8]) -> Result<RenderedPage>,
) -> Result<Prepared> {
    cancel.check()?;
    let preserve_pages =
        source == "image_translation_collection" && matches!(&input, Input::Bull(_));
    let mut validated_sources = Vec::new();
    let pages = match input {
        Input::Selection(events) => events
            .into_iter()
            .map(|event| Page::File(Box::new(event)))
            .collect(),
        Input::Bull(event) => {
            let stamp = media_service::stamp(&event.path)?;
            crate::open_file::validate(&event)?;
            let pages = read_bull(&event)?;
            check_stamp(&event, &stamp)?;
            validated_sources.push((*event, stamp));
            pages
        }
    };
    let total = pages.len();
    let mut included = Vec::with_capacity(total);
    let mut receipt = Vec::with_capacity(total);
    let mut bytes_count = 16usize;
    let mut succeeded = 0;
    cancel.report_progress(0, total as u32);
    for (index, page) in pages.into_iter().enumerate() {
        cancel.check()?;
        let mut fallback = None;
        let result: Result<_> = (|| {
            let (bytes, source, already_png) = match page {
                Page::Embedded { bytes, already_png } => (bytes, None, already_png),
                Page::File(event) => {
                    let stamp = media_service::stamp(&event.path)?;
                    let path = crate::wallpaper_engine::validate(root, &event)?;
                    let mut bytes = Vec::new();
                    File::open(&path)?
                        .take(MAX_SOURCE as u64 + 1)
                        .read_to_end(&mut bytes)?;
                    (bytes, Some((*event, stamp)), false)
                }
            };
            let rendered = (|| {
                if bytes.len() > MAX_SOURCE {
                    return Err(ApiError::new("invalid_image", "Page exceeds 32 MiB"));
                }
                if let Some((event, stamp)) = &source {
                    check_stamp(event, stamp)?;
                }
                let page = render(&bytes)?;
                let new_total = page_budget(bytes_count, page.png.len(), quota)?;
                if let Some((event, stamp)) = &source {
                    check_stamp(event, stamp)?;
                }
                Ok((page, new_total, source))
            })();
            if preserve_pages && rendered.is_err() {
                fallback = Some((bytes, already_png));
            }
            rendered
        })();
        cancel.check()?;
        match result {
            Ok((page, new_total, source)) => {
                succeeded += 1;
                bytes_count = new_total;
                if let Some(source) = source {
                    validated_sources.push(source);
                }
                let mut outcome = json!({"index":index, "ok":true, "page":included.len()});
                if let Some(report) = page.report {
                    outcome["rendered_blocks"] = json!(report.rendered);
                    outcome["skipped_blocks"] = json!(report.skipped);
                    outcome["fallback_blocks"] = json!(report.fallback);
                    outcome["side_blocks"] = json!(report.side);
                }
                receipt.push(outcome);
                included.push(page.png);
            }
            Err(error) => {
                let mut outcome = json!({"index":index, "ok":false,
                    "error":{"code":error.code,"message":error.message}});
                if let Some((bytes, already_png)) = fallback {
                    let png = if already_png {
                        bytes
                    } else {
                        crate::image_collection::png_page(&bytes).map_err(|failure| {
                            ApiError::new(
                                "collection_original_unavailable",
                                format!(
                                    "Page {} could not be kept in the result: {}",
                                    index + 1,
                                    failure.message
                                ),
                            )
                        })?
                    };
                    bytes_count =
                        page_budget(bytes_count, png.len(), quota).map_err(|failure| {
                            ApiError::new(
                                &failure.code,
                                format!(
                                    "Page {} could not be kept in the result: {}",
                                    index + 1,
                                    failure.message
                                ),
                            )
                        })?;
                    outcome["page"] = json!(included.len());
                    outcome["preserved_original"] = json!(true);
                    included.push(png);
                }
                receipt.push(outcome);
            }
        }
        cancel.report_progress((index + 1) as u32, total as u32);
    }
    if succeeded == 0 {
        let detail = receipt
            .iter()
            .filter_map(|page| {
                let index = page["index"].as_u64()?;
                let message = page["error"]["message"].as_str()?;
                Some(format!("#{} — {message}", index + 1))
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Err(ApiError::new("collection_empty", detail));
    }
    let mut portable = Vec::with_capacity(bytes_count);
    baloh_collection::codec::write(&mut portable, &included)?;
    drop(included);
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name,
            source,
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )?;
    cancel.check()?;
    Ok(Prepared {
        blob,
        validated_sources,
        receipt: json!({"collection":true, "total":total, "pages":receipt}),
    })
}

fn page_budget(current: usize, len: usize, quota: u64) -> Result<usize> {
    let total = current.saturating_add(36).saturating_add(len);
    if len > MAX_PAGE_BYTES || total > MAX_TOTAL_BYTES || total as u64 > quota {
        return Err(ApiError::new(
            "collection_size_limit",
            "This page exceeds the collection size limit",
        ));
    }
    Ok(total)
}

fn check_stamp(event: &Event, stamp: &media_service::Stamp) -> Result<()> {
    if media_service::stamp(&event.path)? != *stamp {
        return Err(ApiError::new(
            "source_changed",
            "Image or collection changed during processing",
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Command, Service,
        domain_jobs::{DomainJobs, Lane},
    };
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn batch_retains_render_report_without_marking_the_page_failed() {
        let profile = tempfile::tempdir().unwrap();
        let root = profile.path().to_owned();
        let mut service = Service::open(&root, crate::DEFAULT_QUOTA).unwrap();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        let mut archive = Vec::new();
        baloh_collection::codec::write(&mut archive, &[png.clone(), png]).unwrap();
        let source = service
            .dispatch(Command::IngestBytes {
                name: "fixture.bull".into(),
                bytes: archive,
                source: "synthetic".into(),
            })
            .unwrap();
        let event = service.get(source["id"].as_i64().unwrap()).unwrap();
        let (send, receive) = mpsc::channel();
        let mut jobs = DomainJobs::<()>::new();
        jobs.submit(Lane::ImageTranslate, move |cancel| {
            let mut skipped = 3;
            let prepared = transform(
                Input::Bull(Box::new(event)),
                &root,
                crate::DEFAULT_QUOTA,
                &[],
                ("translated.bull", "image_translation_collection"),
                &cancel,
                |bytes| {
                    let report = RenderReport {
                        rendered: 2,
                        skipped,
                        fallback: 1,
                        side: 1,
                        left_pad: 0,
                    };
                    skipped = 0;
                    Ok(RenderedPage {
                        png: bytes.to_vec(),
                        report: Some(report),
                    })
                },
            );
            send.send(prepared).unwrap();
            Ok(())
        })
        .unwrap();
        let prepared = receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(prepared.included(), 2);
        assert_eq!(
            prepared.receipt["pages"],
            json!([
                {"index":0,"ok":true,"page":0,"rendered_blocks":2,"skipped_blocks":3,
                    "fallback_blocks":1,"side_blocks":1},
                {"index":1,"ok":true,"page":1,"rendered_blocks":2,"skipped_blocks":0,
                    "fallback_blocks":1,"side_blocks":1}
            ])
        );
    }
}

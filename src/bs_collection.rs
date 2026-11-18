//! Portable BS file collections. Source bytes are never modified.
use crate::{
    ApiError, Event, EventKind, Result, domain_jobs::Cancellation, interaction::ShelfTab,
    media_service, open_file, origin::Origin, storage,
};
use baloh_collection::bs::{self, FileEntry, MAX_FILE_BYTES, MAX_FILES, MAX_TOTAL_BYTES};
use base64::Engine;
use image::ImageDecoder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

pub(crate) const SOURCE: &str = "bs_collection";
const EXTRACTED: &str = "bs_extracted_file";

struct CancellableReader<'a, R> {
    inner: R,
    cancel: &'a Cancellation,
    hash: Sha256,
}
impl<R: Read> Read for CancellableReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("Cancelled"));
        }
        let count = self.inner.read(buffer)?;
        self.hash.update(&buffer[..count]);
        Ok(count)
    }
}

fn collection_extension(name: &Path) -> bool {
    name.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bs") || ext.eq_ignore_ascii_case("bull"))
}

pub(crate) fn is_managed(event: &Event) -> bool {
    event.storage == "cached" && event.source == SOURCE && bs_name(event)
}

fn bs_name(event: &Event) -> bool {
    Path::new(&event.name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bs"))
}

pub(crate) fn can_view(event: &Event) -> bool {
    is_managed(event) || (event.storage == "reference" && event.source == "files" && bs_name(event))
}

pub(crate) fn is_packable_file(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::FileReference | EventKind::ManagedImage
    ) && !collection_extension(Path::new(&event.name))
        && !collection_extension(&event.path)
}

fn no_nested(entries: &[FileEntry]) -> Result<()> {
    if entries
        .iter()
        .any(|entry| collection_extension(Path::new(&entry.name)))
    {
        return Err(ApiError::new(
            "invalid_collection",
            "BS cannot contain nested BS or BULL collections",
        ));
    }
    Ok(())
}

fn read_collection(event: &Event, cancel: &Cancellation) -> Result<Vec<FileEntry>> {
    cancel.check()?;
    if !can_view(event) {
        return Err(ApiError::new(
            "invalid_collection",
            "Expected a saved BS collection",
        ));
    }
    let stamp = media_service::stamp(&event.path)?;
    let file = File::open(&event.path)?;
    if file.metadata()?.len() > MAX_TOTAL_BYTES as u64 {
        return Err(ApiError::new(
            "collection_size_limit",
            "BS exceeds the collection size limit",
        ));
    }
    let mut input = CancellableReader {
        inner: file,
        cancel,
        hash: Sha256::new(),
    };
    let entries = match bs::read(&mut input) {
        Ok(entries) => entries,
        Err(error) => {
            cancel.check()?;
            return Err(error.into());
        }
    };
    cancel.check()?;
    no_nested(&entries)?;
    let prefix = if event.storage == "cached" {
        "blob:"
    } else {
        "ref:"
    };
    if event.revision != format!("{prefix}{:x}", input.hash.finalize())
        || media_service::stamp(&event.path)? != stamp
    {
        return Err(ApiError::new("source_changed", "BS changed while reading"));
    }
    Ok(entries)
}

fn read_regular(event: &Event, cancel: &Cancellation) -> Result<(FileEntry, media_service::Stamp)> {
    if !is_packable_file(event) {
        return Err(ApiError::new(
            "invalid_selection",
            "Select regular files, without nested collections",
        ));
    }
    let stamp = media_service::stamp(&event.path)?;
    let mut input = File::open(&event.path)?;
    if input.metadata()?.len() > MAX_FILE_BYTES as u64 {
        return Err(ApiError::new(
            "collection_size_limit",
            "File exceeds the BS per-file limit",
        ));
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        cancel.check()?;
        let n = input.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        if bytes.len().saturating_add(n) > MAX_FILE_BYTES as usize {
            return Err(ApiError::new(
                "collection_size_limit",
                "File exceeds the BS per-file limit",
            ));
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    let expected = event
        .revision
        .strip_prefix("blob:")
        .or_else(|| event.revision.strip_prefix("ref:"))
        .ok_or_else(|| ApiError::new("stale_selection", "File has no verified content revision"))?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected
        || media_service::stamp(&event.path)? != stamp
    {
        return Err(ApiError::new(
            "source_changed",
            "File changed while reading",
        ));
    }
    Ok((
        FileEntry {
            name: event.name.clone(),
            bytes,
        },
        stamp,
    ))
}

pub(crate) fn pack(
    events: Vec<Event>,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<storage::CollectionPack> {
    if events.is_empty() {
        return Err(ApiError::new(
            "invalid_selection",
            "Select files or BS collections",
        ));
    }
    let merge = is_managed(&events[0]);
    if events.iter().any(|event| {
        if merge {
            !is_managed(event)
        } else {
            !is_packable_file(event)
        }
    }) {
        return Err(ApiError::new(
            "invalid_selection",
            "Select either regular files or BS collections",
        ));
    }
    let mut entries = Vec::new();
    let mut inputs = Vec::with_capacity(events.len());
    let mut bytes_count: u64 = 16;
    for (index, event) in events.into_iter().enumerate() {
        cancel.check()?;
        let result = (|| -> Result<_> {
            if merge {
                let stamp = media_service::stamp(&event.path)?;
                let entries = read_collection(&event, cancel)?;
                if media_service::stamp(&event.path)? != stamp {
                    return Err(ApiError::new("source_changed", "BS changed while reading"));
                }
                Ok((entries, stamp))
            } else {
                let (entry, stamp) = read_regular(&event, cancel)?;
                Ok((vec![entry], stamp))
            }
        })();
        let (items, stamp) = result
            .map_err(|e| ApiError::new(&e.code, format!("Item {} · {}", index + 1, e.message)))?;
        let offset = entries.len();
        if offset.saturating_add(items.len()) > MAX_FILES as usize {
            return Err(ApiError::new(
                "collection_size_limit",
                "Too many files for BS",
            ));
        }
        no_nested(&items)?;
        for item in &items {
            bytes_count = bytes_count
                .checked_add(item.bytes.len() as u64)
                .and_then(|n| n.checked_add(item.name.len() as u64))
                .and_then(|n| n.checked_add(40))
                .ok_or_else(|| ApiError::new("collection_size_limit", "BS size overflow"))?;
            if item.bytes.len() > MAX_FILE_BYTES as usize
                || bytes_count > MAX_TOTAL_BYTES as u64
                || bytes_count > quota
            {
                return Err(ApiError::new(
                    "collection_size_limit",
                    "BS exceeds the collection or cache size limit",
                ));
            }
        }
        inputs.push(storage::CollectionInput {
            event,
            stamp,
            offset,
            count: items.len(),
        });
        entries.extend(items);
    }
    cancel.check()?;
    let mut portable = Vec::new();
    bs::write(&mut portable, &entries)?;
    if portable.len() > MAX_TOTAL_BYTES as usize || portable.len() as u64 > quota {
        return Err(ApiError::new(
            "collection_size_limit",
            "BS exceeds the collection or cache size limit",
        ));
    }
    cancel.check()?;
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: "Files collection.bs",
            source: SOURCE,
            bytes: &portable,
            origin: &Origin::default(),
        },
        references,
    )?;
    Ok(storage::CollectionPack {
        inputs,
        blob,
        tab: ShelfTab::Files,
        count: entries.len(),
    })
}

pub(crate) fn unpack(
    event: &Event,
    members: &storage::CollectionMembers,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Vec<Option<storage::PreparedBlob>>> {
    let entries = read_collection(event, cancel)?;
    if members
        .last_key_value()
        .is_some_and(|(&position, _)| position >= entries.len())
    {
        return Err(ApiError::new(
            "invalid_collection",
            "Saved collection membership is out of range",
        ));
    }
    let mut result = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        cancel.check()?;
        if members.contains_key(&index) {
            result.push(None);
            continue;
        }
        result.push(Some(storage::prepare_bytes(
            root,
            quota,
            storage::ByteIngest {
                name: &entry.name,
                source: EXTRACTED,
                bytes: &entry.bytes,
                origin: &Origin::default(),
            },
            references,
        )?));
    }
    cancel.check()?;
    open_file::validate(event)?;
    Ok(result)
}

fn selected(event: &Event, index: usize, cancel: &Cancellation) -> Result<(FileEntry, usize)> {
    let entries = read_collection(event, cancel)?;
    let count = entries.len();
    let entry = entries
        .into_iter()
        .nth(index)
        .ok_or_else(|| ApiError::new("invalid_page", "File is outside the BS collection"))?;
    Ok((entry, count))
}
pub(crate) fn read_entry(event: &Event, index: usize, cancel: &Cancellation) -> Result<FileEntry> {
    Ok(selected(event, index, cancel)?.0)
}

fn thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() > 16 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::WebP
                | image::ImageFormat::Bmp
                | image::ImageFormat::Gif
        )
    ) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(48 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 8_000_000 {
        return None;
    }
    let image = image::DynamicImage::from_decoder(decoder)
        .ok()?
        .thumbnail(320, 320);
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).ok()?;
    (output.get_ref().len() <= 1024 * 1024).then(|| output.into_inner())
}

pub(crate) fn page(event: &Event, index: usize, cancel: &Cancellation) -> Result<Value> {
    let (entry, count) = selected(event, index, cancel)?;
    cancel.check()?;
    let preview_png =
        thumbnail(&entry.bytes).map(|png| base64::engine::general_purpose::STANDARD.encode(png));
    cancel.check()?;
    let extension = Path::new(&entry.name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(
        json!({"index":index,"count":count,"file":{"name":entry.name,"size":entry.bytes.len(),"extension":extension,"preview_png":preview_png}}),
    )
}

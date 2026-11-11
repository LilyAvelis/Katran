//! Managed copies of user-selected theme backgrounds. No source is modified.
use crate::{ApiError, Result};
use image::{ImageDecoder, ImageFormat, ImageReader};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

const MAX_INPUT: u64 = 32 * 1024 * 1024;
const MAX_STORED: u64 = 96 * 1024 * 1024;
const MAX_PIXELS: u64 = 32_000_000;
const MAX_ALLOC: u64 = 128 * 1024 * 1024;
const MAX_EDGE: u32 = 4096;
const MAX_ITEMS: usize = 64;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn safe_entry(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
fn directory(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let path = root.join("theme-backgrounds");
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_dir()
        || !safe_entry(&metadata)
        || path.canonicalize()?.parent() != Some(root.as_path())
    {
        return Err(ApiError::new(
            "unsafe_background",
            "Background directory is not a managed directory",
        ));
    }
    Ok(path)
}
fn checked_file(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || !safe_entry(&metadata) {
        return Err(ApiError::new(
            "unsafe_background",
            "Background is not a regular managed file",
        ));
    }
    Ok(metadata)
}
fn name_for(path: &Path) -> String {
    let name: String = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("Фон")
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect();
    if name.trim().is_empty() {
        "Фон".into()
    } else {
        name
    }
}
fn image_bytes(source: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(source)?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT {
        return Err(ApiError::new(
            "invalid_background",
            "Image must be a file no larger than 32 MiB",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(source)?
        .take(MAX_INPUT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err(ApiError::new("invalid_background", "Image exceeds 32 MiB"));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ApiError::new("invalid_background", e))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| ApiError::new("invalid_background", e))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ApiError::new(
            "invalid_background",
            "Image exceeds 32 million pixels",
        ));
    }
    let orientation = decoder
        .orientation()
        .map_err(|e| ApiError::new("invalid_background", e))?;
    let mut image = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| ApiError::new("invalid_background", e))?;
    image.apply_orientation(orientation);
    let image = if width.max(height) > MAX_EDGE {
        image.resize(MAX_EDGE, MAX_EDGE, image::imageops::FilterType::Lanczos3)
    } else {
        image
    };
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|e| ApiError::new("invalid_background", e))?;
    let bytes = png.into_inner();
    if bytes.len() as u64 > MAX_STORED {
        return Err(ApiError::new(
            "invalid_background",
            "Normalized image exceeds storage limit",
        ));
    }
    Ok(bytes)
}
fn entries(dir: &Path) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".png")) else {
            continue;
        };
        if valid_id(id) {
            checked_file(&entry.path())?;
            ids.push(id.to_owned());
        }
    }
    ids.sort();
    Ok(ids)
}

/// Prepared independently of the destination; owns all data needed for commit.
/// This value is Send and contains no borrowed source file or handle.
pub(crate) struct PreparedBackground {
    png: Vec<u8>,
    id: String,
    name: String,
}

/// Perform potentially expensive source IO, decode, resize, encode and hashing
/// without creating or modifying the destination directory.
pub(crate) fn prepare(source: &Path) -> Result<PreparedBackground> {
    let png = image_bytes(source)?;
    let id = format!("{:x}", Sha256::digest(&png));
    Ok(PreparedBackground {
        png,
        id,
        name: name_for(source),
    })
}

/// A stable ID always identifies the normalized PNG bytes, never the source path.
pub fn import(root: &Path, source: &Path) -> Result<Value> {
    let prepared = prepare(source)?;
    commit(root, prepared)
}

/// Recheck storage and publish the already prepared immutable image atomically.
pub(crate) fn commit(root: &Path, prepared: PreparedBackground) -> Result<Value> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| ApiError::new("background_busy", "Background import lock poisoned"))?;
    let PreparedBackground { png, id, name } = prepared;
    let dir = directory(root)?;
    let destination = dir.join(format!("{id}.png"));
    match fs::symlink_metadata(&destination) {
        Ok(_) => return get(root, &id),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if entries(&dir)?.len() >= MAX_ITEMS {
        return Err(ApiError::new(
            "background_limit",
            "At most 64 backgrounds can be stored",
        ));
    }
    let mut stage = None;
    for _ in 0..100 {
        let path = dir.join(format!(
            ".import-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                stage = Some((path, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    let (stage, mut file) = stage.ok_or_else(|| {
        ApiError::new("background_busy", "Cannot allocate a temporary background")
    })?;
    let result = (|| -> Result<()> {
        file.write_all(&png)?;
        file.sync_all()?;
        drop(file);
        match fs::hard_link(&stage, &destination) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                checked_file(&destination)?;
            }
            Err(e) => return Err(e.into()),
        }
        Ok(())
    })();
    let _ = fs::remove_file(&stage);
    result?;
    // Optional display metadata is separate from identity. Never replace it.
    let sidecar = dir.join(format!("{id}.json"));
    if let Ok(mut file) = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(sidecar)
    {
        let _ = file.write_all(serde_json::json!({"name":name}).to_string().as_bytes());
        let _ = file.sync_all();
    }
    get(root, &id)
}

pub fn get(root: &Path, id: &str) -> Result<Value> {
    if !valid_id(id) {
        return Err(ApiError::new("invalid_background", "Invalid background ID"));
    }
    let dir = directory(root)?;
    let path = dir.join(format!("{id}.png"));
    let metadata = checked_file(&path)?;
    if metadata.len() > MAX_STORED {
        return Err(ApiError::new(
            "invalid_background",
            "Stored background exceeds limit",
        ));
    }
    let mut file = fs::File::open(&path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > MAX_STORED {
            return Err(ApiError::new(
                "invalid_background",
                "Stored background exceeds limit",
            ));
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != id {
        return Err(ApiError::new(
            "invalid_background",
            "Stored background failed verification",
        ));
    }
    let reader = ImageReader::open(&path)?
        .with_guessed_format()
        .map_err(|e| ApiError::new("invalid_background", e))?;
    if reader.format() != Some(ImageFormat::Png) {
        return Err(ApiError::new(
            "invalid_background",
            "Stored background is not PNG",
        ));
    }
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| ApiError::new("invalid_background", e))?;
    if width == 0 || height == 0 || width.max(height) > MAX_EDGE {
        return Err(ApiError::new(
            "invalid_background",
            "Stored background dimensions are invalid",
        ));
    }
    let sidecar = dir.join(format!("{id}.json"));
    let name = match fs::symlink_metadata(&sidecar) {
        Ok(_) => {
            let metadata = checked_file(&sidecar)?;
            if metadata.len() > 4096 {
                return Err(ApiError::new(
                    "invalid_background",
                    "Background metadata exceeds limit",
                ));
            }
            let mut bytes = Vec::new();
            fs::File::open(sidecar)?
                .take(4097)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 4096 {
                return Err(ApiError::new(
                    "invalid_background",
                    "Background metadata exceeds limit",
                ));
            }
            let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            value["name"]
                .as_str()
                .filter(|s| {
                    !s.is_empty() && s.chars().count() <= 255 && !s.chars().any(char::is_control)
                })
                .unwrap_or("Фон")
                .to_owned()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "Фон".into(),
        Err(e) => return Err(e.into()),
    };
    Ok(json!({"id":id,"path":path,"name":name,"width":width,"height":height}))
}
pub fn list(root: &Path) -> Result<Value> {
    let dir = directory(root)?;
    let mut backgrounds = Vec::new();
    for id in entries(&dir)? {
        backgrounds.push(get(root, &id)?);
    }
    Ok(json!({"backgrounds":backgrounds}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_inputs_dedup_and_original_independence() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = outside.path().join("образец.png");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([30, 40, 50, 255]))
            .save(&source)
            .unwrap();
        let one = import(root.path(), &source).unwrap();
        assert_eq!(one["name"], "образец.png");
        assert_eq!(one["width"], 4);
        assert_eq!(one["height"], 3);
        let again = import(root.path(), &source).unwrap();
        assert_eq!(one["id"], again["id"]);
        assert_eq!(
            list(root.path()).unwrap()["backgrounds"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        fs::rename(&source, outside.path().join("moved.png")).unwrap();
        assert_eq!(get(root.path(), one["id"].as_str().unwrap()).unwrap(), one);
        assert!(get(root.path(), "../outside").is_err());
        let bad = outside.path().join("bad.png");
        fs::write(&bad, b"not an image").unwrap();
        assert!(import(root.path(), &bad).is_err());
        assert_eq!(fs::read(bad).unwrap(), b"not an image");
        let big = outside.path().join("too-big.png");
        fs::File::create(&big)
            .unwrap()
            .set_len(MAX_INPUT + 1)
            .unwrap();
        assert_eq!(
            import(root.path(), &big).unwrap_err().code,
            "invalid_background"
        );
        assert_eq!(fs::metadata(big).unwrap().len(), MAX_INPUT + 1);
        let managed = PathBuf::from(one["path"].as_str().unwrap());
        fs::write(&managed, b"tampered synthetic data").unwrap();
        assert_eq!(
            get(root.path(), one["id"].as_str().unwrap())
                .unwrap_err()
                .code,
            "invalid_background"
        );
    }
    #[test]
    fn preparation_is_source_independent_and_writes_only_on_commit() {
        fn assert_send<T: Send>() {}
        assert_send::<PreparedBackground>();
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = outside.path().join("synthetic.png");
        image::RgbaImage::from_pixel(5, 3, image::Rgba([2, 7, 12, 255]))
            .save(&source)
            .unwrap();
        let prepared = prepare(&source).unwrap();
        assert!(!root.path().join("theme-backgrounds").exists());
        fs::remove_file(&source).unwrap();
        let value = commit(root.path(), prepared).unwrap();
        assert_eq!(value["name"], "synthetic.png");
        assert_eq!(value["width"], 5);
        assert_eq!(value["height"], 3);
        assert_eq!(
            get(root.path(), value["id"].as_str().unwrap()).unwrap(),
            value
        );
    }
    #[test]
    fn jpeg_exif_rotation_is_applied_before_normalization() {
        let root = tempfile::tempdir().unwrap();
        let source_root = tempfile::tempdir().unwrap();
        let mut pixels = image::RgbImage::new(40, 20);
        for (x, _, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = if x < 20 {
                image::Rgb([240, 15, 15])
            } else {
                image::Rgb([15, 15, 240])
            };
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
            .encode_image(&pixels)
            .unwrap();
        for (orientation, top_red) in [(6u8, true), (8u8, false)] {
            // Minimal big-endian EXIF TIFF with exactly the orientation tag.
            let exif = [
                b'E',
                b'x',
                b'i',
                b'f',
                0,
                0,
                b'M',
                b'M',
                0,
                42,
                0,
                0,
                0,
                8,
                0,
                1,
                1,
                18,
                0,
                3,
                0,
                0,
                0,
                1,
                0,
                orientation,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            let length = (exif.len() + 2) as u16;
            let mut encoded = vec![0xff, 0xd8, 0xff, 0xe1, (length >> 8) as u8, length as u8];
            encoded.extend_from_slice(&exif);
            encoded.extend_from_slice(&jpeg[2..]);
            let path = source_root
                .path()
                .join(format!("orientation-{orientation}.jpg"));
            fs::write(&path, &encoded).unwrap();
            let result = import(root.path(), &path).unwrap();
            assert_eq!(result["width"], 20);
            assert_eq!(result["height"], 40);
            let image = image::open(result["path"].as_str().unwrap())
                .unwrap()
                .to_rgb8();
            let upper = image.get_pixel(10, 5);
            let lower = image.get_pixel(10, 35);
            if top_red {
                assert!(upper[0] > upper[2] && lower[2] > lower[0]);
            } else {
                assert!(upper[2] > upper[0] && lower[0] > lower[2]);
            }
            assert_eq!(fs::read(path).unwrap(), encoded);
        }
    }
    #[cfg(unix)]
    #[test]
    fn managed_directory_symlink_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(target.path(), root.path().join("theme-backgrounds")).unwrap();
        assert!(list(root.path()).is_err());
    }
}

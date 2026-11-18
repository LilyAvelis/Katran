//! Preparation of a captured frame using the normal managed blob staging path.
use crate::{
    ApiError, Result, domain_jobs::Cancellation, media_playback::CaptureTask, origin::Origin,
    storage,
};
use image::ImageEncoder;
use std::path::{Path, PathBuf};

pub(crate) struct PreparedCapture {
    pub blob: storage::PreparedBlob,
    pub width: u32,
    pub height: u32,
    pub position_ms: u64,
}
pub(crate) fn prepare(
    task: Box<dyn CaptureTask>,
    cancel: &Cancellation,
    root: &Path,
    quota: u64,
    references: &[PathBuf],
) -> Result<PreparedCapture> {
    let frame = task.wait(cancel)?;
    cancel.check()?;
    let pixels = (frame.width as usize).checked_mul(frame.height as usize);
    if frame.width == 0
        || frame.height == 0
        || pixels.is_none_or(|n| n > 64_000_000)
        || pixels.and_then(|n| n.checked_mul(4)) != Some(frame.rgba.len())
    {
        return Err(ApiError::new(
            "invalid_media_frame",
            "Captured video frame is invalid or too large",
        ));
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &frame.rgba,
            frame.width,
            frame.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| ApiError::new("media_capture_encode", error))?;
    cancel.check()?;
    let blob = storage::prepare_bytes(
        root,
        quota,
        storage::ByteIngest {
            name: "video-frame.png",
            source: "video_frame",
            bytes: &png,
            origin: &Origin::default(),
        },
        references,
    )?;
    cancel.check()?;
    Ok(PreparedCapture {
        blob,
        width: frame.width,
        height: frame.height,
        position_ms: frame.position_ms,
    })
}

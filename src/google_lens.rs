//! Explicit, bounded image search. Browser receives only a verified result URL.
use crate::{ApiError, Event, Result, domain_jobs::Cancellation};
use image::ImageDecoder;
use reqwest::{blocking::Client, header::LOCATION, redirect::Policy};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Cursor, Read},
    time::Duration,
};
use url::Url;

const ENDPOINT: &str = "https://lens.google.com/v3/upload";
const MAX_INPUT: u64 = 32 * 1024 * 1024;
const MAX_EDGE: u32 = 2048;

fn error(message: impl ToString) -> ApiError {
    ApiError::new("google_lens_error", message)
}

fn read_selected(event: &Event) -> Result<Vec<u8>> {
    let stamp = crate::media_service::stamp(&event.path)?;
    let mut file = File::open(&event.path)?;
    if file.metadata()?.len() > MAX_INPUT {
        return Err(error("Google Lens accepts a source image up to 32 MiB"));
    }
    let mut bytes = Vec::new();
    file.by_ref().take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err(error("Google Lens accepts a source image up to 32 MiB"));
    }
    let current = match event.revision.split_once(':') {
        Some(("ref" | "blob", expected)) => format!("{:x}", Sha256::digest(&bytes)) == expected,
        _ => crate::preview::revision(&event.path, None) == event.revision,
    };
    if !current || stamp != crate::media_service::stamp(&event.path)? {
        return Err(error("Image changed since selection"));
    }
    Ok(bytes)
}

fn upload_image(input: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let format = image::guess_format(input).map_err(|_| error("Select a readable image"))?;
    let mut reader = image::ImageReader::with_format(Cursor::new(input), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| error("Select a readable image"))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 32_000_000 {
        return Err(error("Image dimensions exceed the Google Lens limit"));
    }
    let orientation = decoder
        .orientation()
        .map_err(|_| error("Could not read the image orientation"))?;
    let mut decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|_| error("Select a readable image"))?;
    decoded.apply_orientation(orientation);
    let decoded = if width > MAX_EDGE || height > MAX_EDGE {
        decoded.thumbnail(MAX_EDGE, MAX_EDGE)
    } else {
        decoded
    };
    // Re-encode just the visible pixels. Transparent pixels are composited onto white.
    let rgba = decoded.into_rgba8();
    let rgb =
        image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let [red, green, blue, alpha] = rgba.get_pixel(x, y).0;
            let alpha = u32::from(alpha);
            image::Rgb([red, green, blue].map(|channel| {
                ((u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
            }))
        });
    let (width, height) = rgb.dimensions();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&rgb)
        .map_err(error)?;
    Ok((jpeg, width, height))
}

fn result_url(endpoint: &Url, location: &str) -> Result<Url> {
    let url = endpoint
        .join(location)
        .map_err(|_| error("Google Lens returned an invalid result address"))?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("www.google.com" | "lens.google.com"))
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/search"
        || url.query().is_none_or(str::is_empty)
        || url.fragment().is_some()
        || url.as_str().len() > 24_000
    {
        return Err(error(
            "Google Lens did not return a supported search result",
        ));
    }
    Ok(url)
}

fn multipart(jpeg: &[u8]) -> Result<(String, Vec<u8>)> {
    let hash = format!("{:x}", Sha256::digest(jpeg));
    let mut boundary = format!("katran-{}", &hash[..32]);
    while jpeg
        .windows(boundary.len())
        .any(|window| window == boundary.as_bytes())
    {
        if boundary.len() >= 70 {
            return Err(error("Could not encode the Google Lens request"));
        }
        boundary.push('x');
    }
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"encoded_image\"; filename=\"image.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n"
    ).into_bytes();
    body.extend_from_slice(jpeg);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok((format!("multipart/form-data; boundary={boundary}"), body))
}

pub(crate) fn search(event: &Event, language: &str, cancel: &Cancellation) -> Result<Url> {
    cancel.check()?;
    let (jpeg, width, height) = upload_image(&read_selected(event)?)?;
    let mut endpoint = Url::parse(ENDPOINT).expect("valid Lens upload endpoint");
    endpoint
        .query_pairs_mut()
        .extend_pairs([
            ("ep", "cntpubb"),
            ("hl", language),
            ("re", "df"),
            ("s", "4"),
        ])
        .append_pair("vpw", &width.to_string())
        .append_pair("vph", &height.to_string());
    let client = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|_| error("Could not create a connection to Google Lens"))?;
    let (content_type, body) = multipart(&jpeg)?;
    cancel.begin_effect()?;
    let response = client
        .post(endpoint.clone())
        .header("content-type", content_type)
        .body(body)
        .send()
        .map_err(|_| error("Could not reach Google Lens"))?;
    if !matches!(response.status().as_u16(), 302 | 303) {
        return Err(error(format!(
            "Google Lens did not accept the image (HTTP {})",
            response.status().as_u16()
        )));
    }
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|header| header.to_str().ok())
        .ok_or_else(|| error("Google Lens returned no search result address"))?;
    result_url(&endpoint, location)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_https_google_search_redirects() {
        let endpoint = Url::parse(ENDPOINT).unwrap();
        assert!(result_url(&endpoint, "https://www.google.com/search?vsrid=synthetic").is_ok());
        assert!(result_url(&endpoint, "/search?p=synthetic").is_ok());
        for target in [
            "https://www.google.com.evil.test/search?p=x",
            "https://www.google.com:444/search?p=x",
            "http://www.google.com/search?p=x",
            "https://name@www.google.com/search?p=x",
            "https://www.google.com/url?next=https://evil.test",
            "https://www.google.com/search",
        ] {
            assert!(result_url(&endpoint, target).is_err(), "{target}");
        }
    }

    #[test]
    fn sends_only_normalized_visible_pixels_under_a_generic_name() {
        let source = image::RgbaImage::from_pixel(10, 10, image::Rgba([100, 0, 50, 0]));
        let mut png = Cursor::new(Vec::new());
        source.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let (jpeg, width, height) = upload_image(png.get_ref()).unwrap();
        assert_eq!((width, height), (10, 10));
        assert_eq!(
            image::load_from_memory(&jpeg)
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [255; 3]
        );
        let (content_type, body) = multipart(&jpeg).unwrap();
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        assert!(
            String::from_utf8_lossy(&body)
                .contains("name=\"encoded_image\"; filename=\"image.jpg\"")
        );
        assert!(upload_image(b"not an image").is_err());
    }

    #[test]
    fn preserves_the_display_orientation_without_sending_exif() {
        use image::ImageEncoder;
        let mut input = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut input);
        // Little-endian TIFF with one orientation entry (6, rotate 90 degrees).
        encoder
            .set_exif_metadata(
                b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0".to_vec(),
            )
            .unwrap();
        encoder.encode_image(&image::RgbImage::new(10, 20)).unwrap();
        let (jpeg, width, height) = upload_image(&input).unwrap();
        assert_eq!((width, height), (20, 10));
        let mut decoder =
            image::ImageReader::with_format(Cursor::new(&jpeg), image::ImageFormat::Jpeg)
                .into_decoder()
                .unwrap();
        assert!(decoder.exif_metadata().unwrap().is_none());
    }
}

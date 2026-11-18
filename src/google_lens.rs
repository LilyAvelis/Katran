//! One-shot loopback handoff for a Google Lens upload in the default browser.
use crate::{ApiError, Event, Result, domain_jobs::Cancellation};
use base64::Engine;
use image::ImageDecoder;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};
use url::Url;

const ENDPOINT: &str = "https://lens.google.com/v3/upload";
const MAX_INPUT: u64 = 32 * 1024 * 1024;
const MAX_EDGE: u32 = 2048;
const HANDOFF: &str = include_str!("google_lens_handoff.html");

fn error(message: impl ToString) -> ApiError {
    ApiError::new("google_lens_error", message)
}

fn read_selected(event: &Event) -> Result<Vec<u8>> {
    let stamp = crate::media_service::stamp(&event.path)?;
    let file = File::open(&event.path)?;
    if file.metadata()?.len() > MAX_INPUT {
        return Err(error("Google Lens accepts a source image up to 32 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
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

fn upload_page(jpeg: &[u8], width: u32, height: u32, language: &str) -> String {
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
    HANDOFF
        .replace("{{ENDPOINT}}", &endpoint.as_str().replace('&', "&amp;"))
        .replace(
            "{{JPEG}}",
            &base64::engine::general_purpose::STANDARD.encode(jpeg),
        )
}

fn is_handoff_request(stream: &mut TcpStream, route: &str, host: &str) -> std::io::Result<bool> {
    let mut headers = [0u8; 8192];
    let mut used = 0;
    let deadline = Instant::now() + Duration::from_millis(500);
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    while used < headers.len() && Instant::now() < deadline {
        let count = stream.read(&mut headers[used..])?;
        if count == 0 {
            break;
        }
        used += count;
        if headers[..used].windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let Ok(request) = std::str::from_utf8(&headers[..used]) else {
                return Ok(false);
            };
            let mut lines = request.lines();
            let mut request_line = lines.next().unwrap_or_default().split_ascii_whitespace();
            return Ok(request_line.next() == Some("GET")
                && request_line.next() == Some(route)
                && matches!(request_line.next(), Some("HTTP/1.0" | "HTTP/1.1"))
                && lines.take_while(|line| !line.is_empty()).any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("host") && value.trim() == host
                    })
                }));
        }
    }
    Ok(false)
}

pub(crate) fn search(
    event: &Event,
    language: &str,
    open: impl FnOnce(&str) -> Result<()>,
    cancel: &Cancellation,
) -> Result<()> {
    cancel.check()?;
    let (jpeg, width, height) = upload_image(&read_selected(event)?)?;
    let page = upload_page(&jpeg, width, height, language);
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let host = listener.local_addr()?.to_string();
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| error("Could not create a private Lens handoff"))?;
    let route = format!(
        "/{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce)
    );
    cancel.begin_effect()?;
    open(&format!("http://{host}{route}"))?;

    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false)?;
                if !is_handoff_request(&mut stream, &route, &host).unwrap_or(false) {
                    continue;
                }
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; form-action https://lens.google.com https://www.google.com https://consent.google.com; base-uri 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
                    page.len(),
                )?;
                stream.write_all(page.as_bytes())?;
                return Ok(());
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(error(
        "The browser did not receive the image for Google Lens",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_form_posts_the_image_to_google_in_the_same_navigation() {
        let page = upload_page(b"image payload", 640, 440, "en");
        assert!(page.contains("https://lens.google.com/v3/upload?"));
        assert!(page.contains("method=\"post\" enctype=\"multipart/form-data\""));
        assert!(page.contains("name=\"encoded_image\""));
        assert!(page.contains("aW1hZ2UgcGF5bG9hZA=="));
        assert!(page.contains(".submit()"));
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

//! Opt-in favicon retrieval. Call only on a bounded worker, never on the UI thread.
//!
//! Resolve once, reject non-public answers and pin the accepted addresses while
//! retaining the hostname for HTTPS certificate validation. Local routing rules
//! outside this process can still route a public address to a private network.
use image::{ImageFormat, ImageReader};
use scraper::{Html, Selector};
use std::{
    io::{Cursor, Read},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use url::{Host, Url};

const MAX_BYTES: usize = 256 * 1024;
const MAX_HTML_BYTES: usize = 512 * 1024;
const MAX_EDGE: u32 = 1024;
const MAX_ALLOC: u64 = 8 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    InvalidUrl,
    UnsafeAddress,
    Network(String),
    TooLarge,
    Read(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl => f.write_str("invalid favicon URL"),
            Self::UnsafeAddress => f.write_str("unsafe favicon address"),
            Self::Network(message) => write!(f, "favicon network error: {message}"),
            Self::TooLarge => f.write_str("favicon too large"),
            Self::Read(message) => write!(f, "favicon read error: {message}"),
        }
    }
}

// Conservative allow policy: special-purpose, transitional and documentation
// networks are rejected even where some addresses may be globally reachable.
fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    // Accept global unicast only; reject IETF special assignments, 6to4,
    // documentation prefixes and their possible embedded private IPv4s.
    (segments[0] & 0xe000) == 0x2000
        && !(segments[0] == 0x2001 && (segments[1] <= 0x01ff || segments[1] == 0x0db8))
        && segments[0] != 0x2002
        && !(segments[0] == 0x3fff && (segments[1] & 0xf000) == 0)
}

fn validate_addresses(addrs: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, Error> {
    if addrs.is_empty() || addrs.len() > 64 || addrs.iter().any(|addr| !public_address(addr.ip())) {
        return Err(Error::UnsafeAddress);
    }
    Ok(addrs)
}

static RESOLVERS: AtomicUsize = AtomicUsize::new(0);

fn resolve(host: &str, timeout: Duration) -> Result<Vec<SocketAddr>, Error> {
    RESOLVERS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
            (active < 4).then_some(active + 1)
        })
        .map_err(|_| Error::Network("DNS resolver busy".into()))?;
    let host = host.to_owned();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let spawn = std::thread::Builder::new()
        .name("katran-favicon-dns".into())
        .spawn(move || {
            struct Guard;
            impl Drop for Guard {
                fn drop(&mut self) {
                    RESOLVERS.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _guard = Guard;
            let result = (host.as_str(), 443)
                .to_socket_addrs()
                .map(|addrs| addrs.take(65).collect::<Vec<_>>())
                .map_err(|e| Error::Network(e.to_string()));
            let _ = sender.send(result);
        });
    if let Err(error) = spawn {
        RESOLVERS.fetch_sub(1, Ordering::AcqRel);
        return Err(Error::Network(error.to_string()));
    }
    let addresses = receiver
        .recv_timeout(timeout)
        .map_err(|_| Error::Network("DNS lookup timed out".into()))??;
    validate_addresses(addresses)
}

/// Derive only an HTTPS origin on the default port; paths, queries, fragments
/// and credentials from the source link are never sent.
fn favicon_url(input: &str) -> Result<Url, Error> {
    if input
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err(Error::InvalidUrl);
    }
    let parsed = Url::parse(input).map_err(|_| Error::InvalidUrl)?;
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https")
        || !input
            .get(scheme.len()..)
            .is_some_and(|tail| tail.starts_with("://"))
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
    {
        return Err(Error::InvalidUrl);
    }
    let Host::Domain(domain) = parsed.host().ok_or(Error::InvalidUrl)? else {
        return Err(Error::InvalidUrl);
    };
    let domain = domain.trim_end_matches('.');
    if domain.is_empty()
        || !domain.contains('.')
        || domain == "localhost"
        || domain.ends_with(".localhost")
        || domain.ends_with(".local")
        || domain.ends_with(".internal")
    {
        return Err(Error::InvalidUrl);
    }
    Url::parse(&format!("https://{domain}/favicon.ico")).map_err(|_| Error::InvalidUrl)
}

fn safe_https_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.host(), Some(Host::Domain(domain)) if domain.contains('.')
            && !domain.ends_with(".local") && !domain.ends_with(".internal"))
}

fn declared_icons(html: &str, origin: &Url) -> Vec<Url> {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse("link[rel][href]") else {
        return Vec::new();
    };
    document
        .select(&selector)
        .filter_map(|element| {
            let rel = element.value().attr("rel")?;
            if !rel.split_ascii_whitespace().any(|part| {
                part.eq_ignore_ascii_case("icon") || part.eq_ignore_ascii_case("apple-touch-icon")
            }) {
                return None;
            }
            if element
                .value()
                .attr("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("image/svg+xml"))
            {
                return None;
            }
            let icon = origin.join(element.value().attr("href")?).ok()?;
            (safe_https_url(&icon)
                && icon.as_str().len() <= 2048
                && !icon.path().to_ascii_lowercase().ends_with(".svg"))
            .then_some(icon)
        })
        .take(4)
        .collect()
}

fn download(url: &Url, limit: usize) -> Result<Option<Vec<u8>>, Error> {
    if !safe_https_url(url) {
        return Err(Error::InvalidUrl);
    }
    let started = Instant::now();
    let timeout = Duration::from_secs(3);
    let host = url.host_str().ok_or(Error::InvalidUrl)?;
    let addresses = resolve(host, timeout)?;
    let remaining = timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| Error::Network("DNS lookup timed out".into()))?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .timeout(remaining)
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;
    let response = client
        .get(url.clone())
        .send()
        .map_err(|e| Error::Network(e.to_string()))?;
    if response.status().is_redirection() {
        let destination = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|location| url.join(location).ok());
        return destination
            .filter(safe_https_url)
            .map(|next| download_direct(&next, limit))
            .unwrap_or(Ok(None));
    }
    if !response.status().is_success() {
        return Ok(None);
    }
    if response
        .content_length()
        .is_some_and(|len| len > limit as u64)
    {
        return Err(Error::TooLarge);
    }
    read_bounded_limit(response, limit).map(Some)
}

// One validated redirect at most; never let the HTTP client follow an unchecked chain.
fn download_direct(url: &Url, limit: usize) -> Result<Option<Vec<u8>>, Error> {
    if !safe_https_url(url) {
        return Err(Error::InvalidUrl);
    }
    let timeout = Duration::from_secs(3);
    let host = url.host_str().ok_or(Error::InvalidUrl)?;
    let addresses = resolve(host, timeout)?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .timeout(timeout)
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;
    let response = client
        .get(url.clone())
        .send()
        .map_err(|e| Error::Network(e.to_string()))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    if response
        .content_length()
        .is_some_and(|len| len > limit as u64)
    {
        return Err(Error::TooLarge);
    }
    read_bounded_limit(response, limit).map(Some)
}

fn read_bounded_limit(reader: impl Read, limit: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Read(e.to_string()))?;
    if bytes.len() > limit {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}

#[cfg(test)]
fn read_bounded(reader: impl Read) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Read(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}

/// Validate full decoding, not just a MIME type or magic prefix. Return bytes
/// usable by the existing image feature set. PNG-in-ICO is unwrapped;
/// other supported ICO entries are decoded and converted to bounded PNG.
fn image_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    if bytes.starts_with(&[0, 0, 1, 0]) {
        let count = u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?) as usize;
        if count == 0 || count > 128 {
            return None;
        }
        for index in 0..count {
            let start = 6usize.checked_add(index.checked_mul(16)?)?;
            let entry = bytes.get(start..start.checked_add(16)?)?;
            let len = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
            if offset < 6usize.checked_add(count.checked_mul(16)?)? {
                continue;
            }
            let Some(candidate) = offset
                .checked_add(len)
                .and_then(|end| bytes.get(offset..end))
            else {
                continue;
            };
            if candidate.starts_with(b"\x89PNG\r\n\x1a\n") && decode_image(candidate) {
                return Some(candidate.to_vec());
            }
        }
        let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Ico);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_EDGE);
        limits.max_image_height = Some(MAX_EDGE);
        limits.max_alloc = Some(MAX_ALLOC);
        reader.limits(limits);
        let image = reader.decode().ok()?;
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, ImageFormat::Png).ok()?;
        (png.get_ref().len() <= MAX_BYTES).then(|| png.into_inner())
    } else if decode_image(bytes) {
        Some(bytes.to_vec())
    } else {
        None
    }
}

fn decode_image(bytes: &[u8]) -> bool {
    let Ok(mut reader) = ImageReader::new(Cursor::new(bytes)).with_guessed_format() else {
        return false;
    };
    if !matches!(
        reader.format(),
        Some(
            ImageFormat::Png
                | ImageFormat::Jpeg
                | ImageFormat::Gif
                | ImageFormat::WebP
                | ImageFormat::Bmp
        )
    ) {
        return false;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_EDGE);
    limits.max_image_height = Some(MAX_EDGE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    reader.decode().is_ok()
}

/// Network errors and oversized responses are explicit; unsupported/missing
/// images return None so the caller can display its local domain symbol.
pub fn fetch(input: &str) -> Result<Option<Vec<u8>>, Error> {
    let url = favicon_url(input)?;
    if let Ok(Some(bytes)) = download(&url, MAX_BYTES)
        && let Some(icon) = image_bytes(&bytes)
    {
        return Ok(Some(icon));
    }
    let origin = url.join("/").map_err(|_| Error::InvalidUrl)?;
    let Some(html) = download(&origin, MAX_HTML_BYTES)? else {
        return Ok(None);
    };
    for icon_url in declared_icons(&String::from_utf8_lossy(&html), &origin) {
        if let Ok(Some(bytes)) = download(&icon_url, MAX_BYTES)
            && let Some(icon) = image_bytes(&bytes)
        {
            return Ok(Some(icon));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_declared_site_icon_without_reusing_link_path() {
        let origin = Url::parse("https://app.notion.com/").unwrap();
        let html = r#"<html><head><link rel="apple-touch-icon" href="https://static.notion.com/icon.png"></head></html>"#;
        assert_eq!(
            declared_icons(html, &origin)[0].as_str(),
            "https://static.notion.com/icon.png"
        );
        assert!(
            declared_icons(
                r#"<link rel="icon" href="http://127.0.0.1/icon.png">"#,
                &origin
            )
            .is_empty()
        );
    }
    #[test]
    fn public_address_policy_rejects_special_and_mixed_dns_results() {
        for address in [
            "127.0.0.1",
            "10.1.2.3",
            "100.64.0.1",
            "169.254.169.254",
            "172.31.1.2",
            "192.168.1.2",
            "198.18.0.1",
            "192.0.2.1",
            "224.0.0.1",
            "::1",
            "::ffff:8.8.8.8",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2002:0808:0808::1",
            "3fff::1",
        ] {
            assert!(!public_address(address.parse().unwrap()), "{address}");
        }
        for address in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(public_address(address.parse().unwrap()), "{address}");
        }
        assert!(validate_addresses(vec![]).is_err());
        assert!(
            validate_addresses(vec![
                "8.8.8.8:443".parse().unwrap(),
                "127.0.0.1:443".parse().unwrap()
            ])
            .is_err()
        );
        assert!(validate_addresses(vec!["8.8.8.8:443".parse().unwrap()]).is_ok());
    }
    #[test]
    fn url_policy_discards_private_link_details_and_rejects_unsafe_hosts() {
        assert_eq!(
            favicon_url("http://Example.COM/a?q=secret#fragment")
                .unwrap()
                .as_str(),
            "https://example.com/favicon.ico"
        );
        for bad in [
            "https://localhost/",
            "https://a.localhost/",
            "https://printer.local/",
            "https://intranet/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://user:secret@example.com/",
            "https://example.com:8443/",
            "file:///tmp/file",
            "https:example.com",
            "https://example.com/hello world",
        ] {
            assert!(favicon_url(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn image_validation_and_response_limit_are_bounded() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        assert_eq!(image_bytes(&png), Some(png.clone()));
        assert!(image_bytes(b"not an image").is_none());
        assert!(image_bytes(&png[..16]).is_none());
        assert!(matches!(
            read_bounded(&vec![0; MAX_BYTES + 1][..]),
            Err(Error::TooLarge)
        ));
        let mut ico = vec![0, 0, 1, 0, 1, 0];
        let mut entry = [0u8; 16];
        entry[8..12].copy_from_slice(&(png.len() as u32).to_le_bytes());
        entry[12..16].copy_from_slice(&22u32.to_le_bytes());
        ico.extend_from_slice(&entry);
        ico.extend_from_slice(&png);
        assert_eq!(image_bytes(&ico), Some(png));
        ico[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(image_bytes(&ico).is_none());
    }
}

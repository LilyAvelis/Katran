//! Local v2 candidate: ordered, typed Links without application-specific metadata.
//! URLs are inert strings; consumers must parse and validate again before opening.
use crate::codec;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

pub const VERSION: u32 = 2;
pub const KIND: &[u8; 4] = b"LINK";
pub const MAX_URL_BYTES: usize = 8 * 1024;
pub const MAX_TITLE_BYTES: usize = 4 * 1024;
pub const MAX_DESCRIPTION_BYTES: usize = 16 * 1024;
pub const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;
const RECORD_HEADER: usize = 36;
const HEADER: usize = 20;
const MAX_PAYLOAD: usize =
    16 + MAX_URL_BYTES + MAX_TITLE_BYTES + MAX_DESCRIPTION_BYTES + MAX_PREVIEW_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub title: String,
    pub description: String,
    pub preview_png: Option<Vec<u8>>,
}
fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}
fn clean_text(s: &str, limit: usize) -> bool {
    s.len() <= limit && !s.chars().any(char::is_control)
}
fn valid_url(value: &str) -> bool {
    if value.is_empty()
        || !clean_text(value, MAX_URL_BYTES)
        || value.chars().any(char::is_whitespace)
        || value.contains('\\')
    {
        return false;
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return false;
    }
    let authority = if let Some((userinfo, host)) = authority.rsplit_once('@') {
        if userinfo.is_empty() || userinfo.contains('@') {
            return false;
        }
        host
    } else {
        authority
    };
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let Some((ip, suffix)) = v6.split_once(']') else {
            return false;
        };
        if ip.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        let port = if suffix.is_empty() {
            None
        } else {
            match suffix.strip_prefix(':') {
                Some(port) => Some(port),
                None => return false,
            }
        };
        ("ipv6", port)
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        if host.is_empty()
            || host.split('.').any(|label| {
                label.is_empty()
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label.chars().all(|ch| ch.is_alphanumeric() || ch == '-')
            })
        {
            return false;
        }
        (host, port)
    };
    !host.is_empty()
        && port.is_none_or(|port| {
            !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok()
        })
}
fn validate(link: &Link) -> io::Result<usize> {
    if !valid_url(&link.url) {
        return Err(invalid("invalid HTTP(S) URL"));
    }
    if !clean_text(&link.title, MAX_TITLE_BYTES)
        || !clean_text(&link.description, MAX_DESCRIPTION_BYTES)
    {
        return Err(invalid("invalid link text"));
    }
    let preview = link.preview_png.as_deref().unwrap_or(&[]);
    if link.preview_png.is_some()
        && (preview.is_empty() || preview.len() > MAX_PREVIEW_BYTES || !codec::png_valid(preview))
    {
        return Err(invalid("invalid link preview PNG"));
    }
    Ok(16 + link.url.len() + link.title.len() + link.description.len() + preview.len())
}
fn write_field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}
fn payload(link: &Link, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    write_field(&mut out, link.url.as_bytes());
    write_field(&mut out, link.title.as_bytes());
    write_field(&mut out, link.description.as_bytes());
    write_field(&mut out, link.preview_png.as_deref().unwrap_or(&[]));
    out
}
/// Validate the complete collection before writing its first byte.
pub fn write<W: Write>(mut out: W, links: &[Link]) -> io::Result<()> {
    if !(1..=codec::MAX_PAGES).contains(&links.len()) {
        return Err(invalid("invalid link count"));
    }
    let mut total = HEADER;
    let mut lengths = Vec::with_capacity(links.len());
    for link in links {
        let len = validate(link)?;
        total = total
            .checked_add(RECORD_HEADER + len)
            .ok_or_else(|| invalid("collection size overflow"))?;
        if total > codec::MAX_TOTAL_BYTES {
            return Err(invalid("collection exceeds size limit"));
        }
        lengths.push(len);
    }
    out.write_all(codec::MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(KIND)?;
    out.write_all(&(links.len() as u32).to_le_bytes())?;
    for (link, len) in links.iter().zip(lengths) {
        let bytes = payload(link, len);
        out.write_all(&(len as u32).to_le_bytes())?;
        out.write_all(&Sha256::digest(&bytes))?;
        out.write_all(&bytes)?;
    }
    Ok(())
}
fn number<R: Read>(input: &mut R) -> io::Result<u32> {
    let mut bytes = [0; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
fn field<'a>(input: &mut &'a [u8], max: usize) -> io::Result<&'a [u8]> {
    let size = number(input)? as usize;
    if size > max || size > input.len() {
        return Err(invalid("invalid link field length"));
    }
    let (value, remainder) = input.split_at(size);
    *input = remainder;
    Ok(value)
}
fn text_field(input: &mut &[u8], max: usize) -> io::Result<String> {
    let value = field(input, max)?;
    std::str::from_utf8(value)
        .map(str::to_owned)
        .map_err(|_| invalid("invalid UTF-8"))
}
/// Read the bounded format; trailing data and untrusted checksums are rejected.
pub fn read<R: Read>(mut input: R) -> io::Result<Vec<Link>> {
    let mut magic = [0; 8];
    input.read_exact(&mut magic)?;
    if &magic != codec::MAGIC || number(&mut input)? != VERSION {
        return Err(invalid("invalid link collection version"));
    }
    let mut kind = [0; 4];
    input.read_exact(&mut kind)?;
    if &kind != KIND {
        return Err(invalid("invalid collection kind"));
    }
    let count = number(&mut input)? as usize;
    if !(1..=codec::MAX_PAGES).contains(&count) {
        return Err(invalid("invalid link count"));
    }
    let mut total = HEADER;
    let mut links = Vec::with_capacity(count);
    for _ in 0..count {
        let len = number(&mut input)? as usize;
        if !(16..=MAX_PAYLOAD).contains(&len) {
            return Err(invalid("invalid record length"));
        }
        total = total
            .checked_add(RECORD_HEADER + len)
            .ok_or_else(|| invalid("collection size overflow"))?;
        if total > codec::MAX_TOTAL_BYTES {
            return Err(invalid("collection exceeds size limit"));
        }
        let mut expected = [0; 32];
        input.read_exact(&mut expected)?;
        let mut bytes = vec![0; len];
        input.read_exact(&mut bytes)?;
        if Sha256::digest(&bytes).as_slice() != expected {
            return Err(invalid("link digest mismatch"));
        }
        let mut remaining = bytes.as_slice();
        let url = text_field(&mut remaining, MAX_URL_BYTES)?;
        let title = text_field(&mut remaining, MAX_TITLE_BYTES)?;
        let description = text_field(&mut remaining, MAX_DESCRIPTION_BYTES)?;
        let preview = field(&mut remaining, MAX_PREVIEW_BYTES)?;
        let link = Link {
            url,
            title,
            description,
            preview_png: (!preview.is_empty()).then(|| preview.to_vec()),
        };
        if !remaining.is_empty() {
            return Err(invalid("trailing link payload"));
        }
        validate(&link)?;
        links.push(link);
    }
    let mut trailing = [0];
    if input.read(&mut trailing)? != 0 {
        return Err(invalid("trailing collection data"));
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrips_large_collections_up_to_shared_limit() {
        for count in [33, codec::MAX_PAGES] {
            let entries: Vec<_> = (0..count)
                .map(|i| {
                    let mut value = link();
                    value.url = format!("https://example.org/{i}");
                    value
                })
                .collect();
            let mut encoded = Vec::new();
            write(&mut encoded, &entries).unwrap();
            assert_eq!(read(encoded.as_slice()).unwrap(), entries);
            encoded[16..20].copy_from_slice(&((codec::MAX_PAGES + 1) as u32).to_le_bytes());
            assert!(read(encoded.as_slice()).is_err());
        }
    }
    fn link() -> Link {
        Link {
            url: "https://example.org/страница?q=1".into(),
            title: "Название".into(),
            description: "Описание".into(),
            preview_png: None,
        }
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(bytes);
        out.extend_from_slice(&crc32fast::hash(&out[start..]).to_be_bytes());
    }
    fn png() -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(&mut out, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]);
        chunk(
            &mut out,
            b"IDAT",
            &[0x78, 0x9c, 0x63, 0x60, 0, 0, 0, 2, 0, 1],
        );
        chunk(&mut out, b"IEND", &[]);
        out
    }
    #[test]
    fn roundtrip_and_exact_header() {
        let first = link();
        let mut second = link();
        second.preview_png = Some(png());
        let mut out = Vec::new();
        write(&mut out, &[first.clone(), second.clone()]).unwrap();
        assert_eq!(&out[..8], codec::MAGIC);
        assert_eq!(&out[8..12], &2u32.to_le_bytes());
        assert_eq!(&out[12..16], b"LINK");
        assert_eq!(&out[16..20], &2u32.to_le_bytes());
        let len = u32::from_le_bytes(out[20..24].try_into().unwrap()) as usize;
        assert_eq!(&out[24..56], Sha256::digest(&out[56..56 + len]).as_slice());
        assert_eq!(read(out.as_slice()).unwrap(), vec![first, second]);
    }
    #[test]
    fn reject_bad_urls_fields_and_png() {
        for url in [
            "",
            "javascript:alert(1)",
            "http://",
            "https://foo:abc",
            "https://a b/",
            "https://user@@example.com",
            "https://x/\n",
            "https://x\\evil",
        ] {
            let mut entry = link();
            entry.url = url.into();
            let mut out = Vec::new();
            assert!(write(&mut out, &[entry]).is_err(), "{url}");
            assert!(out.is_empty());
        }
        let mut entry = link();
        entry.title = "x".repeat(MAX_TITLE_BYTES + 1);
        assert!(write(Vec::new(), &[entry]).is_err());
        let mut entry = link();
        entry.description = "line\nnext".into();
        assert!(write(Vec::new(), &[entry]).is_err());
        let mut entry = link();
        entry.preview_png = Some(b"bad".to_vec());
        assert!(write(Vec::new(), &[entry]).is_err());
        assert!(write(Vec::new(), &[]).is_err());
        assert!(write(Vec::new(), &vec![link(); codec::MAX_PAGES + 1]).is_err());
    }
    #[test]
    fn reject_corruption_truncation_and_trailing() {
        let mut data = Vec::new();
        write(&mut data, &[link()]).unwrap();
        for size in [0, 8, 19, 20, 55, data.len() - 1] {
            assert!(read(&data[..size]).is_err());
        }
        let mut bad = data.clone();
        bad[12] ^= 1;
        assert!(read(&bad[..]).is_err());
        let mut bad = data.clone();
        bad[24] ^= 1;
        assert!(read(&bad[..]).is_err());
        let mut bad = data.clone();
        bad[20..24].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_le_bytes());
        assert!(read(&bad[..]).is_err());
        let mut bad = data;
        bad.push(0);
        assert!(read(&bad[..]).is_err());
    }
}

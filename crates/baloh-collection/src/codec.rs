//! Local v1 candidate for ordered PNG collections, independent of application data.
//!
//! PNG validation checks the signature, chunk boundaries, CRCs, IHDR, IDAT and
//! IEND; it does not decompress pixels or guarantee successful image decoding.
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

pub const MAGIC: &[u8; 8] = b"BALOHCOL";
pub const VERSION: u32 = 1;
pub const MAX_PAGES: usize = 32;
pub const MAX_PAGE_BYTES: usize = 64 * 1024 * 1024;
/// Complete encoded file budget, including framing and checksums.
pub const MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;
const HEADER: usize = 16;
const RECORD: usize = 36;
const PNG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn check_count(count: usize) -> io::Result<()> {
    if !(1..=MAX_PAGES).contains(&count) {
        return Err(invalid("invalid collection page count"));
    }
    Ok(())
}
fn check_size(size: usize, total: &mut usize) -> io::Result<()> {
    if size == 0 || size > MAX_PAGE_BYTES {
        return Err(invalid("invalid PNG size"));
    }
    *total = total
        .checked_add(RECORD)
        .and_then(|n| n.checked_add(size))
        .ok_or_else(|| invalid("collection size overflow"))?;
    if *total > MAX_TOTAL_BYTES {
        return Err(invalid("collection exceeds size limit"));
    }
    Ok(())
}
pub(crate) fn png_valid(bytes: &[u8]) -> bool {
    if !bytes.starts_with(PNG) {
        return false;
    }
    let mut offset = PNG.len();
    let mut chunks = 0usize;
    let mut image_data = false;
    while let Some(header_end) = offset.checked_add(8) {
        if header_end > bytes.len() {
            return false;
        }
        let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let Some(end) = header_end.checked_add(len).and_then(|n| n.checked_add(4)) else {
            return false;
        };
        if end > bytes.len() {
            return false;
        }
        let kind = &bytes[offset + 4..header_end];
        let content = &bytes[header_end..end - 4];
        let crc = u32::from_be_bytes(bytes[end - 4..end].try_into().unwrap());
        if crc32fast::hash(&bytes[offset + 4..end - 4]) != crc {
            return false;
        }
        if chunks == 0 {
            if kind != b"IHDR" || len != 13 || content[..4] == [0; 4] || content[4..8] == [0; 4] {
                return false;
            }
        } else if kind == b"IHDR" {
            return false;
        }
        if kind == b"IDAT" {
            image_data = true;
        }
        if kind == b"IEND" {
            return len == 0 && image_data && end == bytes.len();
        }
        offset = end;
        chunks += 1;
    }
    false
}
/// Write one complete collection. All pages are checked before writing any bytes.
pub fn write<W: Write>(mut out: W, pages: &[Vec<u8>]) -> io::Result<()> {
    check_count(pages.len())?;
    let mut total = HEADER;
    for page in pages {
        check_size(page.len(), &mut total)?;
        if !png_valid(page) {
            return Err(invalid("invalid PNG structure"));
        }
    }
    out.write_all(MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&(pages.len() as u32).to_le_bytes())?;
    for page in pages {
        out.write_all(&(page.len() as u32).to_le_bytes())?;
        out.write_all(&Sha256::digest(page))?;
        out.write_all(page)?;
    }
    Ok(())
}
fn number<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
fn read_header(mut input: impl Read) -> io::Result<usize> {
    let mut magic = [0; 8];
    input.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid("invalid collection magic"));
    }
    if number(&mut input)? != VERSION {
        return Err(invalid("unsupported collection version"));
    }
    let count = number(&mut input)? as usize;
    check_count(count)?;
    Ok(count)
}

fn read_page(mut input: impl Read, total: &mut usize) -> io::Result<Vec<u8>> {
    let size = number(&mut input)? as usize;
    check_size(size, total)?;
    let mut digest = [0u8; 32];
    input.read_exact(&mut digest)?;
    let mut page = vec![0; size];
    input.read_exact(&mut page)?;
    if Sha256::digest(&page).as_slice() != digest {
        return Err(invalid("PNG digest mismatch"));
    }
    if !png_valid(&page) {
        return Err(invalid("invalid PNG structure"));
    }
    Ok(page)
}

/// Verify the header and first page for a thumbnail. Opening the collection
/// still validates every page and the file boundary with `read`.
pub fn read_cover(mut input: impl Read) -> io::Result<Vec<u8>> {
    read_header(&mut input)?;
    let mut total = HEADER;
    read_page(&mut input, &mut total)
}

/// Read and authenticate a collection, rejecting any trailing byte.
pub fn read<R: Read>(mut input: R) -> io::Result<Vec<Vec<u8>>> {
    let count = read_header(&mut input)?;
    let mut total = HEADER;
    let mut pages = Vec::with_capacity(count);
    for _ in 0..count {
        pages.push(read_page(&mut input, &mut total)?);
    }
    let mut trailing = [0];
    if input.read(&mut trailing)? != 0 {
        return Err(invalid("trailing collection data"));
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(bytes);
        out.extend_from_slice(&crc32fast::hash(&out[start..]).to_be_bytes());
    }
    fn page() -> Vec<u8> {
        let mut png = PNG.to_vec();
        chunk(&mut png, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]);
        chunk(
            &mut png,
            b"IDAT",
            &[0x78, 0x9c, 0x63, 0x60, 0, 0, 0, 2, 0, 1],
        );
        chunk(&mut png, b"IEND", &[]);
        png
    }
    #[test]
    fn roundtrip_and_wire_order() {
        let a = page();
        let pages = vec![a.clone(), a.clone()];
        let mut encoded = Vec::new();
        write(&mut encoded, &pages).unwrap();
        assert_eq!(&encoded[..8], MAGIC);
        assert_eq!(&encoded[8..12], &1u32.to_le_bytes());
        assert_eq!(&encoded[12..16], &2u32.to_le_bytes());
        assert_eq!(&encoded[16..20], &(a.len() as u32).to_le_bytes());
        assert_eq!(&encoded[20..52], Sha256::digest(&a).as_slice());
        assert_eq!(&encoded[52..52 + a.len()], &a);
        assert_eq!(read(encoded.as_slice()).unwrap(), pages);
    }
    #[test]
    fn rejects_invalid_framing_and_integrity() {
        let mut encoded = Vec::new();
        write(&mut encoded, &[page()]).unwrap();
        for length in [0, 7, 12, 15, 16, 51, encoded.len() - 1] {
            assert!(read(&encoded[..length]).is_err());
        }
        let mut bad = encoded.clone();
        bad[8] = 2;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[12] = 0;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[12] = 33;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[16..20].copy_from_slice(&(MAX_PAGE_BYTES as u32 + 1).to_le_bytes());
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[20] ^= 1;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[52] ^= 1;
        let digest = Sha256::digest(&bad[52..]);
        bad[20..52].copy_from_slice(&digest);
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded;
        bad.push(0);
        assert!(read(&bad[..]).is_err());
    }
    #[test]
    fn rejects_bad_png_before_any_write() {
        for bytes in [vec![], b"not png".to_vec(), PNG.to_vec(), {
            let mut p = page();
            p[30] ^= 1;
            p
        }] {
            let mut out = Vec::new();
            assert!(write(&mut out, &[bytes]).is_err());
            assert!(out.is_empty());
        }
        assert!(write(Vec::new(), &[]).is_err());
        assert!(write(Vec::new(), &vec![page(); 33]).is_err());
    }
    #[test]
    fn total_budget_includes_record_overhead() {
        let mut total = HEADER;
        for _ in 0..3 {
            check_size(MAX_PAGE_BYTES, &mut total).unwrap();
        }
        assert!(check_size(MAX_PAGE_BYTES, &mut total).is_err());
    }
}

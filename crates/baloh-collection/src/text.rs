//! Local v2 candidate for ordered exact UTF-8 text entries.
//! Entries retain whitespace and line endings; embedded NUL is not supported.
use crate::codec;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

pub const VERSION: u32 = 2;
pub const KIND: &[u8; 4] = b"TEXT";
pub const MAX_ENTRY_BYTES: usize = 1024 * 1024;
const HEADER: usize = 20;
const RECORD_HEADER: usize = 36;
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn count_valid(count: usize) -> io::Result<()> {
    if !(1..=codec::MAX_PAGES).contains(&count) {
        return Err(invalid("invalid text count"));
    }
    Ok(())
}
fn size_valid(len: usize, total: &mut usize) -> io::Result<()> {
    if !(1..=MAX_ENTRY_BYTES).contains(&len) {
        return Err(invalid("text entry length is outside the supported range"));
    }
    *total = total
        .checked_add(RECORD_HEADER)
        .and_then(|n| n.checked_add(len))
        .ok_or_else(|| invalid("collection size overflow"))?;
    if *total > codec::MAX_TOTAL_BYTES {
        return Err(invalid("collection exceeds size limit"));
    }
    Ok(())
}
fn number<R: Read>(input: &mut R) -> io::Result<u32> {
    let mut bytes = [0; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
/// Validate the entire collection before writing its first byte.
pub fn write<W: Write>(mut out: W, entries: &[String]) -> io::Result<()> {
    count_valid(entries.len())?;
    let mut total = HEADER;
    for text in entries {
        size_valid(text.len(), &mut total)?;
        if text.contains('\0') {
            return Err(invalid("embedded NUL in text"));
        }
    }
    out.write_all(codec::MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(KIND)?;
    out.write_all(&(entries.len() as u32).to_le_bytes())?;
    for text in entries {
        out.write_all(&(text.len() as u32).to_le_bytes())?;
        out.write_all(&Sha256::digest(text.as_bytes()))?;
        out.write_all(text.as_bytes())?;
    }
    Ok(())
}
/// Reject malformed framing, non-UTF-8 text, NUL, digest errors and trailing bytes.
pub fn read<R: Read>(mut input: R) -> io::Result<Vec<String>> {
    let mut magic = [0; 8];
    input.read_exact(&mut magic)?;
    if &magic != codec::MAGIC || number(&mut input)? != VERSION {
        return Err(invalid("invalid text collection version"));
    }
    let mut kind = [0; 4];
    input.read_exact(&mut kind)?;
    if &kind != KIND {
        return Err(invalid("invalid collection kind"));
    }
    let count = number(&mut input)? as usize;
    count_valid(count)?;
    let mut total = HEADER;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let len = number(&mut input)? as usize;
        size_valid(len, &mut total)?;
        let mut expected = [0; 32];
        input.read_exact(&mut expected)?;
        let mut bytes = vec![0; len];
        input.read_exact(&mut bytes)?;
        if Sha256::digest(&bytes).as_slice() != expected {
            return Err(invalid("text digest mismatch"));
        }
        let text = String::from_utf8(bytes).map_err(|_| invalid("invalid UTF-8"))?;
        if text.contains('\0') {
            return Err(invalid("embedded NUL in text"));
        }
        entries.push(text);
    }
    let mut trailing = [0];
    if input.read(&mut trailing)? != 0 {
        return Err(invalid("trailing collection data"));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrips_large_collections_up_to_shared_limit() {
        for count in [33, codec::MAX_PAGES] {
            let entries: Vec<_> = (0..count).map(|i| format!("entry-{i}")).collect();
            let mut encoded = Vec::new();
            write(&mut encoded, &entries).unwrap();
            assert_eq!(read(encoded.as_slice()).unwrap(), entries);
            encoded[16..20].copy_from_slice(&((codec::MAX_PAGES + 1) as u32).to_le_bytes());
            assert!(read(encoded.as_slice()).is_err());
        }
    }
    #[test]
    fn roundtrip_preserves_exact_bytes_and_order() {
        let entries = vec![
            "  Привет\r\nмир\t \n".to_owned(),
            " \n".into(),
            "\u{00a0}日本語".to_owned(),
        ];
        let mut out = Vec::new();
        write(&mut out, &entries).unwrap();
        assert_eq!(&out[..8], codec::MAGIC);
        assert_eq!(&out[8..12], &2u32.to_le_bytes());
        assert_eq!(&out[12..16], b"TEXT");
        assert_eq!(&out[16..20], &3u32.to_le_bytes());
        let len = u32::from_le_bytes(out[20..24].try_into().unwrap()) as usize;
        assert_eq!(len, entries[0].len());
        assert_eq!(
            &out[24..56],
            Sha256::digest(entries[0].as_bytes()).as_slice()
        );
        assert_eq!(&out[56..56 + len], entries[0].as_bytes());
        assert_eq!(read(out.as_slice()).unwrap(), entries);
    }
    #[test]
    fn reject_limits_nul_and_invalid_framing() {
        assert!(write(Vec::new(), &[]).is_err());
        assert!(write(Vec::new(), &vec![String::from("x"); codec::MAX_PAGES + 1]).is_err());
        for text in [
            String::new(),
            "a\0b".to_owned(),
            "x".repeat(MAX_ENTRY_BYTES + 1),
        ] {
            let mut out = Vec::new();
            assert!(write(&mut out, &[text]).is_err());
            assert!(out.is_empty());
        }
        let mut data = Vec::new();
        write(&mut data, &["x".into()]).unwrap();
        for len in [0, 8, 19, 20, 55, data.len() - 1] {
            assert!(read(&data[..len]).is_err());
        }
        let mut bad = data.clone();
        bad[12] ^= 1;
        assert!(read(&bad[..]).is_err());
        let mut bad = data.clone();
        bad[24] ^= 1;
        assert!(read(&bad[..]).is_err());
        let mut bad = data.clone();
        bad[20..24].copy_from_slice(&(MAX_ENTRY_BYTES as u32 + 1).to_le_bytes());
        assert!(read(&bad[..]).is_err());
        let mut bad = data;
        bad.push(0);
        assert!(read(&bad[..]).is_err());
    }
    #[test]
    fn reject_authenticated_invalid_utf8_and_nul() {
        let mut data = Vec::new();
        write(&mut data, &["x".into()]).unwrap();
        for byte in [0, 0xff] {
            let mut bad = data.clone();
            bad[56] = byte;
            let digest = Sha256::digest(&bad[56..]);
            bad[24..56].copy_from_slice(&digest);
            assert!(read(&bad[..]).is_err());
        }
    }
}

//! Portable ordered file collection, local BS v1 candidate.
//!
//! Names are safe Windows leaf names and are never interpreted as paths here.
//! The caller must still use safe destination creation and collision handling.
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

pub const MAGIC: &[u8; 8] = b"BALOHBS\0";
pub const VERSION: u32 = 1;
pub const MAX_FILES: usize = crate::MAX_ENTRIES;
pub const MAX_FILE_BYTES: usize = 128 * 1024 * 1024;
/// Maximum complete encoded file size, including all framing.
pub const MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_NAME_BYTES: usize = 255;
const HEADER: usize = 16;
const RECORD_HEADER: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub bytes: Vec<u8>,
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn valid_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
    {
        return false;
    }
    // Windows reserves these stems even when followed by an extension; spaces
    // immediately before the extension are normalized by Win32 as well.
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return false;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(suffix) = upper.strip_prefix(prefix)
            && (matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            ))
        {
            return false;
        }
    }
    true
}
fn count_valid(count: usize) -> io::Result<()> {
    if !(1..=MAX_FILES).contains(&count) {
        return Err(invalid("invalid file count"));
    }
    Ok(())
}
fn size_valid(name: usize, data: usize, total: &mut usize) -> io::Result<()> {
    if name == 0 || name > MAX_NAME_BYTES || data > MAX_FILE_BYTES {
        return Err(invalid("invalid file record size"));
    }
    *total = total
        .checked_add(RECORD_HEADER)
        .and_then(|n| n.checked_add(name))
        .and_then(|n| n.checked_add(data))
        .ok_or_else(|| invalid("collection size overflow"))?;
    if *total > MAX_TOTAL_BYTES {
        return Err(invalid("collection exceeds total size limit"));
    }
    Ok(())
}
fn digest(name: &[u8], data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update((name.len() as u32).to_le_bytes());
    hash.update((data.len() as u32).to_le_bytes());
    hash.update(name);
    hash.update(data);
    hash.finalize().into()
}
fn number<R: Read>(input: &mut R) -> io::Result<u32> {
    let mut bytes = [0; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
/// Check all records before writing any bytes. Duplicate names remain distinct.
pub fn write<W: Write>(mut out: W, entries: &[FileEntry]) -> io::Result<()> {
    count_valid(entries.len())?;
    let mut total = HEADER;
    for entry in entries {
        if !valid_name(&entry.name) {
            return Err(invalid("unsafe file name"));
        }
        size_valid(entry.name.len(), entry.bytes.len(), &mut total)?;
    }
    out.write_all(MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&(entries.len() as u32).to_le_bytes())?;
    for entry in entries {
        let name = entry.name.as_bytes();
        out.write_all(&(name.len() as u32).to_le_bytes())?;
        out.write_all(&(entry.bytes.len() as u32).to_le_bytes())?;
        out.write_all(&digest(name, &entry.bytes))?;
        out.write_all(name)?;
        out.write_all(&entry.bytes)?;
    }
    Ok(())
}
fn read_count<R: Read>(mut input: R) -> io::Result<usize> {
    let mut magic = [0; 8];
    input.read_exact(&mut magic)?;
    if &magic != MAGIC || number(&mut input)? != VERSION {
        return Err(invalid("invalid BS header"));
    }
    let count = number(&mut input)? as usize;
    count_valid(count)?;
    Ok(count)
}

fn read_entry<R: Read>(
    input: &mut R,
    total: &mut usize,
    retain_limit: usize,
) -> io::Result<(FileEntry, bool)> {
    let name_len = number(input)? as usize;
    let data_len = number(input)? as usize;
    size_valid(name_len, data_len, total)?;
    let mut expected = [0; 32];
    input.read_exact(&mut expected)?;
    let mut name = vec![0; name_len];
    input.read_exact(&mut name)?;
    let name = String::from_utf8(name).map_err(|_| invalid("invalid UTF-8 file name"))?;
    if !valid_name(&name) {
        return Err(invalid("unsafe file name"));
    }
    let mut bytes = Vec::new();
    let retained = data_len <= retain_limit;
    if retained {
        bytes.resize(data_len, 0);
        input.read_exact(&mut bytes)?;
        if digest(name.as_bytes(), &bytes) != expected {
            return Err(invalid("file record digest mismatch"));
        }
    } else {
        let mut hash = Sha256::new();
        hash.update((name_len as u32).to_le_bytes());
        hash.update((data_len as u32).to_le_bytes());
        hash.update(name.as_bytes());
        let mut left = data_len;
        let mut buffer = [0; 64 * 1024];
        while left > 0 {
            let len = left.min(buffer.len());
            input.read_exact(&mut buffer[..len])?;
            hash.update(&buffer[..len]);
            left -= len;
        }
        let actual: [u8; 32] = hash.finalize().into();
        if actual != expected {
            return Err(invalid("file record digest mismatch"));
        }
    }
    Ok((FileEntry { name, bytes }, retained))
}

/// Read and validate the first record for a cover. Oversized payloads are
/// verified without retaining bytes, so callers can still show a type icon.
pub fn read_cover<R: Read>(mut input: R, retain_limit: usize) -> io::Result<(FileEntry, bool)> {
    read_count(&mut input)?;
    let mut total = HEADER;
    read_entry(&mut input, &mut total, retain_limit)
}

/// Enforce all bounds before allocation; return entries in their original order.
pub fn read<R: Read>(mut input: R) -> io::Result<Vec<FileEntry>> {
    let count = read_count(&mut input)?;
    let mut total = HEADER;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(read_entry(&mut input, &mut total, MAX_FILE_BYTES)?.0);
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
        for count in [33, MAX_FILES] {
            let entries: Vec<_> = (0..count)
                .map(|i| entry(&format!("{i:04}.txt"), &(i as u32).to_le_bytes()))
                .collect();
            let mut encoded = Vec::new();
            write(&mut encoded, &entries).unwrap();
            assert_eq!(read(encoded.as_slice()).unwrap(), entries);
        }
    }
    fn entry(name: &str, bytes: &[u8]) -> FileEntry {
        FileEntry {
            name: name.into(),
            bytes: bytes.into(),
        }
    }
    #[test]
    fn roundtrip_duplicate_names_empty_data_and_exact_wire() {
        let values = vec![
            entry("тест.txt", b"abc"),
            entry("тест.txt", b""),
            entry("nested.bs", b"x"),
        ];
        let mut encoded = Vec::new();
        write(&mut encoded, &values).unwrap();
        assert_eq!(&encoded[..8], MAGIC);
        assert_eq!(&encoded[8..12], &1u32.to_le_bytes());
        assert_eq!(&encoded[12..16], &3u32.to_le_bytes());
        assert_eq!(
            &encoded[16..20],
            &(values[0].name.len() as u32).to_le_bytes()
        );
        assert_eq!(&encoded[20..24], &3u32.to_le_bytes());
        assert_eq!(&encoded[24..56], &digest(values[0].name.as_bytes(), b"abc"));
        assert_eq!(read(encoded.as_slice()).unwrap(), values);
    }
    #[test]
    fn reject_unsafe_windows_leaf_names() {
        for name in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "c:evil",
            "file:stream",
            "a?b",
            "a*b",
            "a|b",
            "a<b",
            "a>b",
            "a\"b",
            "name.",
            "name ",
            "CON",
            "con.txt",
            "COM1.png",
            "lpt9.a",
            "COM¹.txt",
            "AUX .txt",
            "NUL\n.txt",
        ] {
            let mut out = Vec::new();
            assert!(write(&mut out, &[entry(name, b"")]).is_err(), "{name:?}");
            assert!(out.is_empty());
        }
        assert!(valid_name("мир.bull"));
        assert!(valid_name("com10.txt"));
        assert!(!valid_name(&"я".repeat(128)));
    }
    #[test]
    fn rejects_corruption_truncation_and_bad_limits() {
        let mut encoded = Vec::new();
        write(&mut encoded, &[entry("a.txt", b"data")]).unwrap();
        for len in [0, 7, 15, 16, 55, encoded.len() - 1] {
            assert!(read(&encoded[..len]).is_err());
        }
        let mut bad = encoded.clone();
        bad[8] = 2;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[12] = 0;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[12..16].copy_from_slice(&((MAX_FILES + 1) as u32).to_le_bytes());
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[16..20].copy_from_slice(&256u32.to_le_bytes());
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[20..24].copy_from_slice(&(MAX_FILE_BYTES as u32 + 1).to_le_bytes());
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        bad[56] ^= 1;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(read(&bad[..]).is_err());
        let mut bad = encoded;
        bad.push(0);
        assert!(read(&bad[..]).is_err());
        assert!(write(Vec::new(), &[]).is_err());
        assert!(write(Vec::new(), &vec![entry("a", b""); MAX_FILES + 1]).is_err());
    }
    #[test]
    fn total_budget_includes_names_and_framing() {
        let mut total = HEADER;
        size_valid(1, MAX_FILE_BYTES, &mut total).unwrap();
        assert!(size_valid(1, MAX_FILE_BYTES, &mut total).is_err());
    }
}

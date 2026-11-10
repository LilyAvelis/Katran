//! Pure catalogue sort keys. No filesystem access and no page-local ordering.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    #[default]
    Arrival,
    Extension,
    Modified,
    Shuffle,
    Natural,
    Size,
    Name,
    Origin,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SortSpec {
    pub mode: SortMode,
    pub reverse: bool,
    /// Stable for a complete browsing session, including every subsequent page.
    pub seed: u64,
}

impl SortSpec {
    pub fn normalized(mut self) -> Self {
        if self.mode == SortMode::Shuffle {
            self.reverse = false;
        }
        self
    }
}

pub fn name_key(name: &str) -> String {
    name.to_lowercase()
}

/// Last suffix; a leading dot alone does not make a dotfile an extension.
pub fn extension_key(name: &str) -> String {
    name.rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or_else(String::new, |(_, suffix)| suffix.to_lowercase())
}

/// Recorded provenance only: no inference of a Downloads file's website.
/// Missing provenance stays distinguishable so SQL can keep it last on reverse.
pub fn origin_key(domain: Option<&str>, app: Option<&str>, source: &str) -> Option<String> {
    domain
        .into_iter()
        .chain(app)
        .find_map(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_lowercase())
        })
        .or_else(|| match source {
            "downloads" => Some("downloads".into()),
            "sharex" => Some("sharex".into()),
            "clipboard" | "clipboard_image" | "clipboard_file" | "clipboard_text"
            | "clipboard_link" | "clipboard_table" => Some("clipboard".into()),
            "manual" => Some("manual".into()),
            _ => None,
        })
}

/// Lexicographically comparable natural key, with unbounded ASCII digit runs.
/// Numeric values compare by significant digit count then digits, avoiding
/// integer overflow. Equal numbers retain the name/id tie-break in the query.
pub fn natural_key(name: &str) -> Vec<u8> {
    let name = name.to_lowercase();
    let bytes = name.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset].is_ascii_digit() {
            let start = offset;
            while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                offset += 1;
            }
            let digits = &bytes[start..offset];
            let significant = digits
                .iter()
                .position(|b| *b != b'0')
                .unwrap_or(digits.len() - 1);
            let digits = &digits[significant..];
            output.push(1);
            output.extend_from_slice(&(digits.len() as u64).to_be_bytes());
            output.extend_from_slice(digits);
        } else {
            output.push(2);
            output.push(bytes[offset]);
            offset += 1;
        }
    }
    output
}

/// SplitMix64 finalizer: deterministic across processes, platforms and pages.
/// Nonnegative output fits SQLite's signed integer representation.
pub fn shuffle_key(id: i64, seed: u64) -> i64 {
    let mut value = (id as u64)
        .wrapping_add(seed)
        .wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((value ^ (value >> 31)) & i64::MAX as u64) as i64
}

pub fn modified_ms(time: std::time::SystemTime) -> Option<i64> {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(value) => i64::try_from(value.as_millis()).ok(),
        Err(error) => i64::try_from(error.duration().as_millis())
            .ok()
            .and_then(i64::checked_neg),
    }
}

pub fn action_sort(id: &str) -> Option<(SortMode, bool)> {
    let id = id.strip_prefix("sort_")?;
    let reverse = id.ends_with("_reverse");
    let key = id.strip_suffix("_reverse").unwrap_or(id);
    let mode = match key {
        "arrival" => SortMode::Arrival,
        "extension" => SortMode::Extension,
        "modified" => SortMode::Modified,
        "shuffle" if !reverse => SortMode::Shuffle,
        "natural" => SortMode::Natural,
        "size" => SortMode::Size,
        "name" => SortMode::Name,
        "origin" => SortMode::Origin,
        _ => return None,
    };
    Some((mode, reverse))
}

pub(crate) fn register_sql(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    db.create_scalar_function("katran_name_key", 1, flags, |c| {
        Ok(name_key(&c.get::<String>(0)?))
    })?;
    db.create_scalar_function("katran_extension_key", 1, flags, |c| {
        Ok(extension_key(&c.get::<String>(0)?))
    })?;
    db.create_scalar_function("katran_natural_key", 1, flags, |c| {
        Ok(natural_key(&c.get::<String>(0)?))
    })?;
    db.create_scalar_function("katran_shuffle_key", 2, flags, |c| {
        Ok(shuffle_key(c.get(0)?, c.get::<i64>(1)? as u64))
    })?;
    db.create_scalar_function("katran_origin_key", 3, flags, |c| {
        Ok(origin_key(
            c.get::<Option<String>>(0)?.as_deref(),
            c.get::<Option<String>>(1)?.as_deref(),
            &c.get::<String>(2)?,
        ))
    })?;
    Ok(())
}

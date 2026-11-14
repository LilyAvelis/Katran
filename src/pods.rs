//! Classification of symbol-only clipboard text. Content is never normalized.
use unicode_general_category::{GeneralCategory, get_general_category};

fn symbol(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::OtherSymbol
            | GeneralCategory::MathSymbol
            | GeneralCategory::CurrencySymbol
            | GeneralCategory::ModifierSymbol
    )
}
fn regional(c: char) -> bool {
    ('\u{1f1e6}'..='\u{1f1ff}').contains(&c)
}
fn skin(c: char) -> bool {
    ('\u{1f3fb}'..='\u{1f3ff}').contains(&c)
}
fn emoji_base(c: char) -> bool {
    matches!(c as u32, 0x1f000..=0x1faff | 0x2300..=0x23ff | 0x2600..=0x27ff | 0x2b00..=0x2bff)
        && symbol(c)
        && !skin(c)
        && !regional(c)
}

/// Unicode 16 symbol categories, plus structured emoji joining, modifiers,
/// paired flags and keycaps. Lone punctuation/letters/numbers are not Pods.
pub fn is_pod(text: &str) -> bool {
    if crate::pod_metadata::is_official_emoji(text) {
        return true;
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() || text.len() > crate::text_fragments::MAX_TEXT_BYTES {
        return false;
    }
    let mut i = 0;
    let mut found = false;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '#' || c == '*' {
            i += 1;
            if chars.get(i) == Some(&'\u{fe0f}') {
                i += 1;
            }
            if chars.get(i) != Some(&'\u{20e3}') {
                return false;
            }
            i += 1;
            found = true;
            continue;
        }
        if regional(c) {
            if !chars.get(i + 1).is_some_and(|c| regional(*c)) {
                return false;
            }
            i += 2;
            found = true;
            continue;
        }
        if !symbol(c) || skin(c) {
            return false;
        }
        found = true;
        let mut joined_base = emoji_base(c);
        i += 1;
        loop {
            if matches!(chars.get(i), Some('\u{fe0e}' | '\u{fe0f}')) {
                i += 1;
            }
            if chars.get(i).is_some_and(|c| skin(*c)) {
                if !joined_base {
                    return false;
                }
                i += 1;
            }
            if chars.get(i) != Some(&'\u{200d}') {
                break;
            }
            if !joined_base {
                return false;
            }
            i += 1;
            let Some(&next) = chars.get(i) else {
                return false;
            };
            if !emoji_base(next) {
                return false;
            }
            joined_base = true;
            i += 1;
        }
    }
    found
}

pub(crate) fn register_sql(db: &rusqlite::Connection) -> crate::Result<()> {
    db.create_scalar_function(
        "katran_is_pod",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let text: String = ctx.get(0)?;
            Ok(is_pod(&text))
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_pod;
    #[test]
    fn emoji_and_symbols_without_text_or_dangling_controls() {
        for input in [
            "🐄",
            "👨‍👩‍👧‍👦",
            "👍🏽",
            "🇰🇿",
            "1️⃣",
            "#️⃣",
            "∑",
            "€",
            "→",
            "©",
            "🐄 🐄",
        ] {
            assert!(is_pod(input), "{input:?}");
        }
        for input in [
            "",
            " ",
            "1",
            "#",
            "*",
            "!?",
            "a🐄",
            "1🐄",
            "🐄‍",
            "‍🐄",
            "🐄️️",
            "🇰",
            "🏽",
        ] {
            assert!(!is_pod(input), "{input:?}");
        }
    }
}

// Stable, reserved session IDs; they never address stored events or Folder files.
const EMOJI_ID_BASE: i64 = -(1_i64 << 61);
pub(crate) fn is_emoji_id(id: i64) -> bool {
    id <= EMOJI_ID_BASE && id > crate::desktop_projection::DESKTOP_ID_MAX
}
pub(crate) fn emoji_for_id(id: i64) -> Option<&'static (String, String)> {
    if !is_emoji_id(id) {
        return None;
    }
    let index = usize::try_from(EMOJI_ID_BASE.checked_sub(id)?).ok()?;
    crate::pod_metadata::emoji_entries().get(index)
}
pub(crate) fn emoji_event(id: i64) -> crate::Result<crate::Event> {
    let (text, name) = emoji_for_id(id)
        .ok_or_else(|| crate::ApiError::new("not_found", "Emoji catalogue entry not found"))?;
    Ok(crate::Event {
        id,
        kind: crate::EventKind::TextFragment,
        text_preview: Some(text.clone()),
        delete_policy: crate::source_policy::DeletePolicy::ShelfOnly,
        revision: "unicode-16.0".into(),
        origin: crate::origin::Origin::default(),
        detected_at_ms: 0,
        modified_at_ms: None,
        source: "emoji_catalogue".into(),
        name: name.clone(),
        storage: "virtual".into(),
        path: std::path::PathBuf::new(),
        size: text.len() as u64,
        available: true,
    })
}
pub(crate) fn browse_emoji(
    query: &str,
    limit: u32,
    cursor: Option<&str>,
) -> crate::Result<serde_json::Value> {
    if !(1..=1000).contains(&limit) {
        return Err(crate::ApiError::new(
            "invalid_argument",
            "Expected limit 1..1000",
        ));
    }
    let start = cursor
        .map_or(Ok(0usize), |c| c.parse())
        .map_err(|_| crate::ApiError::new("invalid_cursor", "Invalid emoji cursor"))?;
    let query = query.to_lowercase();
    let entries: Vec<_> = crate::pod_metadata::emoji_entries()
        .iter()
        .enumerate()
        .filter(|(_, (emoji, name))| emoji.contains(&query) || name.to_lowercase().contains(&query))
        .collect();
    if start > entries.len() {
        return Err(crate::ApiError::new(
            "invalid_cursor",
            "Emoji cursor out of range",
        ));
    }
    let end = (start + limit as usize).min(entries.len());
    let events: Vec<_> = entries[start..end].iter().map(|(index, (emoji, name))| serde_json::json!({"id":EMOJI_ID_BASE - *index as i64,"kind":"text_fragment","text_preview":emoji,"name":name,"revision":"unicode-16.0","source":"emoji_catalogue","storage":"virtual","path":"","size":emoji.len(),"available":true})).collect();
    Ok(
        serde_json::json!({"events":events,"next_cursor":(end < entries.len()).then(|| end.to_string()), "unicode_version":"16.0", "total":entries.len()}),
    )
}

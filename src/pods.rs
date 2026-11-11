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

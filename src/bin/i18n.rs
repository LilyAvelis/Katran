//! Presentation translations; the persisted language comes from the domain API.
use std::cell::Cell;

use katran::settings::Language;

thread_local! {
    static LANGUAGE: Cell<Language> = const { Cell::new(Language::Russian) };
}

pub fn set(language: Language) {
    LANGUAGE.with(|selected| selected.set(language));
}

pub fn is_english() -> bool {
    LANGUAGE.with(|selected| selected.get() == Language::English)
}

pub fn tr(ru: &'static str, en: &'static str) -> &'static str {
    tr_for(
        if is_english() {
            Language::English
        } else {
            Language::Russian
        },
        ru,
        en,
    )
}

fn tr_for(language: Language, ru: &'static str, en: &'static str) -> &'static str {
    if language == Language::English {
        en
    } else {
        ru
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_for_selected_language() {
        assert_eq!(
            tr_for(Language::Russian, "Настройки", "Settings"),
            "Настройки"
        );
        assert_eq!(
            tr_for(Language::English, "Настройки", "Settings"),
            "Settings"
        );
    }
}

# Interface languages

Katran supports English and Russian. Settings → General → Interface language stores the selection through `GetSettings` and `SetSettings`; the wire values are `en` and `ru`. New profiles default to English. Existing settings documents without the `language` field load as Russian, preserving their current interface.

The Rust dispatcher localizes the Actions catalogue, including dynamic labels and groups. Action IDs, shortcut chords, command names, tab IDs and JSON result fields remain stable. The GUI translates its own labels and status text from the selected language; names, URLs, file paths, clipboard content and custom tag names remain user data.

The Windows tray menu reads the selected language when opened. A language change takes effect after saving settings and does not require a restart.

Focused checks: `tests/localization_api.rs` covers fresh and legacy settings plus persistence across restart; `tests/actions_presentation.rs` covers bilingual Actions labels and stable IDs/shortcuts. The headless UI tests cover presentation, while native Windows layout and tray behavior remain owner acceptance checks.

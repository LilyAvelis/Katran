//! One capability policy shared by image tabs, capture discovery and preview routing.
use std::path::Path;
pub fn has_internal_preview(event: &crate::Event) -> bool {
    matches!(
        event.kind,
        crate::EventKind::TextFragment
            | crate::EventKind::LinkFragment
            | crate::EventKind::TableFragment
    ) || is_markdown(Path::new(&event.name))
        || is_native_image(Path::new(&event.name))
}
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
}
pub const NATIVE_IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
pub fn is_native_image(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            NATIVE_IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}
pub(crate) fn sql_image_predicate() -> String {
    format!(
        "({})",
        NATIVE_IMAGE_EXTENSIONS
            .iter()
            .map(|ext| format!("lower(e.name) GLOB '*.{ext}'"))
            .collect::<Vec<_>>()
            .join(" OR ")
    )
}

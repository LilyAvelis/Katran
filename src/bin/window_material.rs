//! Katran adapter for the reusable native material request.
use super::i18n::tr;

/// Call on the window's owning UI thread after HWND creation. The caller owns
/// presentation fallback and must keep an opaque theme when this fails.
pub(super) fn apply(hwnd: isize, enabled: bool) -> Result<(), String> {
    use avilon_windows::material::{MaterialError, MaterialRequest, apply};
    let request = if enabled {
        MaterialRequest::Acrylic
    } else {
        MaterialRequest::None
    };
    apply(hwnd, request)
        .map(|_| ())
        .map_err(|error| match error {
            MaterialError::InvalidWindow => tr(
                "Окно Katran недоступно для оформления",
                "Katran window is unavailable for styling",
            )
            .into(),
            other => other.to_string(),
        })
}

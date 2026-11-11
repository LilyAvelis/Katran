//! Katran adapter for the reusable native material request.

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
            MaterialError::InvalidWindow => "Окно Katran недоступно для оформления".into(),
            other => other.to_string(),
        })
}

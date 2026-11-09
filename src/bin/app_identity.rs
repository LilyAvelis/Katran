//! Process/window identity is presentation metadata, independent of the domain.
use eframe::egui;
use std::sync::Arc;

pub(super) fn initialize() {
    #[cfg(windows)]
    if let Err(error) = unsafe {
        windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(windows::core::w!(
            "LiliaAvelis.Katran"
        ))
    } {
        eprintln!("Cannot set Katran taskbar identity: {error}");
    }
}

pub(super) fn window_icon() -> Arc<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../../assets/branding/katran-icon.png"))
        .expect("Bundled Katran icon must be a valid PNG")
        .into_rgba8();
    Arc::new(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_icon_is_square_with_transparent_corners() {
        let icon = window_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert_eq!(icon.rgba[3], 0);
        assert!(icon.rgba.chunks_exact(4).any(|p| p[3] == 255));
    }
}

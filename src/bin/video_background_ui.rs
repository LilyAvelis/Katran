//! Katran host adapter for the reusable Avilon video background player.
pub use avilon_media::VideoSource;
use eframe::egui::{self, TextureHandle};

#[derive(Default)]
pub struct State(avilon_media::State);

impl State {
    pub fn error(&self) -> Option<&str> {
        self.0.error()
    }

    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        source: Option<VideoSource>,
        active: bool,
    ) -> Option<TextureHandle> {
        self.0.tick(ctx, source, active, || {
            katran::media_runtime::command("ffmpeg")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn invalid_source_error_is_exposed_by_host_adapter() {
        let mut state = State::default();
        let source = VideoSource {
            path: PathBuf::from("synthetic.mp4"),
            width: 0,
            height: 90,
            duration_ms: 1000,
            smooth_loop: false,
        };
        assert!(
            state
                .tick(&egui::Context::default(), Some(source), true)
                .is_none()
        );
        assert_eq!(state.error(), Some("Invalid video dimensions"));
    }
}

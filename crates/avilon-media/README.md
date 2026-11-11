# avilon-media

A reusable, bounded FFmpeg video-background player for egui hosts. The host supplies an FFmpeg command factory and owns discovery, installation, and configuration of the executable. Enable the `egui` feature for the player and texture adapter.

```rust,no_run
use avilon_media::{State, VideoSource};
use std::{path::PathBuf, process::Command};

let mut player = State::default();
let source = VideoSource {
    path: PathBuf::from("background.mp4"),
    width: 1280,
    height: 720,
    duration_ms: 10_000,
    smooth_loop: true,
};
let ctx = egui::Context::default();
let texture = player.tick(&ctx, Some(source), true, || Ok(Command::new("ffmpeg")));
// Call tick on subsequent frames; display the returned texture in your UI.
```

The player uses FFmpeg's Windows D3D11VA decoder and downloads NV12 frames for processing. Output is RGBA in CPU memory and uploaded to egui as a texture; this is not zero-copy. Video is scaled to fit 1280×720 and paced at 24 fps. A bounded 512 MiB estimated NV12 loop cache is used when possible; longer videos use streaming. Optional four-frame seam smoothing applies only to cached loops. Pausing stops the decoder and retains the last texture. Decoder errors are exposed through `State::error`; no fallback decoder is selected automatically. Source metadata must be supplied by the host (dimensions up to 3840×2160 and duration up to 120 seconds). The host must provide a compatible FFmpeg build and handle file permissions and user consent.

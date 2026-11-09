# Shelf 02

Historical snapshot. Use the current [README](../README.md) for launch/build instructions.

Start with `Start Katran.cmd`, which selects `target/shelf/debug/katran-ui.exe`
and an absolute `katran-data` directory. Close previous windows manually first.
Build: `cargo build --offline --features ui --bin katran-ui --target-dir target/shelf`.

## Interaction

- Ctrl K opens Actions. Type to filter, Up/Down selects, Enter executes, Esc closes.
- Capture on/off, Refresh, gallery/stream mode, Open, and Save copies live there.
- Gallery: four arrow directions; stream: Up/Down. Navigation scrolls to cursor.
- Click replaces selection, Ctrl-click toggles individual items, Shift-click or
  Shift-arrows selects a range from the anchor. Enter/double-click opens a fitted
  large preview; Esc returns. Unsupported/non-image files show a placeholder.
- Save copies accepts an existing folder path, supports keyboard entry and Enter,
  and uses the existing non-overwriting Import API. No source deletion or move.
- Drag the full top title strip to reposition the window. The search field below
  remains a text input. Footer has status and Afio's Actions button only.

## API and architecture

`Interaction`, `SetVisible`, `Select`, `Navigate`, `SetView`, `OpenCurrent`,
`ClosePreview`, `Actions`, `RunAction`, `Search`, and `PreviewImage` are version 1
commands in the existing dispatcher (JSON uses snake_case). Selection is keyed by
event IDs, not card positions. Each process owns its interaction state; live
cross-process UI control still requires future IPC. `Search` currently searches
the latest limit events (1..1000), not the complete historical database.

PreviewImage accepts `id` and `max_edge` (32..2048) and returns width, height and
base64-encoded RGBA bytes (`rgba_base64`). Decode allocations are capped at 256 MiB. PNG, JPEG, WebP, GIF first
frame and BMP are enabled. It reads sources only and never modifies them.
UI uses a worker and lazy decoding of visible cards, with at most 96 GPU textures
and a 48 MiB texture budget (excluding transient uploads/backend overhead).
Card previews are 320 px and full-view previews 1400 px: this is a fitted preview,
not an original-pixel zoom viewer. Video thumbnails are not implemented.

The shared tokens and ui_style (including Actions button/keycaps) are copied from
LilyAvelis/Afiocira. The palette adapts that project's exact anchor, dimensions,
surface colors, group labels, row geometry and bottom search. Afio's unrelated
commands/shortcut configuration are not included. See `AFIO-LICENSE` for the
retained MIT notice. No runtime dependency on Afio.

Validation: `cargo test --offline --features ui` and
`cargo clippy --offline --all-targets --features ui -- -D warnings`.
Tests cover real dispatcher selection/ranges/view/save intents, image dimensions,
and headless egui keyboard transitions including Actions. Synthetic data only;
they neither read the user's clipboard nor manipulate the native desktop.
Rendered appearance and physical Windows input remain for owner review.

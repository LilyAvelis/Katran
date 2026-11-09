# Shelf 03 · family UI, commands and object lifecycle

Historical snapshot. Use the current [README](../README.md) for launch/build instructions.

Launch **Start Katran.cmd** after closing the previous window yourself. The title
strip identifies **Katran · Shelf 03**. Build/prepare with
`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-working.ps1`.
Build artifacts are never launched directly: the script copies a successful build
to an immutable hash-named runtime version and then updates the launcher pointer.
The absolute data directory remains `katran-data` in the repository.

## Settings and commands

- Ctrl+Comma opens Settings, including from Actions. The shell, sidebar, colours,
  sections, rows, controls, keycaps and SVG Lucide icons follow/copied from Afio.
- General preferences: cache quota, gallery card width, launch view and explicit
  capture-on-launch preference. General settings live in the SQLite preferences
  table. A quota below current cache use is rejected without deleting anything.
- Ctrl+K opens Actions. Select a command with arrows; Ctrl+Shift+Comma starts its
  key recorder. Settings → Keyboard uses the exact same recorder/API.
- Record a chord including Ctrl/Alt/Shift, or Ctrl+Backspace to clear; Escape
  cancels. Conflicts offer Replace, Retry and Cancel with keyboard controls.
  JSON overrides persist in `katran-data/shortcuts.json`.
- Ctrl+K and Ctrl+Comma, Escape, Tab and ordinary arrow navigation are protected.
  Windows/Super chords are not supported in this adapter. Assignments are local
  to the Katran window; they are not system-global hotkeys.
- While editing text, normal text copy/paste/delete remain text operations.

## Updates and scrolling

Clipboard sequence is polled every 250 ms only when capture is enabled. Expensive
image/file reads occur only for a changed sequence. Search/status are refreshed
after new objects or explicit actions, not on each poll. Idle paused UI has no
poll timer. Thumbnail results wake the UI themselves.

Keyboard navigation reveals a row only when outside the viewport, rather than
centering each selection. The Actions list reveals selection once on navigation
or search changes. F5's shortcut badge has no periodic refresh animation.

## Delete

Delete removes a shelf event, never its external source file. A single object is
immediate; a group opens a confirmation accepted by Enter, canceled by Escape.
Confirmation holds exact event IDs, independent of new arrivals/selection changes.
An owned blob is reclaimed only after its last event reference disappears.
The response includes `cleanup_pending` if event removal committed but physical
cache cleanup failed. This is not deletion to the system Recycle Bin.

## Copy

Ctrl+C and the Copy action use `copy_events {ids}`. A single supported image
publishes full-size oriented pixels (CF_DIBV5) in addition to the standard Windows
CF_HDROP file list. Groups publish CF_HDROP; Preferred DropEffect requests copies.
Consumers choose supported formats. Pixel decoding has a 32-megapixel budget;
unsupported/large images remain file payloads with an explicit warning.
Responses report `copied`, per-item `failures` and `warnings` after the native
clipboard write succeeds. Katran acknowledges its own clipboard sequence to avoid
re-ingesting the copy operation. Text-field Ctrl+C remains text copy.

Cached blobs receive extension-bearing hardlinks under `katran-data/exports`
(fallback copies). They survive a shelf deletion so a pending paste remains valid.
These clipboard snapshots currently have no automatic cleanup, are outside the
blob quota, and may retain storage after shelf deletion. Repeated exports need
bounded lifecycle management before treating this as a production cache.

## Provenance

New clipboard arrivals store `origin.source_app` and, if CF_HTML's header supplies
a valid HTTP(S) SourceURL, `origin.source_domain`. Full URLs, process paths,
window titles and browser history are not stored. The app tag is the clipboard
owner process (a clipboard manager may become owner). If unavailable, origin is
unknown; the foreground app is never guessed. Only stream rows show origin tags
in this iteration. Main search also matches stored app/domain fields. Historical
events stay unknown; no retroactive attribution. Dedicated source sorting is
future work.

## API additions

`get_settings`, `set_settings`, `show_settings`, `close_settings`, `shortcuts`,
`set_shortcut`, `delete_events`, `ingest_with_origin` use API version 1 and the
same Rust dispatcher as the UI. `RunAction` resolves the common catalogue.
`delete_events` takes exact `ids` and `confirmed` (default false), returns
`confirmation_required`, normalized IDs, `deleted`, `cleanup_pending`.
`ingest_with_origin` wraps only `ingest_path` or `ingest_bytes` with origin fields.

## Validation

`cargo test --offline --features ui` and
`cargo clippy --offline --all-targets --features ui -- -D warnings`.
Synthetic API/headless tests cover settings persistence, shortcut conflicts,
selection and confirmation, owned-vs-referenced deletion, schema migration and
provenance parsing, scrolling and polling. These checks do not manipulate the
owner's real clipboard or desktop. Native appearance and paste compatibility
require owner verification.

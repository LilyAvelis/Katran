# Developer and agent entry point

Read `AGENTS.md`, current `README.md`, then this map and the relevant API test.
Current source structure is listed below. The audit extraction plan has been
implemented in 0.5; historical reviews describe their examined snapshots.

| Responsibility | Current owner files |
| --- | --- |
| Wire types / dispatcher | `src/api.rs`, `src/lib.rs` |
| SQL, recovery, durable blobs / preview decode and cache | `src/storage.rs`, `src/preview.rs` |
| Arbitrary-file gallery/stream visuals / Windows Shell thumbnail and icon boundary | `src/file_visual.rs`, `tests/file_visual.rs`, `docs/file-visuals.md` |
| Native input adapter / action definitions and routing | `src/capture.rs`, `src/actions.rs` |
| Clipboard signals / bounded retry API / hidden-window client (default backend) | `src/clipboard_listener.rs`, `src/capture_events.rs`, `src/bin/clipboard_events_ui.rs` |
| Selection, cursor, view, preview/settings state | `src/interaction.rs` |
| Image zoom/anchor/pan bounds and canvas geometry | `src/preview_view.rs` |
| General preferences and validation | `src/settings.rs` |
| Shortcut catalogue, normalization, persistence/conflicts | `src/shortcuts.rs` |
| Native copy payload preparation/publication | `src/copy_out.rs` |
| Two-phase drag lifecycle / native OLE adapter / cursor image | `src/drag_session.rs`, `src/drag_out.rs`, `src/drag_image.rs` |
| Window-thread native drag handoff | `src/bin/native_drag_ui.rs` |
| Exact-ID deletion, shared blob cleanup | `src/delete_shelf.rs` |
| Provenance snapshot/parsing | `src/origin.rs` |
| CLI JSON-line transport | `src/main.rs` |
| UI shell / jobs / preview worker / palette / list-grid | `src/bin/ui.rs`, `api_client.rs`, `images.rs`, `palette.rs`, `shelf_view.rs` |
| Gallery geometry / image fit / clipped labels | `src/bin/gallery_card.rs` |
| Local logging / viewport input | `src/diagnostics.rs`, `src/bin/input_log.rs` |
| Settings / key recorder / shared visuals | `src/bin/settings_ui.rs`, `shortcuts_ui.rs`, `ui_foundation.rs`, `ui_style.rs` |
| Shared text fallback (Windows fonts, loaded once) | `src/bin/ui_fonts.rs` |
| Window/taskbar identity / embedded executable resources | `src/bin/app_identity.rs`, `build.rs`, `assets/branding/` |
| Stable launch / per-user Start Menu registration / single-instance activation | `src/bin/launcher.rs`, `app_registration.rs`, `single_instance.rs` |
| Action SVGs / capture status | `src/bin/action_icons.rs`, `capture_lamp.rs` |
| Build/install pointer | `scripts/build-working.ps1`, `Start Katran.cmd` |

## Handoff · 2026-11-09 · 1.0.7

The next local preview is 1.1.0: `src/shelf_tabs.rs` owns tab browsing and ShareX
ingest orchestration; `src/sharex.rs` owns bounded read-only folder discovery.
`src/interaction.rs` owns tab memory independently of Gallery/Stream. See
[tabs and ShareX](tabs-and-sharex.md) for API contracts and manual acceptance.
The following 1.0.7 handoff remains the accepted baseline pending owner review.

The owner accepted the current build after manual testing. Main includes the
accepted event-driven clipboard branch; no pending migration or feature work is
required to finish this session. Continue from this version rather than the old
experimental executable or historical shelf snapshots.

- Default capture uses a hidden Win32 listener with bounded retries, independent
  of whether the visible window is minimized. Legacy polling is available only
  for explicit comparison; see `clipboard-events-experiment.md`.
- Preview: wheel zooms toward the pointer, Ctrl+wheel toward the center. Above
  fitted scale (1×), primary drag pans and native export is disabled, including
  Ctrl+drag. At 1×, preview drag exports; gallery drag exports as before.
- The last fix resolves gesture intent even when a worker owns the dispatcher or
  a texture has not loaded. Do not restore an unconditional drag-export fallback
  outside the preview geometry block.
- Latest verification: 21 UI/headless tests and all-target/all-feature Clippy
  passed. The preview regression covers missing textures, busy dispatcher and
  Ctrl input using synthetic data. The owner then confirmed the fix works.
- Normal installed executable: immutable build `6ae338dd4a69`, version 1.0.7.
  Start Menu/Raycast and `Start Katran.cmd` use the stable launcher; rebuild with
  the script below. Do not interrupt the owner's running application.

## Small extension recipe

1. Define the command/result, error and side-effect contract. Put behavior in an
   ordinary Rust module and connect it through the one dispatcher. Never write a
   second action implementation in a widget.
2. Write a synthetic API test first, including invalid/partial inputs and restart
   when data persists. Native adapters need a fake boundary: no real clipboard
   contents, personal files, GUI launch or desktop automation for tests.
3. Add the definition and route to `actions.rs`; Actions and shortcuts derive from
   that shared table. Add a GUI icon if needed; the generic fallback is safe.
   `commands` is intentionally hidden in the palette; capture's label is dynamic.
4. Add the UI as a client of that command. Handle all errors/partial results; do
   not infer successful writes from sending a request. Test keyboard behavior
   headlessly, and leave physical Windows verification to the owner.
5. A new setting requires a backward-compatible default or migration and a test
   loading old JSON before restart. Never silently replace corrupt preferences.
   A new source uses ingest commands and evidence-based origin fields; a new view
   uses the interaction API for selection/navigation.

## Parallel ownership

Give each agent a bounded module, test files and an agreed input/output contract.
One integrator owns edits to the shared `Command` enum, dispatcher, schema and
catalogue. Workers propose their integration snippets instead of modifying these
same files concurrently. New code should not enlarge the existing GUI/dispatcher
monolith without a reason. Follow the staged extraction plan in the audit;
neither a plugin framework nor a separate crate per command is required.

## Checks and installation

For authorized GitHub pushes, run `python scripts/push-private.py`. It reads
`GITHUB_TOKEN` from the process environment or Git-ignored `.env.local`; it never
retrieves 1Password itself or puts the token in the remote URL. Do not print,
commit or upload that local file. Rotate the local value when the PAT expires.

```powershell
cargo fmt --check
cargo test --offline --features ui --target-dir target/build
cargo clippy --offline --all-targets --features ui -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-working.ps1
```

Use `target/build` for tests to avoid overwriting an old directly launched debug
binary. Existing capture tests query only the system clipboard sequence number;
they never read or write clipboard contents. Release installation emits a source
hash manifest and switches `Start Katran.cmd` only after a successful build.
Do not launch apps or kill the owner's running process to complete a build.


## Local 1.2.0 Test · current additions

- `file_visual.rs`: PreviewFile fallback pipeline; image large preview unchanged.
- `folder_watch.rs`: bounded native per-source discovery/hash workers.
- `folder_commit.rs`: prepared source commits and classification input snapshots.
- `downloads.rs`: Known Folder resolution, CLI scan, injectable recycler.
- `screenshot_identity.rs`: background pixel digests and cheap association commits.
- `bin/source_jobs.rs`: GUI clients, Clipboard priority and one prepared file/turn.
- `delete_selection.rs`: ownership-sensitive delete; Downloads recycler,
  Screenshots permanent, ordinary shelf entries shelf-only. Single immediate,
  groups Enter confirmation. See docs/downloads.md and docs/latency.md.

No GitHub publication until the owner's 2.0 release instruction. Source folder
workers start when owner runs the new build; tests use synthetic temp roots.


## Local 1.2.1 Test

`tab_layout.rs` validates persisted non-All order and maps positions. Commands
SelectTabPosition/MoveTab share settings persistence. `bin/tab_bar.rs` emits
commands for click/drop; UI consumes unmodified Tab before button focus traversal,
with actual search/dialog/recorder gating. `quicklook.rs` owns validated external
preview requests with injectable launcher; TogglePreview/OpenCurrent route files
through it, retaining native supported-image preview. See docs/quicklook.md.


## Local 1.2.2 Test

`density.rs` and AdjustDensity provide independently persisted gallery_columns
and stream_rows, with backward-compatible defaults4/3 and validated1..12.
CtrlMinus/Plus/Equals route through the catalogue; egui keyboard UI zoom disabled
before input. UI clips content within fixed-font cards and clamps effective
count to viewport minimums. QuickLook readiness now refuses duplicate spawning
when its instance mutex exists without an accepting pipe; test transport uses
isolated named pipes and injected launcher boundaries, never installed QuickLook.


## Local 1.2.4 Test · Actions polish

The shared catalogue groups actions contiguously into existing shelf/file/view/app
sections. All exposed catalogue actions have specific Lucide icons. Palette rows
reserve disjoint clipped label and shortcut regions; only the first alias is shown
as keycaps, full aliases appear on hover. Command IDs, routing and bindings remain
unchanged. Headless regression tests cover icon coverage and narrow-row geometry.
QuickLook failure also reproduces in Explorer per owner; no QuickLook settings,
plugins or installation changes performed in this iteration.


## Local 1.2.5 · regression repair

QuickLook direct pipe access removed; external preview delegates to desktop CLI.
See docs/quicklook.md for source, local recovery and verification limits. Palette
row clips intersect ancestor scroll clipping; regression renders real ScrollArea
rows and verifies shapes at1x/1.5x, including fully offscreen rows. Restoring old
set_clip_rect behavior causes the regression to fail.

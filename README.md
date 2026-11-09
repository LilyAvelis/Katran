# Katran

Clipboard events are now the default after the owner's successful browser and
filesystem checks. The accepted `experiment/clipboard-events` branch is merged
into `main`. [Capture contract and verification](docs/clipboard-events-experiment.md).

Independent, local Windows-first object shelf in Rust. API First: UI, capture,
CLI and future integrations use one versioned Rust command dispatcher.

## Current build · 1.0.6 Release

Close the previous window yourself once after this update, then use **Katran** in
the Start Menu/Raycast or **Start Katran.cmd**. The title strip
must say **Katran · 1.0.6 · Release**. Existing data stays in `katran-data`.

Prepare an updated build without overwriting a running executable:

On a fresh checkout, first run `cargo fetch --locked` to populate dependencies.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-working.ps1
```

The script builds an optimized release into `target/build/release`, copies a successful binary to an immutable
hash-named `runtime/versions` directory, then changes the launch pointer. Do not
launch old `target/debug`, `target/capture` or `target/shelf` previews.

Each installed version includes `build-manifest.json` with profile, version,
executable SHA-256 and hashes of its source files. Developer/agent entry point:
[module map and extension recipes](docs/development.md).

Katran's shark icon is embedded into the window and Windows executable. A stable
application ID identifies its taskbar group. It remains a portable application;
an installer is unnecessary for its icon. [Artwork and build details](assets/branding/README.md).

The build also installs a per-user `Katran.lnk` Start Menu entry pointing to
`runtime/Katran.exe`. This stable launcher follows the current-version pointer.
Repeated launches request activation of the existing window for the same shelf
root, including restoration from minimized state. In Raycast, assign its app
hotkey under Settings → Applications and select Hotkey Action → Always Show.
[Activation, registration and manual checks](docs/application-launch.md).

## Controls

| Control | Action |
| --- | --- |
| Ctrl K | Actions: search, arrows, Enter, Escape |
| Ctrl , | Settings: general and keyboard |
| Ctrl Shift , in Actions | Record shortcut for selected command |
| Arrow keys | Navigate gallery; Up/Down in stream |
| Click / Ctrl-click / Shift-click | Single / individual toggle / range |
| Shift + arrows | Range selection |
| Enter / double-click | Open large fitted image preview |
| Space | Toggle large preview / shelf; arrows browse adjacent objects |
| Wheel in large preview / Ctrl + wheel | Zoom toward pointer / canvas center |
| Middle-button drag in large preview | Pan the enlarged image |
| Primary drag in enlarged preview | Grab and pan the image |
| Drag a card / fitted preview; Ctrl + drag in preview | Native copy-only drag-out; selected group / current image |
| Escape | Close preview/dialog/palette |
| Ctrl C | Copy selected files/images to the Windows clipboard |
| Delete | Remove from shelf; groups require Enter confirmation |

Preview zoom follows the Afio pattern: 1–16×, clipped to the image canvas; changing
the image or closing/reopening preview resets the fit. Window dimensions stay
unchanged. See [zoom and font contracts](docs/preview-zoom-and-fonts.md).

Use Actions to enable/pause capture, select gallery/stream, refresh or save copies
to a folder. A green/red lamp next to Actions shows the current capture state.
Capture is disabled at startup unless explicitly changed in Settings.

Copied Explorer files are references; browser **Copy image** creates a cached PNG.
Text/URL clipboard contents are not archived. File drops are supported. Capture
uses Windows change notifications with short bounded retries. Its message-only
listener and background logic continue while the window is minimized or hidden;
the process must stay running. Very fast intermediate copies can still be
coalesced because Clipboard holds a current snapshot, not a history. Downloads/
watch-folder ingestion and capture after the app exits are not implemented yet.

Stream origin tags identify the clipboard owner's app and, if supplied in
CF_HTML, the browser domain. Unknown/old origins remain unknown. Full URLs and
browser history are not collected. Origin tags appear in gallery and stream.
Dedicated source sorting is future work.

Delete never removes external original files. Owned blob files are reclaimed
after their last event reference disappears. Clipboard exports are separate
snapshots retained for paste; they are not immediately removed by shelf deletion.

## API

```powershell
cargo run --offline --bin katran -- serve-stdio <explicit-data-directory>
```

One JSON request per input line, one response per output line. Diagnostics go to
stderr. Example requests:

```json
{"api_version":1,"request_id":"1","command":{"type":"status"}}
{"api_version":1,"request_id":"2","command":{"type":"list","limit":100}}
{"api_version":1,"request_id":"3","command":{"type":"actions"}}
{"api_version":1,"request_id":"4","command":{"type":"get_settings"}}
{"api_version":1,"request_id":"5","command":{"type":"copy_events","ids":[1,2]}}
{"api_version":1,"request_id":"6","command":{"type":"delete_events","ids":[1,2],"confirmed":false}}
```

Responses echo `request_id`, include `api_version`, `ok`, and `result` or
`error: {code,message}`. Inspect per-item results for batch copies/imports.
Import never overwrites existing files, selects numbered names on conflict and
does not remove sources. Delete confirmation must resubmit the exact returned
IDs with `confirmed:true`. General settings persist in SQLite; shortcut overrides
in JSON. UI interaction state is process-local until the planned IPC layer.

See [drag-out and Space preview](docs/drag-and-preview.md), [1.0 gallery layout](docs/version-1.0.md),
[0.5 changes and diagnostics](docs/version-0.5.md) and the
[development map](docs/development.md). Shelf 03 documents are historical.

## Verification and boundaries

```powershell
cargo test --offline --features ui
cargo clippy --offline --all-targets --features ui -- -D warnings
```

Automated checks use synthetic files and headless egui. The owner checks native
Windows appearance and paste compatibility. Tests never replace her clipboard
or manipulate the desktop.

Current boundaries: no global hotkey, autostart, watch folders, video thumbnails,
Afio adapter or protected IPC. Previews are fitted images, not original-pixel zoom.
Blobs default to a 2 GiB quota with reject-new policy. Clipboard snapshots have a
separate budget of the same size, including the previous and prepared batch; old
batches retire only after successful clipboard replacement. Persistent thumbnails
have a 128 MiB disk budget and the UI has bounded workers/LRU texture storage.
Owned crash-orphan reconciliation runs at startup/maintenance; boot-session
retention is still future work. Use one window/service writer per data directory.

Afio UI/components and clipboard conventions are reused under the retained
[MIT notice](AFIO-LICENSE), without a runtime dependency on Afio.

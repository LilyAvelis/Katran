# Katran

> **Local Katran 2.5 preview is under development.** The accepted public
> release remains Katran 2.0.0; 2.5 is not accepted or published. Its current
> branch uses Folder in place of All and recycles managed originals, including
> ShareX screenshots. See [Folder](docs/work-folder.md), [Pinned](docs/pinned.md),
> and the [2.5 preview notes](docs/releases/2.5.0-preview.md). Build details:
> [preview build procedure](docs/patch-2.5/BUILD.md).

Clipboard events are now the default after the owner's successful browser and
filesystem checks. The accepted `experiment/clipboard-events` branch is merged
into `main`. [Capture contract and verification](docs/clipboard-events-experiment.md).

Independent, local Windows-first object shelf in Rust. **Katran 2.0.0** was
accepted by the owner after Windows checks of text, Markdown, Links, table
transfer between Google Sheets and Notion, sorting, and zoomable previews.
API First: UI, capture, CLI and integrations use one Rust command dispatcher.
[2.0 release notes](docs/releases/2.0.0.md).

Download the **portable ZIP** or **per-user setup EXE** from
[GitHub Releases](https://github.com/LilyAvelis/Katran/releases).
Portable: extract the entire archive and run `Katran.exe`; data stays beside it.
Setup: installs without administrator rights and keeps data in
`%LOCALAPPDATA%/Katran/data`. Settings → Обновления checks manually and installs
an available update for the next launch. [Distribution and update contract](docs/distribution.md).

## Current release · 2.0.0

Close the previous window yourself once after this update, then use **Katran** in
the Start Menu/Raycast or **Start Katran.cmd**. The title strip
must say **Katran · 2.0.0**. Existing data stays in `katran-data`.

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

[Tabs and ShareX setup, API and limits](docs/tabs-and-sharex.md) records the
earlier source setup. Current visibility and file-opening API behavior is in
[tab visibility and opening](docs/tab-visibility-and-opening.md). The 1.5 release documents remain historical snapshots; current 2.0 behavior is
documented with each feature.

Accepted 2.0 contract: Images is the clipboard/manual image shelf; Screenshots
and Downloads are live views of originals. Downloads uses the Windows Known
Folder by default; Settings can disable it or override the root. In the local
2.5 preview, All is replaced by Folder and managed originals in Downloads,
Screenshots, and Folder use Windows recycling. Single deletion is immediate;
groups confirm with Enter. This preview behavior is not part of the accepted
public 2.0 release.
[Downloads](docs/downloads.md), [file visuals](docs/file-visuals.md), and
[hardening contracts](docs/domain-jobs.md) and [current measurements](docs/reviews/latency-1.3-hardening.md) describe the accepted 1.5.0 boundaries. The older latency document records its measured snapshot.

Tabs are positional: 1..9 select enabled non-All tabs in their saved order. Drag tabs
with the mouse to reorder; All stays pinned. Tab toggles All and the remembered
tab, including when a button has focus. Text fields/dialogs retain local input.
Tab positions are registered in the shared shortcut catalogue; direct source
commands no longer have Ctrl J/H/D defaults. Existing user overrides are retained.
Settings → Tabs eye buttons hide a tab and stop its associated collection;
the remaining number positions close the gap. Existing data is retained.

Enter uses the default app for the current file. Ctrl Enter reveals the exact
file in Explorer; Ctrl Shift Enter opens Katran's bottom
[Open With app picker](docs/open-app-picker.md). Space previews supported
images, Text fragments, Links, Tables, and `.md` / `.markdown` files in Katran; other files use
installed Windows QuickLook. The explicit QuickLook action remains available
without a shortcut.
Katran prepares QuickLook in the background at startup. The verified
fast QuickLook is the main installation at its normal path. Katran starts it
automatically when absent and reuses it when ready. Closing Katran leaves
QuickLook running. [Fast backend, installation, rollback and limits](docs/quicklook-fast-backend.md).

Ctrl− / Ctrl+ changes content density by one: gallery columns (default 4) or
stream rows (default 3), saved independently. Fonts, controls and DPI remain
unchanged. Preferences are bounded to 1..12; narrow/short windows show fewer
cards to preserve 96 px column / 72 px row minimums. Settings expose both values.

## Controls

In the local pre-3.0 candidate, **Shift+Tab moves** the selection between a
category and Folder. **Ctrl+Tab copies**, preserving the current view and
selection so repeated presses make additional copies. Folder image imports are
independent managed objects. See [the transfer contract](docs/work-folder.md).
Reusable [UI patterns](docs/design-patterns.md) and the
[library extraction map](docs/reusable-library-map.md) guide future work.

| Control | Action |
| --- | --- |
| Ctrl K | Actions: search, arrows, Enter, Escape |
| Ctrl , | Open/close Settings |
| Shift Tab / Ctrl Tab | Move / copy between the shelf and Folder |
| Comma | Sort menu: arrows + Enter, or t / e / m / r / n / s / a / o; Shift reverses except shuffle |
| Ctrl Shift , in Actions | Record shortcut for selected command |
| 1..9 | Select numbered tab in compact visible order |
| Arrow keys | Navigate gallery; Up/Down in stream |
| Click / Ctrl-click / Shift-click | Single / individual toggle / range |
| Escape | Close the active panel/preview first; otherwise clear selection and retain cursor |
| Shift + arrows | Range selection |
| Enter | Paste selection into the captured previous application |
| Alt Enter / double-click | Open current file in its default app |
| Ctrl Enter | Reveal current file in Explorer |
| Ctrl Shift Enter | Katran's bottom [Open With app picker](docs/open-app-picker.md); “Другое приложение…” opens the Windows chooser |
| Space | Preview images, Text, Links, Tables, and Markdown in Katran; QuickLook other files |
| Ctrl Left / Ctrl Right in text or Markdown preview | Markdown / original source; stays on the current item |
| Ctrl C / drag with only Links selected | Copy URLs as text / drag one combined links.txt |
| Ctrl C / drag with only Tables selected | Copy cells as HTML + TSV / drag a separate CSV per table |
| Ctrl Left / Ctrl Right in Table preview | Grid / CSV source |
| Wheel / Ctrl-wheel in Table grid | Zoom at pointer / centre; left or middle drag pans, “Вписать” fits the table |
| Wheel in large preview / Ctrl + wheel | Zoom toward pointer / canvas center |
| Middle-button drag in large preview | Pan the enlarged image |
| Primary drag in enlarged preview | Grab and pan the image |
| Drag a card / fitted preview | Native copy-only drag-out; selected group / current image |
| Escape | Close preview/dialog/palette |
| Ctrl C | Copy all-Text selections as joined plain text; mixed/non-Text selections use file copy |
| Delete | Remove from shelf; groups require Enter confirmation |

Preview zoom follows the Afio pattern: 1–16×, clipped to the image canvas; changing
the image or closing/reopening preview resets the fit. Window dimensions stay
unchanged. See [zoom and font contracts](docs/preview-zoom-and-fonts.md).

Use Actions to enable/pause capture, select gallery/stream, refresh or save copies
to a folder. When Text is enabled, Actions also offers **Копировать тексты одним
текстом**, joining selected fragments in selection order. A green/red lamp next
to Actions shows the current capture state. Capture is disabled at startup
unless explicitly changed in Settings.

Copied Explorer files are references; browser **Copy image** creates a cached PNG.
Arbitrary regular files remain transferable. Gallery and stream use Windows
Shell thumbnails when available, then associated file icons or a generic icon.
See [file visual API and limits](docs/file-visuals.md).
Clipboard Text is optional and off by default. When enabled with capture, plain
Unicode clipboard text is saved as Text fragments; URL-looking strings remain
unclassified text. [Text fragment behavior, limits and API](docs/text-fragments.md).
Ctrl+C joins all-Text selections with `\n\n`; selection order follows Ctrl
toggle order or the current visible order for Shift ranges. Drag always exports
files. Markdown previews offer rendered and source views with conservative
formatting; HTML is displayed as literal text.
File drops are supported. Capture
uses Windows change notifications with short bounded retries. Its message-only
listener and background logic continue while the window is minimized or hidden;
the process must stay running. Very fast intermediate copies can still be
coalesced because Clipboard holds a current snapshot, not a history. Capture after the app exits is not implemented. ShareX is opt-in; Downloads
is enabled by default. Both sources use native folder notification workers.

Stream origin tags identify the clipboard owner's app and, if supplied in
CF_HTML, the browser domain. Unknown/old origins remain unknown. Origin tags do
not collect full URLs or browser history; plain Unicode clipboard text can be
saved as a Text fragment when Text is enabled. Origin tags appear in gallery
and stream. Source sorting (comma, then o; Shift+O reverses) uses recorded
domain, app or source category; it does not infer a download website from a file.

Shelf-only DeleteEvents never removes external original files. User-facing
DeleteSelection follows the ownership-specific rules above. Owned blob files are reclaimed
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

Current boundaries: no global hotkey, autostart, built-in video decoding,
Afio adapter or protected IPC. Preview is fitted by default, with bounded image
zoom and pan controls described above.
Blobs default to a 2 GiB quota with reject-new policy. Clipboard snapshots have a
separate budget of the same size, including the previous and prepared batch; old
batches retire only after successful clipboard replacement. Persistent thumbnails
have a 128 MiB disk budget and the UI has bounded workers/LRU texture storage.
Owned crash-orphan reconciliation runs at startup/maintenance; boot-session
retention is still future work. Use one window/service writer per data directory.

Afio UI/components and clipboard conventions are reused under the retained
[MIT notice](AFIO-LICENSE), without a runtime dependency on Afio.

Local 2.5 navigation: Ctrl+Shift+Left/Right moves between visible categories in saved order and exits pinned, including at the ends. It never enters Folder; use Tab for Folder. Tab is routed before widget focus traversal when returning from another application; search editing and dialogs retain their own keyboard focus.

Optional Pods is the seventh category: enable it in Settings → Вкладки. Emoji and symbol-only captures are deduplicated, support pinned and render in color. [Classification, limits and graphics attribution](docs/pods.md). Settings → Основные can assign an independent global summon chord; Alt+Enter copies the selection and requests paste into the previous application. [Summon, residency, quit and Windows limits](docs/overlay.md). These are local pre-3.0 changes, not a published release.

Resident mode: Katran has a system-tray icon with Show / Hide / Quit. The assigned global chord shows Katran from another app and hides it when Katran is active. Closing the resident window hides it; Quit exits. Native tray behavior awaits owner acceptance; no startup registration is installed.

Experimental appearance: Settings → Темы selects Standard (default) or Blue Experimental, with accent/decor controls. [Theme foundation and designer handoff](docs/themes.md). This local prototype is separate from the accepted behavior and has no published release. [Azuros review of81abc6f](docs/reviews/2026-11-11-azuros.md) records five open P2 findings.

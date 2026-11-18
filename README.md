# Katran

Ctrl+K → AI Commands offers four independent image-upscale actions using
existing local ComfyUI weights. See [static image upscale](docs/patch-4.0/IMAGE-UPSCALE.md).

Local 4.0 work adds Ctrl+K → Video → Extract audio. Katran saves the selected
track as a separate AAC/M4A file, adds it to Files, and copies it as a file.
See [audio extraction](docs/patch-4.0/AUDIO-EXTRACTION.md).

The local Translate shelf is reserved for completed image translations.
See [the Translate foundation](docs/patch-4.0/TRANSLATE.md).

Preview handoff for the next team: changing a browse filter must not briefly
blank an already loaded, unchanged Link/Text preview. Keep ready content separate
from async request generations; retain it only for matching item/revision/source
in the same namespace, while rejecting old replies. See the [Mini stream follow-up](docs/patch-3.8/MINI-STREAM-LAYOUT-INVESTIGATION.md).

Local video Actions: Ctrl+K → Video offers Pause/Resume (P by default,
rebindable) and Extract current frame. Pause the video first: the source-size
PNG goes to Images and the image is copied to Clipboard without leaving playback.
If copying fails, the saved image is retained and the message explains the failure.

Local follow-up: [video with sound](docs/video-preview.md) inside Space preview
and Mini stream, with Play/Pause, seeking and volume overlaid on the video.
Space starts playback immediately; Mini stream waits for Play. Closing or changing
the viewed object stops playback. Uses a separately installed pinned libmpv
runtime; this local addition is separate from the v3.8.0 release tag.
P pauses/resumes a video; confirmed seek prevents a stale-position jump. Controls
fit large UI sizes. Settings → General offers optional GPU video acceleration,
off by default, with actual renderer/decoder status and CPU initialization fallback.
Video shortcuts: Ctrl+←/→ seeks1second, Shift+←/→10seconds; Alt+←/→ pauses and
steps by a frame, with bounded hold-repeat paced by confirmed decoded frames.

**Katran 3.8.0** adds: per-tab tag sets with preservation of
legacy assignments, inline live tag settings, drag reordering of complete tags
and improved Folder navigation. The third [Mini stream view](docs/patch-3.8/DETAILS-VIEW.md)
is available through Actions → Display and Settings → Appearance → Tab view.
Optional [local neural Folder search](docs/patch-3.8/FOLDER-NEURAL.md) toggles with
physical backslash in the folder picker; ordinary search remains available.
Neural queries submit on Enter with 10 results per page; Down at the last result
requests the next 10. Settings can opt into a separately installed CUDA component.
The bounded neural index persists across restarts and updates changed descriptors.
Settings → Tabs can assign a [global Windows shortcut to each tab](docs/patch-3.8/TAB-GLOBAL-HOTKEYS.md),
opening Katran directly on that tab with its saved view.
Settings → [AI](docs/patch-3.8/AI-COMPONENTS.md) offers independent optional
components, installed and verified in a visible terminal with a saved log.
The [Enter follow-up](docs/patch-3.8/PASTE-IMPLEMENTATION.md) follows the last
external window on returning to Katran, guards pending paste intent and fixes
file Clipboard ownership. Global tab bindings now accept Ctrl+Shift+Slash.
The [file receipt correction](docs/patch-3.8/PASTE-SEQUENCE.md) settles and verifies
the clipboard sequence after publication teardown before authorizing Enter.
The [inline editor follow-up](docs/patch-3.8/TAG-EDITOR-UX.md) releases tag modals
on exit, retains image thumbnails during style edits and adds Ctrl+Enter to the
refined pinned-properties form.
The same preview retention covers pinning, assigning tags and renaming items,
while the shelf still recomputes search/filter/order metadata.
Comma then Slash selects [tag-order sorting](docs/patch-3.8/TAG-SORT.md): current
tag positions first, untagged last; custom pinned order remains separate.
See the [3.8 release notes](docs/releases/3.8.0.md) and
[AI setup on another PC](docs/ai-setup.md). Models are optional downloads;
installation runs visibly and preserves a diagnostic log. Before upgrading,
retain a profile backup: schema v13 and new saved view/sort modes require a
compatible rollback. Release assets are produced by the Windows release workflow.

**Katran 3.7** adds an emoji catalogue in Pods, a Desktop background library,
stronger file operations, and a new application icon. The latest patch unifies
the application and improves Desktop menus. See the
[3.7.1 release notes](docs/releases/3.7.1.md).

Independent, local Windows-first object shelf in Rust. API First: UI, capture,
CLI and integrations use one Rust command dispatcher. Clipboard events are the
default Windows capture backend; there is no silent polling fallback.

Download the **portable ZIP** or **per-user setup EXE** from
[GitHub Releases](https://github.com/LilyAvelis/Katran/releases).
Portable: extract the entire archive and run `Katran.exe`; data stays beside it.
Setup: installs without administrator rights and keeps data in
`%LOCALAPPDATA%/Katran/data`. Settings → Updates checks manually and installs
an available update for the next launch. [Distribution and update contract](docs/distribution.md).

## Current release · 3.7.1

Close the previous window yourself once after this update, then use **Katran** in
the Start Menu/Raycast or **Start Katran.cmd**. The title strip reports **3.7.1**
in Settings (compact themes hide the title strip). Existing data stays in its
configured profile. A full setup or fresh portable extraction updates the
launcher icon; the in-app updater changes the application executable only.

Video backgrounds use your own file. Portable/setup include FFmpeg components;
users upgrading through the in-app updater can prepare them explicitly in
Settings → Themes → Video background. No video is downloaded automatically.
With Wallpaper Engine installed and running, Actions can send one selected video
from Desktop, Folder, Downloads or Files to it as a Windows video wallpaper.
An additional action sends a copy with its audio track removed. Wallpaper Engine
applies the video to its first monitor by default. Katran keeps a durable copy
and lists successfully sent videos in Desktop; the source may later move or disappear.

## One Katran · 3.7.1

This patch merges the former E emoji AI search into the public application. It uses the existing Pods catalogue and a separately installed local model; the installer does not contain model weights. Without the model, ordinary Katran and literal emoji search continue to work and AI search explains how to install it. See [optional emoji AI setup](docs/emoji-ai.md). No account or payment check is included in this patch; authorization is a separate 4.0 decision.

Desktop Actions resolves the current asset once, while Sorting requests only sorting commands. This avoids repeated scans of saved backgrounds while those menus are open.

## 3.7 implementation history

The 3.7.0 source includes the hardening recorded in the
[development log](docs/releases/next.md). A separate local E build was used for
the first emoji AI experiment; 3.7.1 merges that feature into this
source. That E build is historical migration input rather than the future app.

Local contracts include the opt-in lossless JSON stdio protocol v2 for large
IDs, domain-owned Copy behavior, context-bound Desktop cursors, managed-path
preview selection, schema v12 migrations, complete build provenance, and
safer managed-media publication/action checks. See the
[wire v2 contract](docs/wire-v2.md) for client requirements. A9 product
handoff automation and N1 Folder transfer responsiveness remain open. The former
public and E builds shared the Pods/Desktop/settings database. The local launcher
now selects unified 3.7.1 (`d7eae2862b31` on 2026-11-16).
The local shared profile reached schema v12 on 2026-11-15 after E installation;
its integrity check passed and a pre-v12 backup was created. A compatible local
public v12 binary is staged at `runtime/versions/28c8fb0b5446/katran-ui.exe`.
The published public 3.4.1 and old E runtime cannot open this v12 profile;
neither is selected by the local launcher.

Saved images in Images use extensionless internal blob paths. The unified source
offers both Katran background and Windows desktop wallpaper Actions for these
images using their managed image type; the selected bytes are checked when the
action runs. Files lists file pointers, so a saved image normally appears in
Images instead.

Transfer, import, paste and move preparation now use background jobs, and the
repeated Ctrl+Tab queue case is covered. Public targeted UI checks passed
201/201 and transfer/import/Wallpaper Engine checks passed 23/23; E N1 API
checks passed 27/27 with a targeted UI check. Final recycle/cut completion still
uses the Service thread, so transfers are not fully nonblocking. Synthetic
debug medians (1/16/64 MiB: 48/458/2,078 ms) are not release or real-Recycle-Bin
measurements. See [the development map](docs/development.md) for the remaining
worker ownership constraint.
The local source now recycles verified external originals during moves and
reports any recovery location per item; direct deletion of those originals was
removed from this path. Synthetic fake-Trash API checks passed 2/2, with no
user files involved.
For known file selections totaling at least 16 MiB or 20 distinct files, a
dimmed window and glass status card appear before Folder transfer or physical
deletion starts. Smaller selections do not flash the card; shelf-only removal
does not need it. Unknown-size external clipboard pastes retain the ordinary
status text. Folder Ctrl+Shift+C uses its background import job.

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
root, including restoration from minimized state. Assign Katran’s own global shortcut in Settings. The same shortcut shows or
hides the tray-resident window over the current application.
[Activation, registration and manual checks](docs/application-launch.md).

[Tabs and ShareX setup, API and limits](docs/tabs-and-sharex.md) records the
earlier source setup. Current visibility and file-opening API behavior is in
[tab visibility and opening](docs/tab-visibility-and-opening.md). The 1.5 release documents remain historical snapshots; current 2.0 behavior is
documented with each feature.

Accepted 2.0 contract: Images is the clipboard/manual image shelf; Screenshots
and Downloads are live views of originals. Downloads uses the Windows Known
Folder by default; Settings can disable it or override the root. Folder replaces the former All tab. Managed originals in Downloads,
Screenshots, and Folder use Windows recycling. Single deletion is immediate;
groups confirm with Enter. These operations report per-item failures and protect pinned objects.
Changing the ShareX folder removes old screenshot references from Katran while
leaving the original files untouched. Refresh beside its path applies the entered
folder and requests a rescan at any time. General settings save
automatically; folder paths apply after leaving their text field. Save settings
remains available for an explicit save.
[Downloads](docs/downloads.md), [file visuals](docs/file-visuals.md), and
[hardening contracts](docs/domain-jobs.md) and [current measurements](docs/reviews/latency-1.3-hardening.md) describe the accepted 1.5.0 boundaries. The older latency document records its measured snapshot.

Tabs are positional: 1..9 select visible numbered tabs in their saved order. Drag tabs
with the mouse to reorder; Folder stays separate. Tab toggles Folder and the remembered
tab, including when a button has focus. Text fields/dialogs retain local input.
Tab positions are registered in the shared shortcut catalogue; direct source
commands no longer have Ctrl J/H/D defaults. Existing user overrides are retained.
Settings → Tabs eye buttons hide a tab and stop its associated collection;
the remaining number positions close the gap. Existing data is retained.

Alt Enter or double-click opens the current file in its default app. Enter pastes
the selection into the previously active application. Ctrl Enter reveals the exact
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

**Shift+Tab moves** the selection between a
category and Folder. **Ctrl+Tab copies**, preserving the current view and
selection so repeated presses make additional copies. Folder image imports are
independent managed objects. See [the transfer contract](docs/work-folder.md).
Reusable [UI patterns](docs/design-patterns.md) and the
[library extraction map](docs/reusable-library-map.md) guide future work.

| Control | Action |
| --- | --- |
| Ctrl K | Actions: search, arrows, Enter, Escape |
| Ctrl , | Open/close Settings |
| Ctrl T | Focus/unfocus shelf search |
| Ctrl . / . | Pin selection / show only pinned items |
| Ctrl Alt Up/Down | Move selected pinned item earlier/later in custom pinned order (visible with pinned sort) |
| Shift Tab / Ctrl Tab | Move / copy between the shelf and Folder |
| Comma | Open/close Sort menu; arrows + Enter or a sort key; Y shows pinned in custom order first |
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
to Actions shows the current capture state. Capture is enabled at startup
unless explicitly changed in Settings.

Copied Explorer files are references; browser **Copy image** creates a cached PNG.
Arbitrary regular files remain transferable. Gallery and stream use Windows
Shell thumbnails when available, then associated file icons or a generic icon.
See [file visual API and limits](docs/file-visuals.md).
Clipboard Text is optional and off by default. When enabled with capture, plain
Unicode clipboard text is saved as Text fragments; HTTP(S) URLs are classified
as Links when that tab is enabled. [Text fragment behavior, limits and API](docs/text-fragments.md).
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
in JSON. UI selection and preview state are process-local; per-tab sort and display preferences persist.

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

Current boundaries: no autostart, Afio adapter or protected IPC. A configurable
global hotkey and optional video background are available. Preview is fitted by default, with bounded image
zoom and pan controls described above.
Blobs default to a 2 GiB quota with reject-new policy. Clipboard snapshots have a
separate budget of the same size, including the previous and prepared batch; old
batches retire only after successful clipboard replacement. Persistent thumbnails
have a 128 MiB disk budget and the UI has bounded workers/LRU texture storage.
Owned crash-orphan reconciliation runs at startup/maintenance; boot-session
retention is still future work. Use one window/service writer per data directory.

Afio UI/components and clipboard conventions are reused under the retained
[MIT notice](AFIO-LICENSE), without a runtime dependency on Afio.

Ctrl+Shift+Left/Right moves between visible categories in saved order and exits pinned, including at the ends. It never enters Folder; use Tab for Folder. Tab is routed before widget focus traversal when returning from another application; search editing and dialogs retain their own keyboard focus.

Optional Pods captures emoji and symbol-only content, deduplicates it and renders it in color. Local unreleased work also adds a read-only Unicode 16 emoji catalogue with category filtering. Ctrl+C adds selected catalogue emoji to saved Pods and copies them; Enter pastes into the previous application, and drag-out offers text and a pods.txt file. See [Pods controls, classification and graphics attribution](docs/pods.md). Settings → General can assign a global summon chord. Alt+Enter opens the selected item. [Summon, residency, quit and Windows limits](docs/overlay.md).

Resident mode: Katran has a system-tray icon with Show / Hide / Quit. The assigned global chord shows Katran from another app and hides it when Katran is active. Closing the resident window hides it; Quit exits. No startup registration is installed.

Appearance: new profiles start with Blue Experimental and Lily background 1; existing profiles keep their saved theme. Settings → Themes includes Standard, glass, video and animation options. In Actions, a selected image can also be saved as a durable copy and passed to Windows as desktop wallpaper on all monitors; Windows does not document per-virtual-desktop targeting through this API. The optional Desktop tab shows managed background copies as ordinary shelf objects with standard selection, navigation and Actions. Settings → General has an optional main-window resizing switch, off by default. [Theme foundation](docs/themes.md). The [Azuros review of 81abc6f](docs/reviews/2026-11-11-azuros.md) is a historical snapshot; its five findings were addressed in the later follow-up.

Desktop shelf browsing reads managed-file metadata; background Actions validate the selected file before changing the theme or Windows wallpaper. Display names do not need a filename extension.
Desktop supports the same selection, pins, tag slots, copy, drag-out and Folder transfer commands as other file tabs. Applying a background accepts one selected asset; moving to Folder retains a pinned or active background in Desktop.

The optional Desktop tab shows Katran-managed still backgrounds and video backgrounds, including images sent to Windows. Hiding this tab does not stop new backgrounds from being saved. You can reuse a saved image for Katran or Windows, reuse a video for Katran, and delete inactive managed copies to reclaim space. The currently selected Katran background and the last wallpaper set by Katran remain protected from deletion; Windows does not expose a reliable inventory of all virtual desktops' wallpaper references through the documented API.

Local development storage uses one Cargo directory, `target/build`, with guarded
[build-cache retention](docs/devops.md#local-build-storage-budget--owner-incident-2026-11-18).
Default budgets are 32 GiB for target and 8 GiB for incremental state; installed
runtime, optional models, user data and backups are separate.

Local Mini stream layout follow-up reserves preview space while content loads,
so metadata keeps its position. Details/evidence: [layout investigation](docs/patch-3.8/MINI-STREAM-LAYOUT-INVESTIGATION.md).

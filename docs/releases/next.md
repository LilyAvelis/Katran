# Local release notes and development log

## Local follow-up · Notion search

Ctrl+Shift+K adds a read-only Notion mode alongside Everything. Ctrl+N or the
header icon switches modes. Ctrl+Shift+N opens the Notion mode directly.
Space reads Markdown v2 with native formatting, callouts, tables, toggles and
Notion reference cards. Alt+Enter opens the selected page in the browser. Search titles,
pin pages, revisit page history, and attach a Markdown file to the previous window.
Physical backslash switches to database search. Enter opens the selected data
source in the standard Folder shelf. Search Shelf, Actions and the sort menu
apply to the active Notion source. Mini stream uses its details pane for row
properties. Ctrl+C copies the selected rows' loaded properties, and drag
exports separate Markdown files. The picker also accepts database IDs and
Notion page links and shows visited databases in History.
Folder restores the active database, filter and reader after switching shelf
tabs. Ctrl+T applies a Notion title filter to the active data source, and Ctrl+.
in the picker pins the highlighted database or page to the mixed Favorites list.
Inside a database page, the reader has a graduated tint, persistent background
controls, Landscape and Cinema actions, and independent Ctrl+Plus/Minus text zoom.
The official Notion CLI uses a Windows Credential Manager token from Settings → Integrations.
[Setup and limits](../notion.md)

## Katran 4.0 · Accepted local build

Lilia closed the 4.0 feature run after checking the local workflow. The final
feature checkpoint is `c4fc0a6` and the verified Raycast runtime is
`23df78286fa2`. The executable still reports package version `3.8.0`. Public
4.0 packaging, tagging and publication are separate from this local checkpoint.
Use the [engineering handoff](../patch-4.0/TEAM.md#final-handoff) for acceptance
details and rollback.

### Translate

The local Translate tab is restricted to internally published managed image
results. The independent Rust collection preserves page order. This change
introduces catalogue schema v14. Rollback to a v13 binary requires the
pre-migration profile backup. The local actions for Japanese, Simplified Chinese,
Traditional Chinese and Korean use Qwen3 4B Instruct on the GPU and save an English PNG
in Translate. OCR and Ollama keep
their models loaded between pages until Katran exits. Translate shows results
under tag sorting. One request translates every recognized region on the
page. Readable black-on-white placements can appear near a recognition region
or in the closest available page margin. Matching colored numbers link the
source regions to margin cards. Single-page output opens in QuickLook;
multiple selected pages share one collection and a built-in page viewer.
Failed pages in a partially translated input BULL keep their original images
in place. The completion dialog shows which pages failed and why.
It also shows skipped lettering regions by original page number. Both Chinese
commands apply the same geometry-based vertical reading order.
Japanese and Chinese translation can combine fragments from one enclosed
bubble. Side cards follow their reading numbers.
The viewer keeps its page frame stable and supports the same anchored zoom
as image preview. Page turns retain the previous image without a loading
label. BULL cards show a BK badge. Image translation settings
offer a numbered-only layout.
Lilia accepted the local experience on 2026-11-18. On the approved complex
Japanese page, warm end-to-end translation went from 21.6 s to 3.7 s.
Paddle's first pass fell from 15.1 s to 0.6 s by removing its redundant
recognition pass and enabling oneDNN for detection. See the separate
warm/cold and renderer measurements in the Translate contract.
See [Translate](../patch-4.0/TRANSLATE.md).

Ctrl+K now replaces selected images or BULLs with one ordered collection on
Images or Translate. The action also unpacks a single BULL into its original
cards. The first page is the cover. Pinned inputs give the collection a pin,
and matching tags carry over. The local profile uses catalogue schema v15;
previous builds require the pre-v15 backup for rollback.
Ctrl+C in the BULL viewer copies the single displayed page at its stored
resolution. It stays out of clipboard capture and the Images shelf.

### BULL and BS collections

Reingesting Text or Links and unpacking portable collections resolve only
visible shelf cards when deduplicating. Hidden members keep their ownership.

Upscale and Translate can take one image BULL or an ordered selection of
loose images. Both produce a separate BULL for batch inputs and retain the
source. Upscale saves to Images and Translate saves to Translate.

Links uses portable BULL v2 for ordered URLs with saved titles, descriptions
and optional previews. The same shelf action packs, merges and unpacks.
Custom link names appear as titles and travel with newly packed collections.
Existing local collections display names from their original shelf cards.
The Links BULL viewer can rename the current entry and save its title
inside the collection.
Ctrl+C copies the visible URL and Alt+Enter opens it in the default browser.
The current Windows QuickLook adapter supports image BULL v1. Katran's own
viewer handles Links BULL v2.

Links BULL creation publishes the local file before network work. A background
job fills missing titles and descriptions in the same card. It preserves saved
previews and skips automatic preview-image requests.

Text BULL saves ordered UTF-8 entries and offers Markdown and source views.
Ctrl+C in the viewer copies the current source only. Enter on a visible BULL
entry uses the standard previous-window paste flow for images, Links and Text.

Files and Folder use one rebindable collection action. It selects BULL for
compatible files and BS for a mix. Raw file preview centers a thumbnail or
Windows file icon with the name. Shelf Delete removes the selected collection
and its hidden member cards. Folder packs replace the selected files after
archive verification, preserve the original names and bytes inside the
archive, and show a first-entry cover. Unpack restores files directly into
Folder and recycles the archive after verifying all outputs. Ctrl+E renames
unpinned collection cards and archive files in Folder. Verified completed
recycle stages are cleaned up automatically.

Video entries in BULL and BS play with the usual Katran video controls.
Page turns and closing the viewer stop playback and release the temporary
video file.
The viewer centers the collection name and uses arrow buttons around its page
counter. Space or the top-right cross closes it. Alt+Enter opens the current
entry where supported. The first page prepares during the 300 ms entrance and
video starts afterward. Video page turns keep the previous frame visible
while the next page prepares.
BULL has a contextual fullscreen action in Ctrl+K with F11 as its default
shortcut. The title and pager hide when the image is zoomed in fullscreen.
Versioned Raycast builds find the shared video player installed by the
local setup script.

### Google Lens

Ctrl+K offers a rebindable **Search image with Google Lens** action for one
selected image. The default Windows browser receives a one-shot local
handoff, then sends the image and follows the result in one browser session.
This corrects the initial split between an app-side upload and a browser
navigation, for which the owner reported an empty result. The upload uses a
generic filename and no embedded metadata. The current endpoint follows
[Chromium's Lens upload form](https://chromium.googlesource.com/chromium/src/+/HEAD/chrome/browser/resources/new_tab_page/lens_form.ts).
The endpoint is a web integration and may change independently of Katran.

### Static image upscale

AI Commands adds four independently rebindable commands for exact ×2 and ×4
Remacri and Real-ESRGAN output. Settings → AI reports discovered local files.
See [the integration contract](../patch-4.0/IMAGE-UPSCALE.md).

### Audio extraction

Stage 1 extracts a selected audio track to a distinct AAC/M4A file with
cancellation before publication. The result appears in Files and is copied
to the clipboard as a file. See the [audio contract and
verification](../patch-4.0/AUDIO-EXTRACTION.md).

## Local follow-up after v3.8.0 · 2026-11-18

[Video with audio](../video-preview.md) adds playback in Space and Mini stream,
pause, seek and volume overlaid on the canvas. Space autoplays; Mini waits for
Play. One dispatcher-owned session stops on close, object/surface change or hiding.
The first Media Foundation candidate failed owner Folder/Unicode checks. A pinned
libmpv runtime replaces it, retaining domain policy and bounded CPU-frame output.
This work has no schema migration and does not alter the immutable v3.8.0 tag.
See the verification handoff for automated evidence and owner acceptance status.

Owner confirmed Folder/Desktop playback. Subsequent local polish adds P pause,
large-control positioning and stable asynchronous seek UI. Optional General
GPU video acceleration defaults off, exposes actual renderer/decoder status and
falls back to CPU when GPU initialization fails. The GL path still reads bounded
RGBA through CPU memory; no zero-copy or measured speedup is claimed.

Owner accepted GPU/P/seek/layout. The next local follow-up adds Ctrl/Shift arrows
for1s/10s seeks and Alt arrows for native paused frame stepping with bounded hold.
One step is confirmed before the next; release/input ownership changes cancel
repeat. Decoder/reverse seek limits prevent a guaranteed monitor-Hz rate.
Owner then reported panel/time flicker only during frame steps. The UI keeps
paused inspection controls and confirmed frame position through native transient
states, with stable counter width, returning to normal behavior after completion.

Local 2026-11-18 follow-up: [Tags sorting](../patch-3.8/TAG-SORT.md) is comma then
Slash, current tag order with untagged last; custom pinned order remains separate.
[Image refresh correction](../patch-3.8/TAG-EDITOR-UX.md) now also covers rename,
pinning, assignment and ordinary same-namespace generation changes. Native owner
acceptance of this follow-up remains separate from synthetic evidence.

## Local 3.8 work · 2026-11-17

The 2026-11-18 [inline editor follow-up](../patch-3.8/TAG-EDITOR-UX.md) fixes tag
modal dismissal and image-cache churn during live styling, and refines pinned
properties with Ctrl+Enter. Native owner acceptance remains separate.

The Notion-browser follow-up found `clipboard_changed` before SendInput despite
successful manual paste. File receipts now settle after owner teardown with exact
payload proof, retaining strict delivery checks. See [evidence](../patch-3.8/PASTE-SEQUENCE.md).

The subsequently authorized [Enter follow-up](../patch-3.8/PASTE-IMPLEMENTATION.md)
implements observed destination/immutable paste leases and owned file Clipboard
publication. Global binding recording/parser now supports Ctrl+Shift+Slash.
Native Notion/chat delivery remains owner acceptance, not inferred from `sent`.

Settings → AI now owns explicit visible-terminal component installation and
functional verification, independent Folder CPU setup and optional CUDA. See
[the setup contract](../patch-3.8/AI-COMPONENTS.md) for evidence and remaining
clean-machine acceptance. [Enter audit](../patch-3.8/PASTE-AUDIT.md) is complete;
the target/focus/file-publication fixes are not part of this setup change.

Global tab shortcuts are implemented through the domain summon API, with
settings recording and actual Windows registration status. See
[the follow-up contract](../patch-3.8/TAB-GLOBAL-HOTKEYS.md). Enter interoperability
is separately under the owner-approved deep audit.

The owner confirmed the local base tag/search experience and requested complete
tag reordering in Settings; see [the team and contract](../patch-3.8/TEAM.md) and
[local candidate notes](3.8.0.md). Local main and the primary Raycast installation
with the existing profile are explicitly authorized; GitHub publication is not.
The later owner instruction implements the third Mini stream in Display/settings
and optional local neural Folder search with a physical backslash switch.
Ordinary search stays available without a model. Older checkpoint paragraphs
remain dated evidence of the earlier research-only scope.

## 3.7.1 unified application

Lilia chose one Katran application after the separate local E build developed a native background regression while the compatible public build still played the same video. The 3.7.0 release files are unchanged. Version 3.7.1 carries emoji AI search into the public host as an optional local feature, with no authorization or payment gate yet. Model weights, Python environment and generated index are installed separately, never bundled with Katran. The `EmojiAiStatus` command and AI mode explain when the model is missing; ordinary Pods and wallpaper functionality remain independent. The active profile is the stable home for the runtime, so app updates do not have to copy it. See [model setup](../emoji-ai.md). Lilia confirmed the unified local AI search, backgrounds and Desktop responsiveness before release.

Desktop menu performance follow-up: the owner isolated lag to Desktop after changing a background; image and video backgrounds worked, while Actions and Sorting slowed and Tags did not. `Actions` had repeatedly resolved the current Desktop asset while constructing five background commands on each frame. It now classifies that asset once, and `SortActions` avoids asset resolution entirely. Synthetic API tests cover availability and action-boundary validation; Lilia confirmed the installed GUI was responsive. She also confirmed a second comma closes Sorting without applying a new order.

This page preserves pre-release checkpoints and their then-current status.
For the published 3.7.0 behavior, see [3.7.0 release notes](3.7.0.md).

## PRODUCT-10 hardening in the local source

This work is not a public release. The published public release remains 3.4.1;
the local launcher selects private E runtime `4f53749f7293`. There is
no release acceptance or GitHub publication claim here. Targeted checks passed
for the listed boundaries, but the integrated source has not completed its final
combined validation. An internal status ledger tracks session ownership and the
remaining work. Public and private E share Pods/Desktop/settings storage. The
local profile reached schema v12 on 2026-11-15; the pre-v12 backup and integrity
check succeeded. A compatible local public v12 build is staged as
`runtime/versions/28c8fb0b5446`; published public 3.4.1 and previous E
`c1555b47dc04` cannot open this profile without restoring its earlier database.
Source checkpoint `e6453c5` includes the N1 job preparation, recoverable
original moves, Wallpaper Engine and decoder-stop changes. No new public release
is installed.

- **Images background Actions:** copied image blobs have extensionless digest
  paths. Both Katran background and Windows wallpaper Actions now accept their
  `ManagedImage` type while validating actual bytes at execution. A focused
  Images browse/select API regression covers both Actions, successful Katran
  apply, safe Windows job cancellation and stale bytes without changing real
  desktop wallpaper. Files remains the shelf for file pointers; saved images
  belong to Images.

- **A1 — public UI focus:** category selection now reads the focused widget ID
  before loading text-edit state, avoiding nested egui context locks. The
  targeted UI regression passed 1/1; Windows owner acceptance remains separate.
- **A2 — external JSON clients:** stdio protocol v2 supports lossless large
  numeric IDs using canonical decimal strings, including job and window IDs,
  while v1 and the in-process Rust API retain their existing representation.
  Real-process round-trip and invalid-value checks passed 4/4. See
  [wire v2](../wire-v2.md); the E copy was updated without a separate E test.
- **A3 — Copy action:** the domain action owns catalogue import, clipboard
  publication and per-item/clipboard outcomes, so GUI and API clients share the
  same composition. Public and private E targeted tests passed 2/2 each.
- **A4 — Desktop pagination:** cursors bind to the browse query, sort, filter
  and catalogue context. The public API test passed 1/1; E code was mirrored,
  with no E test reported.
- **A5 — managed preview:** preview capability uses managed file type/path,
  including extensionless display names. The public regression passed 1/1.
- **A6 — durable schema:** Desktop pin/order tables are installed by numbered
  migration v12. The targeted upgrade/data-preservation test passed 1/1;
  rollback after an injected migration failure was not tested. Real shared
  database migration is gated on v12 support in both installed editions.
- **A8 — build provenance:** local build manifests now include toolchain,
  target, features, profile, source identity and compiled-resource hashes;
  release metadata derives its data schema version from the migration source.
  The synthetic package-contract check passed, including resource-change
  detection; the manifest measured 55,607 bytes, below the 64 KiB updater
  limit. No product build was run.
- **N1 — Folder transfers:** copy/import/paste and move preparation run in
  background jobs; repeated Ctrl+Tab queueing is fixed. Public UI checks passed
  201/201 and transfer/import/Wallpaper Engine checks 23/23; E N1 API checks
  passed 27/27 and its targeted UI check passed. The final `safe_recycle` /
  `finish_cut` stage remains Service-bound, so this is not end-to-end
  nonblocking. Synthetic debug medians were 48 ms (1 MiB), 458 ms (16 MiB) and
  2,078 ms (64 MiB); they are not release or real-Recycle-Bin measurements.
  Worker ownership still needs an owner-job lease and ready-commit deferral.
  Transfer completion now sends verified Downloads, Screenshots and explicitly
  movable reference originals through the existing handle-bound recycle path
  instead of direct file disposition. Its per-item API result records recycle
  and recovery details; the UI shows a recovery path when needed. Synthetic
  fake-Trash API tests passed 2/2 across direct/job moves and failure modes,
  and the focused UI check passed 1/1. No real originals were used.
- **N2/N3/N4 — managed media:** media bytes and sidecars publish from staged
  files; a pending receipt is recorded before Wallpaper Engine apply, with
  active and pending assets protected from deletion. Ambiguous delivery returns
  `wallpaper_outcome_unknown`, and action lookup validates managed bytes without
  hashing during browse. Control waits are bounded. Targeted synthetic tests
  passed in public and E; modified-video apply passed 1/1 and timeout handling
  passed 1/1.
- **Shared UI/runtime fixes:** empty subgroup dividers are omitted in the public
  category picker. Avilon's Windows media stop path closes the assigned decoder
  process tree. Evidence is scoped to the divider regression and assigned-child
  stop fixture; the spawn-to-assignment window remains a limitation.
- **A7 — private E only:** its independent AI lane and bounded cancellation
  have targeted API evidence (4 passed, 1 ignored). It is not part of this
  public source tree; no model or implementation details are documented here.

**Still open:** N1 final `safe_recycle` / `finish_cut` still runs in Service;
owner-job lease and ready-commit deferral remain needed. Moving this effect to
a worker also requires cross-process protection for destructive file paths and
durable intent/outcome reconciliation; a SQLite writer lock alone is insufficient.
Owner decision on 2026-11-15: after observing roughly one second for a large
transfer, keep this final effect Service-bound for now. Returning control before
completion would need visible in-progress state and rejection of repeated or
conflicting operations across both editions. This is an accepted responsiveness
limit, not a claim that asynchronous transfer is impossible.
The UI now dims the window and shows a glass status card before starting known
Folder copy/move/import or physical-delete selections totaling at least 16 MiB
or 20 distinct files. It paints the card before starting the job, so a heavy
Service-bound effect has a visible cue. Multi-item deletion first shows its
ordinary confirmation; shelf-only removal and small selections do not flash a
card. Unknown-size external Ctrl+V does not currently get this cue. Folder
Ctrl+Shift+C uses its existing background `AddFolderFiles` job instead of the
direct UI call. The overlay is a dim layer, not full-frame optical blur.
Public and E targeted UI checks passed for a painted frame before `BeginJob`,
20 small Folder originals with confirmation and fake Trash, one small file
without an overlay, operation-specific RU/EN labels, duplicate-move rejection
and the existing Ctrl+Tab copy queue. The owner has not yet visually accepted
the new UI in her installed application.
The A9 public/E source handoff check has zero unresolved findings at
the tested source checkpoint `4bf7ba6`. The private E runtime manifest contract has
synthetic checks 3/3; real-model execution, Python environment portability and
signed distribution remain unverified. The latest E full UI suite passed
203/203 after repairs. No E update is installed; regenerate its manifest only
after the source is frozen. Shared-profile schema v12 remains gated on both
installed editions. Final integrated validation and release remain open.

- The Pods emoji catalogue now has an All Categories picker in a compact strip below the tabs, with the nine Unicode groups, searchable keyboard/mouse navigation, and `Shift+\` within the catalogue. Filtering combines with search and retains all pages and subgroup dividers.

- Pods now offers a read-only Unicode 16 emoji catalogue with 3,781 fully-qualified entries, official groups and subgroups, and a visible-card-aware texture cache. Inside Pods, physical `\` toggles the catalogue and `Ctrl+\` selects between it and saved Pods. Ctrl+C adds selected entries to ordinary Pods and copies them; Enter pastes into the previous window through the ordinary copy/paste path. Catalogue cards support drag-out as Unicode text and pods.txt. Entries remain in the catalogue.

- Changing the ShareX screenshot folder now removes Katran's source-owned records for the previous folder, including stale pinned references, without deleting original files. Settings offers an explicit Refresh button next to the ShareX path; it applies an edited path before rescanning.
- General settings now save automatically after a short pause; folder paths wait until the text field loses focus so a half-typed path cannot replace a source. Closing Settings flushes pending changes, and the Save settings button remains available. ShareX Refresh also saves the entered path before rescanning.
- Actions can set one selected image as Windows static desktop wallpaper. The domain command validates the current selection, stores an immutable managed PNG, then asks Windows to use it on all monitors. The documented Windows API does not specify a virtual-desktop target; this command does not offer current-desktop targeting or native Windows video wallpaper.
- Actions can send one selected video from Desktop, Folder, Downloads, or Files to an already running Wallpaper Engine installation through `UseSelectedVideoAsDesktopWallpaper` (`use_selected_video_as_desktop_wallpaper`). Katran keeps an unchanged copy at an ASCII path under Local AppData because Wallpaper Engine misread a Cyrillic source path during the owner's first trial. The original remains untouched; managed copies are not yet automatically removed. The command confirms the CLI process returned success, not that video is visibly playing. This is separate from the Windows static desktop-wallpaper action above.
- A second Wallpaper Engine action sends a copy without audio; it removes the audio stream without changing the source or globally muting other wallpapers. Successfully handed-off videos are also stored as managed Desktop cards, independently of the source tab. The last video registered by Katran is protected from deletion; Wallpaper Engine state changed outside Katran cannot be observed reliably.
- General offers main-window resizing. It is disabled by default, persists in settings, and applies after saving and at startup. Layouts remain optimized for the default window size.
- The optional Desktop tab lists managed image and video backgrounds regardless of tab visibility. It can apply saved images to Katran or Windows, apply saved video to Katran, and delete inactive managed copies. Active Katran backgrounds and the last wallpaper installed by Katran are protected from deletion. The Windows API does not enumerate all virtual-desktop references, so external wallpaper changes cannot be fully tracked.
- Desktop wallpaper success and errors now display briefly in the main window. Success means Windows accepted the request; visual presentation still depends on Windows and other desktop software.
- The Windows wallpaper adapter now allows a local COM server. The previous in-process-only request could fail with `REGDB_E_CLASSNOTREG` before it reached Windows wallpaper setting; this fix addresses the error reported during the owner's local test.
- Desktop assets now appear through the ordinary shelf cards and selection API. Arrow navigation, multi-selection, Actions, preview, sorting and gallery/list preferences use the same path as other tabs; the separate bottom buttons and custom gallery were removed. Desktop uses stable reserved IDs for managed copies, and deletion through the standard command reports per-item failures while protecting active backgrounds.
- Desktop background Actions now recognize managed files by their actual path even when their saved display name has no extension. Desktop browsing and selection read bounded metadata rather than decoding and hashing every stored image on each request; applying or deleting still validates content.
- Desktop now supports durable pins, pinned filtering/order and tag slots. Managed backgrounds also follow the shared multi-object copy, drag-out and Folder transfer paths; an unpinned inactive copy can move to Folder, while pinned or active copies remain in Desktop and are copied. Applying a background still requires exactly one selected image or video.
- Avilon actions now includes host-neutral command palette filtering/selection and stable-ID navigation over a visible list, extracted from the local Telegram Glass experiment. Katran and Telegram UI adapters are unchanged; this does not alter the 3.4.1 binary.

## Local Video actions / extract frame · 2026-11-18

Owner accepted the Alt-step panel stability fix. Ctrl+K now offers contextual
Video commands: rebindable Pause/Resume (default P), and Extract current frame
(no default). On a ready paused/ended video, capture saves a source-size PNG in
Images and copies actual image pixels without changing the current preview.
Clipboard failure preserves Images and reports partial success. Normal managed
storage/copy_out and immutable session/source guards are reused; no schema
migration or new public release. See the video contract and verification ledger.

## Local Mini stream layout correction · 2026-11-18

Reserve the preview slot before async provider rendering, preventing arriving
content from shifting the metadata panel. This targets the reproduced Links
first/next-frame jump; provider/data/playback semantics are retained. No schema
change or public release. Owner check follows primary installation.

## Local residual Link-preview blink correction · 2026-11-18

Keep a successful unchanged Link/Text preview visible across browse-generation
changes such as switching pinned view, while retaining strict request/source
invalidation. This follows the separately accepted Mini stream geometry fix.
Local owner acceptance remains separate from automated checks; no public release.

## Owner ideas for4.0 · research only · 2026-11-18

The owner ended this engineering session after accepting primary09244a0440d7.
Proposed next-team scope: image upscale, possible video upscale/interpolation,
extract audio as a file, batch image/manga translation, batch Text translation
and Google Lens lookup. This is a planning brief, not implementation authorization.
Candidate sources: https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan ;
https://github.com/hzwer/Practical-RIFE ;
https://github.com/zyddnys/manga-image-translator ;
https://github.com/PaddlePaddle/PaddleOCR ;
https://huggingface.co/Qwen/Qwen3-8B ; https://ffmpeg.org/ffmpeg.html ;
https://support.google.com/websearch/answer/1325808 .
Existing optional-component installation uses explicit visible terminal + functional
verification. Future media jobs should preserve originals, return separate results,
retain batch order, support cancellation and bounded temporary storage. No models
were installed/evaluated for this brief; actual speed/quality/device acceptance,
pinned versions and distribution-license choices belong to the next team.

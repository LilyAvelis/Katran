# Unreleased local changes after 3.4.1

## PRODUCT-10 hardening in the local source

This work is not a public release. The installed public release remains 3.4.1;
the installed private E runtime `c1555b47dc04` predates these changes. There is
no release acceptance or GitHub publication claim here. Targeted checks passed
for the listed boundaries, but the integrated source has not completed its final
combined validation. An internal status ledger tracks session ownership and the
remaining work. Public and private E share Pods/Desktop/settings storage, so
the schema v12 migration must not be applied to a real user database until both
installed editions support it. The installed public 3.4.1 and E runtime
`c1555b47dc04` remain pre-v12; no real user database has been migrated.
Source checkpoint `e6453c5` includes the N1 job preparation, recoverable
original moves, Wallpaper Engine and decoder-stop changes. No new release is
installed.

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
The A9 public/E source handoff check has zero unresolved findings at
`e6453c5`. The private E runtime manifest contract has
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

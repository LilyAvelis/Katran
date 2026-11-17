# Developer and agent entry point

Build/test outputs belong in `target/build`. Read the [storage budget](devops.md)
before cleanup: `maintain-build-cache.ps1` plans by default, `-Apply` uses guarded
native deletion; build-working invokes it after a successful install. Preserve
release artifacts/dependencies and all runtime/model/profile data. This tooling
change requires no Rust rebuild.

Tag-order sorting uses `sorting.rs`, `catalogue_query.rs` (SQL keyset buckets),
`desktop_projection.rs` (metadata snapshot), and the existing sort action/input
clients. [Contract](patch-3.8/TAG-SORT.md) covers Folder/cursor/rollback exceptions.
The general image context transition now carries resident decoded image textures
within the same namespace while retaining revision keys and stale-job rejection.

Inline tag modal lifecycle belongs to `tag_menu_ui::State::close(ctx)` with
main-frame/visibility integration. `Images::accept_metadata_generation` and
`Shelf::accept_tag_style_update` bridge only proven checked style updates, keeping
preview resources while invalidating stale browse cursors. Pinned properties use
one domain save path for mouse/Ctrl+Enter. See [the UI contract](patch-3.8/TAG-EDITOR-UX.md).
Pin/tag/alias/opener ACKs use `accept_item_metadata_update` to retain image
resources over a proven generation transition while still requerying shelf
metadata, search membership, sorting and cursors. Content revisions remain keys.

File Clipboard receipts are settled after their native owner/open guard are
released. `copy_out` verifies the exact fresh payload under a bounded read lock;
sequence=0 preserves copy/export results while withholding automatic paste.
Do not bypass delivery sequence checks. See [incident and proof contract](patch-3.8/PASTE-SEQUENCE.md).

Enter policy uses `overlay::PasteLease` and `overlay_service::PendingPaste`;
production UI initializes `WindowsHost::with_observer()` before creating its
window. `foreground_windows.rs` owns native event/visit policy; test Services
never install hooks. File publication owns an HWND and returns bounded format
facts through both copy API paths. See [the implementation evidence](patch-3.8/PASTE-IMPLEMENTATION.md).

Optional component setup is API-first through `AiComponentsStatus` and
`LaunchAiComponent`. `ai_components.rs` owns visible launch/receipt state,
`ai_component_bundle.rs` embeds version-matched helpers, and
`folder_cpu_runtime.rs` validates the independent Folder runtime. See
[AI setup](patch-3.8/AI-COMPONENTS.md) before changing installation or packaging.

Local 3.8 global tab shortcuts use `SetTabGlobalHotkey` / `SummonTabOverlay`.
`settings.rs` validates/persists the map; `tab_hotkey_ui.rs` owns registration
lifetimes through `overlay_windows::HotkeyListener`, and Settings shows actual
registration status. See [the contract and evidence](patch-3.8/TAB-GLOBAL-HOTKEYS.md).
Enter interoperability is a separate audit, not fixed by this shortcut feature.

Local 3.8 work (2026-11-17) is tracked in [FEATURE-5](patch-3.8/TEAM.md).
The team reuses five existing named runtime sessions, with single ownership of
shared files. Per-tab tag persistence/migration belongs to Nemi; Everything
candidate acquisition and OS Known Folders to Mira; ranking to Nemi; UI to
Fenna with Iris's UX contract; independent acceptance to Kiki. Polly integrates
the public command enum and dispatcher. The owner's later 2026-11-17 correction
authorizes local main integration and primary Raycast installation using the
existing profile, with rollback backups. No public release or GitHub push is
authorized. The later owner instruction adds an optional local neural Folder mode
after ordinary ranking; ordinary search remains independent of its runtime.
Schema v13 copies legacy tag definitions/materials and assignments into
independent tab scopes, preserving the initial appearance. `tag_slots.rs`,
`tag_service.rs`, `catalogue_schema.rs`, `catalogue_query.rs`,
`catalogue_snapshot.rs` and `desktop_pins.rs` own this contract. Explicit
`ListTagSlotsForTab`/`SaveTagSlotForTab` edit a chosen scope; legacy tag commands
use the active tab. Folder rejects tags. See [migration](patch-3.8/TAGS.md).
`folder_search_rank.rs` ranks candidates from `everything.rs` before the final
limit. `ListKnownFolders` uses an injectable `known_folders::Provider`; opening
still revalidates through `ChooseFolder`. See [search evidence](patch-3.8/SEARCH.md).
The tag palette's local editor reuses `tag_settings_ui::edit_controls`; its
captured tab and slot save through `SaveTagSlotChecked` with the list revision.
`ReorderTagSlots` accepts a full permutation (`order[new-1] = old`) and the
expected revision, moving definitions and assignments atomically. Both Settings
and the palette reject stale saves instead of overwriting another tag after a
reorder. Legacy numeric save commands remain compatible. Ctrl+Shift+Comma edits
the highlighted palette tag without opening Settings. Folder Standard folders
uses the contextual, rebindable `folder_standard` action (default Ctrl+Y).

`View::Details` is the third [Mini stream](patch-3.8/DETAILS-VIEW.md), exposed in
Actions Display and per-tab Appearance settings. `details_view_ui.rs` renders
the cursor-driven split list/detail layout without setting full-screen preview
or modifying selection. Existing read-only workers use browse-context epochs;
`details_metadata_ui.rs` adds bounded latest-only Unicode metadata. Desktop
negative IDs use managed-ID preview commands; Folder snapshots use path/revision.

`BeginJob(SearchFoldersSemantic)` uses the bounded Discovery lane and the
installed integrity-checked E5 runtime with a separate embedded worker. The
[Folder neural contract](patch-3.8/FOLDER-NEURAL.md) records input limits,
incompleteness, errors, cache, cancellation and physical backslash UI. Its worker
source is a compiled input included by `Get-KatranSources`. Runtime absence never
turns a lexical-only reply into neural success.

Latest owner refinements: `SearchFoldersSemanticPage` binds 10-item continuation
to a query/candidate/device/Service snapshot. The picker waits for Enter; Down on
the last item appends the next page. `Settings.folder_ai_gpu` opts into the
isolated CUDA runtime resolved by `folder_gpu_runtime.rs`; disabled settings omit
the new field for compatibility. GPU startup verifies CUDA and fails explicitly.
Mini stream Text uses a shared display-title helper (manual alias before first
nonempty line) and standard pin dots. Ctrl+Left/Right activates text rendering
controls for the Text cursor pane without altering full-screen preview state.

Folder embeddings persist in a separate per-profile/per-device derived SQLite
cache, owned by the embedded worker. `Resident` supplies its path and exposes
bounded cache counts/status through the result; `with_cache_directory` supports
isolated integration probes without writing synthetic entries into a real index.
The worker validates model/preprocessing identity and vector integrity, saves the
current bounded set transactionally, and tolerates optional-cache I/O failures.
No catalogue migration or GUI action/storage path was added.

Earlier research:
[architecture](patch-3.8/DETAILS-VIEW-RESEARCH.md), [UX](patch-3.8/DETAILS-VIEW-UX.md)
and [semantic research](patch-3.8/FOLDER-SEMANTIC-RESEARCH.md) for the earlier study.

Owner decision 2026-11-15: use the public 3.7 source as the sole Katran host. The optional emoji AI mode now follows `SearchEmojiAi` through the standard dispatcher and job service, with `src/bin/emoji_ai_ui.rs` as its UI client. Its runtime lives under the active profile's `emoji-ai-runtime` directory, outside versioned executables; a missing model must not break ordinary Katran. The earlier separate E distribution plan is superseded for 3.7. See [the model setup](emoji-ai.md).

In 3.7.1, `Command::Actions` classifies the selected or cursor asset once for background action availability. `Command::SortActions` returns the same sorted command rows without resolving Desktop assets. `src/bin/palette.rs` uses the latter for Sorting. Keep catalogue construction metadata-only; actual wallpaper commands still validate revisions and bytes at the action boundary.

Public release baseline: **3.7.1** (2026-11-16). For each new release, update this
baseline and the current section of `AGENTS.md` together with the README and
release note. Older numbered sections and reviews are dated evidence, not
current product instructions. Lilia's [author guide](https://app.notion.com/p/41649fda329680eeb371d42dbee517d8)
describes the user experience; its page currently identifies itself as 3.2.3,
so check exact shortcuts and defaults against the current command catalogue.

Folder browsing now prepares each page in a Discovery job and commits only a
matching generation, tab, query and sort. The UI requests refresh and further
pages asynchronously. The selected directory is still enumerated and sorted
for each page; cancellation is checked around, not within, one enumeration.
Do not replace revision checks with an mtime-only cache: a file can change
without changing the directory mtime. Native latency and memory on large or
remote folders have not yet been measured.

Read `AGENTS.md`, current `README.md`, then this map and the relevant API test.
For any new view, popup or dialog, follow [reusable UI design patterns](design-patterns.md): shared tokens/ActionRow, one input owner, and asynchronous native/worker adapters.
For the local reusable-component workspace, see [Avilon framework](garden-framework.md).
Its [design SDK index](../crates/README.md) covers semantic colors, responsive layout,
Acrylic, GPU optics and video-background adapters.
For builds, CI and release retries, follow [DevOps execution rules](devops.md).
The public 3.7.0 source includes the hardening described below. See the
[3.7.0 release notes](releases/3.7.0.md) for user-facing changes and the
[development log](releases/next.md) for implementation evidence and remaining
limitations. The `1.5` labels and ownership section below are dated historical
snapshots; historical reviews describe the snapshots they examined.

The former public and private E builds shared the Pods/Desktop/settings database.
The local launcher now selects unified 3.7.1 via `runtime/current-version.txt`
(`d7eae2862b31` on 2026-11-16), and the shared profile reached schema v12 on
2026-11-15 with a pre-migration SQLite backup and a passing integrity check.
The local public v12 binary is staged at `runtime/versions/28c8fb0b5446`.
Published public 3.4.1 and previous E `c1555b47dc04` remain incompatible with
the migrated profile; restoring either requires a controlled database rollback.

Images stores copied image bytes under extensionless digest paths. Background
Actions admit `ManagedImage` at the metadata boundary; external/Desktop images
still use path type, and `theme_backgrounds::prepare_event` validates revision
and bytes before either Katran background or native wallpaper changes.

Folder transfer/copy/import/paste preparation now runs through background jobs.
Known file selections totaling at least 16 MiB or 20 distinct files get a
busy presentation frame before Folder transfer or physical deletion starts.
The UI holds conflicting input while active and keeps the accepted repeated
Ctrl+Tab copy queue. This is a presentation boundary; `PollJob` remains the
domain commit path. Unknown-size external clipboard paste, smaller selections
and shelf-only removal do not receive the overlay. Multi-item delete shows its
confirmation before the busy state. Folder Ctrl+Shift+C enters the existing
`BeginJob(AddFolderFiles)` route.
Transfer completion uses handle-bound recycling for verified Downloads,
Screenshots and explicitly movable reference originals. `finish_cut` returns
per-item recycle/recovery details; `tests/transfer_recycle_api.rs` covers direct
and job moves with synthetic fake Trash. Do not use direct file disposition for
these originals.
The final `safe_recycle` / `finish_cut` stage remains Service-bound; do not claim
the entire operation is nonblocking. Synthetic debug medians were 48 ms for
1 MiB, 458 ms for 16 MiB and 2,078 ms for 64 MiB; these are not release or real
Recycle Bin measurements. A remaining worker ownership issue needs an
owner-job lease and ready-commit deferral. Public UI checks passed 201/201 and transfer/import/
Wallpaper Engine checks passed 23/23; E N1 API checks passed 27/27 with a
targeted UI check. Repeated Ctrl+Tab queueing is covered. Public category
divider handling now omits empty subgroup labels. Avilon Windows decoder stop
closes an assigned process tree; its stop fixture does not prove containment
before process assignment. Keep that process-spawn window separate from the
Folder job ownership requirement.
An in-memory lease or SQLite writer transaction alone cannot guard a second
Service's file deletion, and a DB rollback cannot undo a completed file effect.
Any worker move needs cross-process effect coordination and durable recovery
before it can replace this Service-bound boundary.

| Responsibility | Current owner files |
| --- | --- |
| Folder session, transfer/copy/import/paste jobs, recycle and explicit import | `src/work_folder.rs`, `src/workspace_service.rs`, `src/transfer_service.rs`, `src/paste_service.rs`, `src/clipboard_files.rs`, `src/folder_delete.rs`, `src/safe_recycle.rs`, `src/folder_import.rs`, `src/jobs_service.rs` |
| 2.5 Pinned metadata, tags and saved application handlers | `src/pinned.rs`, `src/pinned_open.rs`, `src/bin/pinned_ui.rs` |
| 2.5 Everything folder search and negative-ID previews | `src/everything.rs`, `src/preview_snapshot.rs`, `src/bin/workspace_ui.rs`, `assets/everything/` |
| Wire types / dispatcher | `src/api.rs`, `src/lib.rs` |
| Lossless JSON stdio wire protocol v2 for large numeric IDs | `src/api_wire_v2.rs`, `src/main.rs`, `tests/wire_v2_api.rs`, [contract](wire-v2.md) |
| SQL, recovery, durable blobs / preview decode and cache | `src/storage.rs`, `src/preview.rs` |
| Text fragment ingest/read/merge/copy and shared-blob metadata | `src/text_fragments.rs`, `src/text_service.rs`, `src/catalogue_schema.rs`, `src/catalogue_query.rs`, `tests/text_fragments_api.rs`, `tests/text_flow.rs`, `docs/text-fragments.md` |
| Exact HTTP(S) Links / grouped drag / functional tab | `src/link_fragments.rs`, `src/text_service.rs`, `src/drag_session.rs`, `src/copy_out.rs`, `tests/links_api.rs`, `docs/links-design.md` |
| Table value grids / CF_HTML and CSV / rich clipboard publication | `src/table_fragments.rs`, `src/table_service.rs`, `src/table_clipboard.rs`, `tests/table_formats.rs`, `tests/tables_api.rs`, `docs/tables.md` |
| Virtualized table and CSV preview client | `src/bin/table_preview_ui.rs` |
| Table zoom/pan/fit geometry and API checks | `src/table_view.rs`, `tests/table_view.rs` |
| Clipboard Text capture / bounded native Unicode decoding | `src/capture.rs`, `src/capture_events.rs` |
| Lazy text preview client | `src/bin/text_preview_ui.rs`, `src/bin/ui.rs` |
| Arbitrary-file gallery/stream visuals / Windows Shell thumbnail and icon boundary | `src/file_visual.rs`, `tests/file_visual.rs`, `docs/file-visuals.md` |
| Video background import / hardware streaming adapter | `src/video_backgrounds.rs`, `src/bin/video_background_ui.rs`, [contract](video-backgrounds.md) |
| Desktop wallpaper action / Windows COM adapter | `src/theme_backgrounds.rs`, `src/desktop_wallpaper.rs`, `src/theme_service.rs`, `src/actions.rs` |
| Optional Wallpaper Engine video wallpaper action, silent remux and Desktop history | `src/wallpaper_engine.rs`, `src/desktop_assets.rs`, `src/desktop_projection.rs`, `src/actions.rs`, `src/bin/api_client.rs` |
| Unified Desktop library (managed image/video backgrounds, independent of tab visibility) | `src/desktop_assets.rs`, `src/theme_backgrounds.rs`, `src/video_backgrounds.rs`, `src/desktop_projection.rs`, `src/desktop_selection.rs`, `tests/desktop_assets_api.rs`, `tests/desktop_shelf_api.rs` |
| Pods Unicode catalogue / virtual IDs / copy, paste, native text drag and import into saved Pods | `src/pods.rs`, `src/pod_metadata.rs`, `src/text_fragments.rs`, `src/text_service.rs`, `src/copy_out.rs`, `src/drag_out.rs`, `src/interaction.rs`, `src/bin/pods_ui.rs`, `src/bin/category_divider.rs`, `src/bin/emoji_categories_ui.rs`, `src/bin/special_mode_bar.rs`, `tests/emoji_catalogue_api.rs`, [contract](pods.md) |
| Emoji font preferences / profile-local imports / private DirectWrite rendering | `src/emoji_fonts.rs`, `src/emoji_render.rs`, `tests/emoji_fonts_api.rs`, [contract](emoji-fonts.md) |
| Native input adapter / action definitions and routing | `src/capture.rs`, `src/actions.rs` |
| Clipboard signals / bounded retry API / hidden-window client (default backend) | `src/clipboard_listener.rs`, `src/capture_events.rs`, `src/bin/clipboard_events_ui.rs` |
| Selection, cursor, view, preview/settings state | `src/interaction.rs` |
| Catalogue sorting / keyset pagination / comma input | `src/sorting.rs`, `src/shelf_tabs.rs`, `src/bin/sorting_input.rs`, `tests/sorting_api.rs`, `docs/sorting.md` |
| Conservative Markdown parsing and preview | `src/markdown.rs`, `src/bin/markdown_ui.rs`, `tests/smart_copy.rs` |
| Image zoom/anchor/pan bounds and canvas geometry | `src/preview_view.rs` |
| General preferences and validation | `src/settings.rs` |
| Functional tab switches / visible order and navigation | `src/tab_service.rs`, `src/tab_layout.rs` |
| Default app / Explorer reveal / Windows Open With | `src/open_file.rs`, `tests/open_file_api.rs` |
| Associated applications / revision-bound picker sessions | `src/app_handlers.rs`, `src/app_picker.rs`, `tests/open_apps_api.rs` |
| Standalone bottom application picker / shared palette rows | `src/bin/open_apps_ui.rs`, `src/bin/action_row.rs` |
| Shortcut catalogue, normalization, persistence/conflicts | `src/shortcuts.rs`, `crates/avilon-actions` |
| Reusable palette selection and visible-list navigation | `crates/avilon-actions/src/palette.rs`, `navigation.rs` |
| Native copy payload preparation/publication | `src/copy_out.rs` |
| Two-phase drag lifecycle / native OLE adapter / cursor image | `src/drag_session.rs`, `src/drag_out.rs`, `src/drag_image.rs` |
| Window-thread native drag handoff | `src/bin/native_drag_ui.rs` |
| Exact-ID deletion, shared blob cleanup | `src/delete_shelf.rs` |
| Provenance snapshot/parsing | `src/origin.rs` |
| CLI JSON-line transport | `src/main.rs` |
| UI shell / jobs / image and text preview workers / palette / list-grid | `src/bin/ui.rs`, `api_client.rs`, `images.rs`, `text_preview_ui.rs`, `palette.rs`, `shelf_view.rs` |
| Gallery geometry / image fit / clipped labels | `src/bin/gallery_card.rs` |
| Local logging / viewport input | `src/diagnostics.rs`, `src/bin/input_log.rs` |
| Settings / key recorder / shared visuals | `src/bin/settings_ui.rs`, `shortcuts_ui.rs`, `ui_foundation.rs`, `ui_style.rs` |
| Shared text fallback (Windows fonts, loaded once) | `src/bin/ui_fonts.rs` |
| Window/taskbar identity / embedded executable resources | `src/bin/app_identity.rs`, `build.rs`, `assets/branding/` |
| Stable launch / per-user Start Menu registration / single-instance activation | `src/bin/launcher.rs`, `app_registration.rs`, `single_instance.rs` |
| Action SVGs / capture status | `src/bin/action_icons.rs`, `capture_lamp.rs` |
| Build/install pointer | `scripts/build-working.ps1`, `Start Katran.cmd` |
| Managed Desktop assets as standard shelf objects | `src/desktop_assets.rs`, `src/desktop_projection.rs`, `src/desktop_selection.rs`, `src/shelf_tabs.rs`, `tests/desktop_shelf_api.rs` |
| Desktop list performance and background type checks | `src/theme_backgrounds.rs` (`list_fast` for metadata), `src/theme_service.rs` (file path for image/video type), `tests/desktop_shelf_api.rs` |
| Desktop browse cursor bound to query, sort, filter and catalogue revision | `src/desktop_projection.rs`, `src/shelf_tabs.rs`, `tests/desktop_cursor_api.rs` |
| Desktop pin/tag state and multi-object transfers | `src/desktop_pins.rs`, `src/tag_service.rs`, `src/copy_out.rs`, `src/transfer_service.rs`, `src/folder_transfer.rs`, `tests/desktop_shelf_api.rs` |

## Historical 1.5 ownership snapshot

This ownership map records the 1.5 implementation and is retained for history;
it is not a current module map or release baseline.

| Responsibility | Modules |
| --- | --- |
| Ordered schema / indexed shelf projection | catalogue_schema.rs, catalogue_query.rs, shelf_tabs.rs |
| Source ownership, generations, status and private receipts | source_policy.rs, source_runtime.rs, source_service.rs, folder_commit.rs |
| Continuing pixel classification / counters | classification_runtime.rs |
| Bounded jobs / private preparation and commit recipes | domain_jobs.rs, jobs_service.rs |
| Original preparation / authorization / effect / receipts | delete_jobs.rs, delete_selection.rs |
| Native backend ACK / lifecycle / compatibility | quicklook_transport.rs, quicklook_backend.rs, quicklook.rs |
| Killable Shell provider adapter | shell_visual_worker.rs, file_visual.rs |
| Native image capability policy | file_types.rs |
| Measured palette presentation | bin/action_row.rs, bin/ui_style.rs, bin/palette.rs |

Source and catalogue generations are domain state. New adapters must not trust
client-supplied digests, pairs, provenance strings or GUI membership as authority.
See the original audit and [hardening follow-up](reviews/azuros-1.3-hardening-followup.md).

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


## Validation and limits

All-target/all-feature API/headless tests, Clippy and synthetic build-install
failure tests accompany this line. [Independent latency report](reviews/latency-1.3-hardening.md)
distinguishes debug/release, source API response and actual rendering. Source
roots remain bounded to20,000 entries; large catalogue/idle work tests do not remove
that limit. Explicit maintenance/native-finalization limitations remain documented.

The owner accepted this historical baseline as 1.5.0 and authorized its GitHub
push at that time. This is not current publication authorization. See
[release/handoff](releases/1.5.0.md). Do not resume experiments without new scope.

For delegation, read the existing Garden mask map and selected card first. Prefer
the lead alone or one justified helper; use junior/mid masks for routine work,
seniors for genuinely difficult decisions/review. Reuse durable task context and
handoffs; names do not grant persistent memory. Owner cost feedback supersedes
older recipes that automatically assigned every module to a new Sol instance.

Tests must not scan personal roots: Settings synchronizes source authority but
starts no workers until EnsureSources/PollSources. Configure synthetic temporary
roots before those commands. The owner performs actual Windows acceptance.

Pods: src/pods.rs classifies content and supplies the SQL predicate; TextFragment IDs/storage/copy/pinned remain shared. src/emoji_render.rs renders bounded color RGBA in memory with native DirectWrite and bundled regional flags; src/bin/pods_ui.rs is the bounded presentation cache. Overlay: src/overlay.rs owns injectable target/session policy; overlay_windows.rs owns Win32 hotkeys/foreground/input; overlay_service.rs coordinates copy receipt before paste. UI remains a dispatcher client. See pods.md and overlay.md.

ShareX recordings: the read-only scanner accepts MP4/MKV/WebM/AVI/MOV/M4V
in addition to images. Video references remain in Screenshots, retain their
original-file revision/deletion policy, and use the existing Quick Look path.
Only image extensions enter screenshot image classification.
Changing the ShareX root advances the source generation and transactionally removes
old ShareX live events, projections and receipts. This only removes catalogue
references, never filesystem originals. The Settings refresh control dispatches
RescanSource for the saved root; PollSources delivers results asynchronously.
The General settings UI debounces SetSettings and flushes edits on close. Path
text fields defer autosave until focus leaves the field; explicit Refresh saves
the draft through SetSettings before requesting RescanSource.

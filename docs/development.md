# Developer and agent entry point

Read `AGENTS.md`, current `README.md`, then this map and the relevant API test.
For any new view, popup or dialog, follow [reusable UI design patterns](design-patterns.md): shared tokens/ActionRow, one input owner, and asynchronous native/worker adapters.
For the local reusable-component workspace, see [Garden framework](garden-framework.md).
For builds, CI and release retries, follow [DevOps execution rules](devops.md).
Current source structure and1.5 contracts are listed below. Historical reviews describe their examined snapshots; current behavior is in [domain-jobs.md](domain-jobs.md).

| Responsibility | Current owner files |
| --- | --- |
| 2.5 Folder session, transfer, clipboard paste, recycle and explicit import | `src/work_folder.rs`, `src/workspace_service.rs`, `src/transfer_service.rs`, `src/paste_service.rs`, `src/clipboard_files.rs`, `src/folder_delete.rs`, `src/safe_recycle.rs`, `src/folder_import.rs` |
| 2.5 Pinned metadata, tags and saved application handlers | `src/pinned.rs`, `src/pinned_open.rs`, `src/bin/pinned_ui.rs` |
| 2.5 Everything folder search and negative-ID previews | `src/everything.rs`, `src/preview_snapshot.rs`, `src/bin/workspace_ui.rs`, `assets/everything/` |
| Wire types / dispatcher | `src/api.rs`, `src/lib.rs` |
| SQL, recovery, durable blobs / preview decode and cache | `src/storage.rs`, `src/preview.rs` |
| Text fragment ingest/read/merge/copy and shared-blob metadata | `src/text_fragments.rs`, `src/text_service.rs`, `src/catalogue_schema.rs`, `src/catalogue_query.rs`, `tests/text_fragments_api.rs`, `tests/text_flow.rs`, `docs/text-fragments.md` |
| Exact HTTP(S) Links / grouped drag / functional tab | `src/link_fragments.rs`, `src/text_service.rs`, `src/drag_session.rs`, `src/copy_out.rs`, `tests/links_api.rs`, `docs/links-design.md` |
| Table value grids / CF_HTML and CSV / rich clipboard publication | `src/table_fragments.rs`, `src/table_service.rs`, `src/table_clipboard.rs`, `tests/table_formats.rs`, `tests/tables_api.rs`, `docs/tables.md` |
| Virtualized table and CSV preview client | `src/bin/table_preview_ui.rs` |
| Table zoom/pan/fit geometry and API checks | `src/table_view.rs`, `tests/table_view.rs` |
| Clipboard Text capture / bounded native Unicode decoding | `src/capture.rs`, `src/capture_events.rs` |
| Lazy text preview client | `src/bin/text_preview_ui.rs`, `src/bin/ui.rs` |
| Arbitrary-file gallery/stream visuals / Windows Shell thumbnail and icon boundary | `src/file_visual.rs`, `tests/file_visual.rs`, `docs/file-visuals.md` |
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
| Shortcut catalogue, normalization, persistence/conflicts | `src/shortcuts.rs` |
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

## Current1.5 ownership

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

The owner accepted this baseline as1.5.0 and authorized its GitHub push. See
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

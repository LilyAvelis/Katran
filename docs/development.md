# Developer and agent entry point

Read `AGENTS.md`, current `README.md`, then this map and the relevant API test.
Current source structure is listed below. The audit extraction plan has been
implemented in 0.5; historical reviews describe their examined snapshots.

| Responsibility | Current owner files |
| --- | --- |
| Wire types / dispatcher | `src/api.rs`, `src/lib.rs` |
| SQL, recovery, durable blobs / preview decode and cache | `src/storage.rs`, `src/preview.rs` |
| Native input adapter / action definitions and routing | `src/capture.rs`, `src/actions.rs` |
| Selection, cursor, view, preview/settings state | `src/interaction.rs` |
| General preferences and validation | `src/settings.rs` |
| Shortcut catalogue, normalization, persistence/conflicts | `src/shortcuts.rs` |
| Native copy payload preparation/publication | `src/copy_out.rs` |
| Exact-ID deletion, shared blob cleanup | `src/delete_shelf.rs` |
| Provenance snapshot/parsing | `src/origin.rs` |
| CLI JSON-line transport | `src/main.rs` |
| UI shell / jobs / preview worker / palette / list-grid | `src/bin/ui.rs`, `api_client.rs`, `images.rs`, `palette.rs`, `shelf_view.rs` |
| Gallery geometry / image fit / clipped labels | `src/bin/gallery_card.rs` |
| Local logging / viewport input | `src/diagnostics.rs`, `src/bin/input_log.rs` |
| Settings / key recorder / shared visuals | `src/bin/settings_ui.rs`, `shortcuts_ui.rs`, `ui_foundation.rs`, `ui_style.rs` |
| Action SVGs / capture status | `src/bin/action_icons.rs`, `capture_lamp.rs` |
| Build/install pointer | `scripts/build-working.ps1`, `Start Katran.cmd` |

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

# Reusable library map

Walter Wallace · source inventory, 2026-11-11 · baseline `ae9b03b`.

## Extraction update · 2026-11-11

The owner subsequently requested implementation while the immutable release CI
runs. Local branch `codex/garden-framework` now contains `avilon-actions` and
`avilon-egui` (owner name: **Avilon**; former working prefix: Garden), both consumed by Katran through compatibility/style adapters.
They are internal0.1 packages, not published and not part of the3.0 release tags.
See [implemented contracts, usage and verification](garden-framework.md).
The original inventory below records the extraction rationale and later adapter
candidates; its “no extraction performed” closing line describes that review,
not the subsequent implementation.

Katran already contains useful building blocks for another Rust desktop app.
Preserve them now as a source map; extract the first small library **after 3.0**,
when a second real application can exercise its API. This document does not
declare the current prototype accepted, authorize publication, or change code.
Read [development.md](development.md) and [design-patterns.md](design-patterns.md)
alongside it. README release labels can lag the source; native tray/show-hide
acceptance remains separate from headless verification.

## What is available today

| Solution and evidence | Reuse boundary | Work needed before extraction |
| --- | --- | --- |
| One command catalogue: [actions.rs](../src/actions.rs), versioned request/command types [api.rs](../src/api.rs), dispatcher [lib.rs](../src/lib.rs), JSON-lines client [main.rs](../src/main.rs), egui client [api_client.rs](../src/bin/api_client.rs) | Reuse the architecture: stable action IDs, metadata and one domain dispatcher for every client. | `Definition`, `DEFS` and `resolve` currently reference Katran `Command`, interaction and capture state. Inject definitions and emit action IDs/intents; keep Katran command execution in Katran. Do not export its entire service as a generic application engine. |
| Normalization, alternatives, conflicts, explicit replacement, unbinding and JSON overrides: [shortcuts.rs](../src/shortcuts.rs); [contract tests](../tests/shortcuts_contract.rs). Recorder shared by Actions/Settings: [shortcuts_ui.rs](../src/bin/shortcuts_ui.rs) | Pure Rust shortcut policy first; egui recorder second. | Replace direct `actions::DEFS` dependency with supplied definitions. Separate persistence from validation. Katran's Enter migration, protected commands, labels and defaults stay application-owned. `/` means alternative chords here, not a sequence framework. Normalized `Super` does not imply a working global registration on every OS. |
| Ctrl K search/navigation/assignment: [palette.rs](../src/bin/palette.rs); measured icon/label/shortcut rows [action_row.rs](../src/bin/action_row.rs); `KeyCaps` [ui_style.rs](../src/bin/ui_style.rs); app picker [open_apps_ui.rs](../src/bin/open_apps_ui.rs) | egui components and client-side presentation, with action IDs returned to caller. | `ActionRow` is the strongest small component. Palette itself is `impl Shelf`, reads dispatcher JSON and changes Katran state. Separate model/filter/selection from host shell before reuse. Current search is case-insensitive substring matching, not a general fuzzy-search engine. Preserve clipped labels, measured right keycaps and full shortcut tooltips. |
| Comma Sort menu: [sorting_input.rs](../src/bin/sorting_input.rs), [palette.rs](../src/bin/palette.rs), owner ordering in [ui.rs](../src/bin/ui.rs) | Modal input ownership pattern plus egui adapter. | Keep actual sort IDs/modes in Katran. Inject choice list if a second app needs the menu. Preserve physical/text input handling, Russian layout aliases, Shift reverse, arrows, Enter, Escape, repeat rules and event consumption; do not let the same comma/text event reach the shelf. Text editors and other modals must retain ownership. |
| Everything folder query/helper/parser [everything.rs](../src/everything.rs), background search client [workspace_ui.rs](../src/bin/workspace_ui.rs), jobs [domain_jobs.rs](../src/domain_jobs.rs) | Optional Windows index adapter, with pure output parsing and injected runner tests. | Existing capability is **folder search**, expression `folder: <query>`, max 100 results, bounded JSON output and subprocess timeout. It requires a running full Everything with IPC; missing service/helper is an error. This does not implement general file search or an indexer. Keep query revisions/cancel handling so stale worker results cannot replace newer input. Revalidate returned paths before use; they are index snapshots. Inject helper/cache roots and runner rather than retain Katran-specific cache names. |
| QuickLook readiness/process/transport [quicklook_backend.rs](../src/quicklook_backend.rs), [quicklook_transport.rs](../src/quicklook_transport.rs), [quicklook.rs](../src/quicklook.rs); [backend contract](quicklook-fast-backend.md) | Windows optional external-viewer adapter; readiness state machine is separately testable. | Preserve bounded startup/IO, protocol/identity validation, ACK semantics and the `written` uncertainty flag: never replay a Toggle after a possibly delivered request. Current code knows the maintained package, executable identity and stock CLI compatibility. Parameterize these only for a concrete consumer. This is not a cross-platform preview widget or generic process supervisor. |
| Open With discovery and icon cache [app_handlers.rs](../src/app_handlers.rs); revision-bound picker [app_picker.rs](../src/app_picker.rs); [API tests](../tests/open_apps_api.rs) | `Provider` plus native Windows Shell implementation is a good adapter seam. | Discovery returns bounded handlers; icon cache is resource/index keyed, bounded by entries/bytes and expiry. Keep cache policy and fake provider. Event IDs, source revisions, picker sessions and launch authorization remain in Katran. A UI selection is not authority to launch a changed file. |
| Previous-target policy [overlay.rs](../src/overlay.rs), Win32 focus/hotkey/input [overlay_windows.rs](../src/overlay_windows.rs), copy-before-paste [overlay_service.rs](../src/overlay_service.rs), tray signals [tray.rs](../src/tray.rs); [policy tests](../tests/overlay.rs), [API tests](../tests/overlay_api.rs) | Injectable `Host`/session policy plus optional Windows adapters; tray emits signals. | Preserve HWND/process identity, foreground rechecks, modifier release, successful complete copy and verified clipboard sequence before paste. Partial copy must not send paste. An input-send receipt does not establish that the target app accepted its content. Copy preparation, visibility and quit policy belong to the app. Parameterize tray branding/window classes; do not couple generic tray events to process exit. Native tray/global show-hide still needs owner acceptance. |
| Semantic colors, cover background painting [theme_ui.rs](../src/bin/theme_ui.rs), preferences [theme.rs](../src/theme.rs), managed background copies [theme_backgrounds.rs](../src/theme_backgrounds.rs), async picker [theme_file_picker.rs](../src/bin/theme_file_picker.rs), footer [ui.rs](../src/bin/ui.rs) | egui tokens, cover-UV geometry and measured anchor pattern. | Standard's exact legacy-color fallback, opaque-text/alpha-surface behavior and opaque fallback when Acrylic fails are useful contracts. Keep decorative layers outside hit testing/layout. The 34-point status strip is a Katran decision; expose metrics rather than hard-code it for every app. Background import is bounded domain work, not a widget file-copy side effect. Preferences/storage and native glass lifecycle need host adapters. |

## Three boundaries, at most three crates

Proposed names are placeholders, not packages to publish now:

1. **`garden-actions`**: Rust-only action metadata supplied by the host, shortcut
   normalization/assignment/conflict policy, serializable override data and typed
   errors. No egui, Win32, SQLite, Katran command enum or filesystem root. Start
   with shortcuts; do not create a generic dispatcher or plugin runtime.
2. **`garden-egui`**: `ActionRow`, `KeyCaps`, recorder intents, then palette/input
   components once needed. Depends on `garden-actions` and a specified egui
   version; receives data/callbacks and emits intents. Theme/geometry helpers may
   live here without requiring native adapters. The existing egui/eframe 0.36.2
   dependency means other UI frameworks need a new presentation client.
3. **`garden-windows`**, only when a second consumer needs it: optional modules
   for Everything folders, Open With, QuickLook, tray/hotkey and foreground paste.
   Host-neutral policy may use the first crate; Win32/process effects stay here.
   Do not make all consumers bundle ES, QuickLook, image decoding or clipboard
   support. Feature gates and explicit unavailable errors prevent silent fallback.

Dependency direction is host app → components/adapters → small pure contracts.
No library depends back on Katran. File ownership, source containment, recovery,
revision receipts and durable storage remain the host's authority.

In particular [storage.rs](../src/storage.rs),
[catalogue_schema.rs](../src/catalogue_schema.rs),
[source_policy.rs](../src/source_policy.rs), [work_folder.rs](../src/work_folder.rs),
[transfer_service.rs](../src/transfer_service.rs),
[safe_recycle.rs](../src/safe_recycle.rs) and Pods classification are Katran
domain code. SQLite is a successful storage choice, not a reason to export its
shelf schema as universal storage. Destructive paths should not be copied as
standalone helpers stripped of their source/revision/pinned/recovery checks.

## First extraction and practical order

1. Finish 3.0 and record its verified source/artifact baseline. No extraction is
   required for that release. Preserve current working behavior and tests.
2. In the next real Rust app that needs editable shortcuts, extract normalization
   and assignment policy into `garden-actions`. Supply two different small
   catalogues (Katran and the new app). Keep Katran migration/persistence adapter
   and wire commands unchanged. A workspace path dependency is enough initially.
3. Verify aliases/modifier order, alternative-chord overlap, protected commands,
   explicit replacement/unbinding, invalid preferences and no-write-on-failure
   with synthetic catalogues/temp roots. Compare Katran API results before/after.
   A fake persistence failure must not appear as a successful saved binding.
4. Move `ActionRow`/`KeyCaps` into `garden-egui` when that app actually uses egui.
   Add synthetic narrow/wide/DPI layout and keyboard ownership cases. Adapt the
   recorder to return intents; the host alone executes/saves them. Then move
   palette state if both consumers share enough behavior.
5. Extract one demanded Windows adapter at a time. Everything runner fixtures
   cover BOM/invalid JSON/attributes/oversized output/timeouts/unavailable index;
   Open With uses fake providers and stale revision sessions; QuickLook uses fake
   clock/start/transport including initializing, child exit and ambiguous writes;
   overlay uses fake identity/focus/clipboard sequence/input. No test operates on
   personal files, replaces the real clipboard or manipulates the desktop.
6. Keep owner-run Windows acceptance for native focus/paste, Shell results,
   tray restart/show-hide and Acrylic. Headless success is narrower evidence.

Do not promise “usable in any project.” Two apps with different command sets
establish a useful Rust contract; they do not establish support for every UI or
OS. Start with internal `0.x` versions, explicit supported toolchain/egui/Windows
matrix and changelog. Version library APIs separately from Katran's wire
`api_version: 1`; keep its serde IDs/error semantics stable through extraction.
Use a breaking library version for incompatible contracts, or a compatibility
adapter when retaining an old client. Cross-language consumers need a deliberate
IPC/FFI contract, not a direct Rust crate dependency by assumption.

## Cost and provenance

Shortcut policy is the smallest first slice, but still requires untangling the
static catalogue and Katran migrations. Egui extraction costs layout/input
regression work and version coordination. Windows extraction is the largest
maintenance commitment: helper distribution, COM/message loops, process identity,
timeouts, resource cleanup and native acceptance per consuming app. Avoid a
separate crate per menu or command; avoid a new service/plugin system until a real
consumer requires it. There is no measured time-saving multiplier yet.

Carry [Katran MIT](../LICENSE) and retained [Afio MIT](../AFIO-LICENSE) notices
with adapted portions. Everything explicitly records Afio adaptation;
[ES's bundled MIT notice](../assets/everything/LICENSE) must accompany its helper.
Do not assume all adjacent assets inherit the Rust license.

The [QuickLook component](../integrations/quicklook/README.md) is a separate GPL
application with upstream/common notices and source provenance; exporting Katran's
transport client does not relicense or replace those distribution obligations.
Keep its package/source/notices separate and review the intended distribution.

Optional Pods graphics carry [Twemoji attribution](../assets/emoji-flags/ATTRIBUTION.md)
and [CC BY 4.0 graphics terms](../assets/emoji-flags/LICENSE-GRAPHICS); Unicode
data has its [own notice](../assets/unicode/LICENSE.txt). The
[owner-supplied lily artwork](../assets/themes/lilies/README.md) is not a generic
library asset pack: obtain a separate redistribution decision before bundling it
elsewhere. Use neutral fixture artwork. Audit Cargo dependency licenses and keep
third-party/version/hash provenance with any extracted distribution, rather than
copying Katran's entire dependency set.

This is a read-only source review plus this document. No extraction, desktop
check, compilation, release or performance measurement was performed for it.

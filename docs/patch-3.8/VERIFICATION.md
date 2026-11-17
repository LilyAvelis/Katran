# Katran 3.8 verification handoff

## Persistent derived Folder index · 2026-11-17

New worker SQLite persistence and Rust profile-cache wiring were verified with
synthetic-only data, using an isolated temporary cache even when reading the
installed real model/runtime. No personal index was filled with synthetic paths.

- Python cache checks **5/5**: independent process restart with no encode, one
  changed descriptor, deleted-entry pruning, identity/model/tokenizer/sidecar/
  provider incompatibility, corrupt vector handling, lock/bad path/bounds and
  rollback preserving the previous committed cache.
- Existing semantic API **3/3** passed; all-targets Clippy `-D warnings` and
  final formatting passed. One new probe fixture initially missed a borrowed
  Path parameter; corrected before its model execution.
- Actual CUDA API probe across three freshly opened Services/resident processes,
  **8192 synthetic paths**: initial encoded=8192; after restart loaded/reused=8192,
  encoded=0; after one changed path loaded/reused=8191, encoded=1. Every phase
  persisted successfully. This confirms model/IPC/disk behavior, not merely a
  simulated counter. The caches used hashed descriptors, not queries/plain paths.
- Single debug-run end-to-end observations: 11835ms initial, 9883ms restart,
  9830ms changed-path restart (`target/build/folder-persistent-restart-gpu.log`).
  Model/runtime integrity checks and process/session startup remain in these
  times; do not claim instant startup or a benchmarked general speed multiplier.

Catalogue schema remains v13; no GUI/provider/search-mode change. Prior 245 UI
checks, GPU/provider/quality evidence and unchanged runtime installation remain
valid. The new compiled inputs invalidate the prior UI artifact for persistence;
one affected optimized build is required, without repeating unrelated suites.

## Text snippets, Enter/top-10 and optional CUDA follow-up · 2026-11-17

Latest native owner feedback accepts the functioning split view/neural mode and
requests the concrete UX refinements recorded in `DETAILS-VIEW.md` and
`FOLDER-NEURAL.md`. Existing named sessions were reused with disjoint ownership.

- UI **245/245** passed (`target/build/mini-text-gpu-pages-ui.log`): actual
  painted first-line/renamed Text titles, pinned/unpinned dot and tag color,
  Ctrl+arrow through the real split shell without selection/cursor/fullscreen
  changes, Enter-only neural input, appending the next 10 on last-row Down,
  settings checkbox click/save/restart and the unchanged UI scenarios.
- Initial UI run had 241 passed/2 fixture failures: the old cross-provider
  expectation still demanded a technical Text filename, and the new keyboard
  fixture sent input before the first render enabled text preview. Expectations
  and initialization were corrected, then the affected integrated UI gate passed.
- Domain semantic Page API **3/3**, settings **2/2**, isolated runtime resolver
  **1/1**, Python cache **2/2** passed. Page tests cover 10+10+5 without repeats,
  snapshot invalidation, explicit backend errors/cancel and GPU preference restart.
  These synthetic checks alone do not prove CUDA availability.
- All-targets Clippy `-D warnings` and formatting passed. Unchanged domain,
  model-quality and broad migration evidence were reused.
- Actual CPU-mode API probe after the new device switch: 512 synthetic paths,
  cold **8914ms**, warm **25ms**, one-path-change **25ms**, three intended targets
  ranked first (`target/build/folder-cpu-mode-final.log`). One run each, including
  runtime validation/startup and polling; not a controlled throughput benchmark.

The requested GPU component is installed separately from the emoji CPU runtime;
provider/kernel and end-to-end GPU evidence is recorded below once confirmed.
No native window was controlled and no personal tags/content were changed by tests.

### Confirmed CUDA component and API

Mira installed the isolated GPU component once; dependency validation passed.
RTX 5090/driver 616.86 was observed locally. The first real GPU startup exposed
a concrete worker defect: blanket `session.disable_cpu_ep_fallback=1` rejects
the model's legitimate shape nodes. Removed only that setting; CUDA selection,
provider verification and disabled whole-session fallback remain. Profiling
confirmed **349 CUDA nodes** (MatMul 84, FusedMatMul 12), and **2 CPU** shape nodes
(Gather/Unsqueeze). No CPU/emoji environment or model manifest was modified.

The corrected worker passed AST/cache checks and real CPU/GPU 64-name smoke.
CPU startup/first/cached: 981/104/3ms; GPU: 1264/184/3ms. Single observations;
this small workload did not show an acceleration.

Final **real CUDA API probe passed** through Settings(true) → BeginJob → resolver
→ resident worker: **8192 synthetic paths**, cold **8544ms**, warm **51ms**,
one-path-change **51ms**, intended targets rank 1 for three manual queries.
Evidence `target/build/folder-gpu-api-final.log`; no private paths printed.
The earlier CPU 8192 probe was 30925ms cold; these were not controlled paired
benchmarks and no general multiplier is claimed. No broad live-index recall
claim follows from the synthetic model/IPC check.

The embedded worker changed, invalidating its GPU startup evidence and prior UI
artifact for GPU use. Repeat only the affected model/worker checks and optimized
UI build; reuse 245 UI checks, domain Page/settings evidence, unchanged Clippy,
installed CUDA component and cached toolchain. No clean/full test loop or repeat
of the 1.8GB setup is required.

### Final CUDA primary artifact

Source local main `58c86ce7354487308a9f7af7f34c1e9a62ea49bb`. The minimal
worker-input rebuild completed in **1m23s**; pre/post compiled inputs matched.
Primary runtime now selects **`a0f03404416d`**, UI SHA-256:
`a0f03404416d5d8b20bfcaeeabe537bcc3c280391e4056a0c7fb85ff1800d3ff`.
The copied executable and helpers were hash-verified. Previous `c363daae5864`
remains immutable; fresh consistent v13 backup passed integrity check, private
receipt `runtime/rollback-cuda-final.json`. Optional CUDA runtime is installed
and verified independently in the existing profile. The checkbox remains opt-in;
tests enabled it only in synthetic Services, never by rewriting Lilia's setting.
No schema migration, forced process termination, GitHub push/tag/release.
Exit the old tray process and relaunch through Raycast before trying the GPU
checkbox. All requested Text/pin/keyboard and Enter/top-10/page UX is included.

### Primary UX artifact while optional GPU setup completes

Source local main `cd4d5bf23c93d604d96d731be69f130088c419e5`; one optimized UI
build **1m24s**, matched pre/post compiled inputs. Primary runtime now selects
**`c363daae5864`**, SHA-256:
`c363daae58642a81352dba366261d026e09655005da5c397a9451720fb628ad1`.
Previous `52d064dd97cb` retained; a fresh consistent v13 backup passed integrity
check, private receipt `runtime/rollback-mini-text-pages.json`. No migration,
shortcut replacement or forced restart. The owner exits the old tray process
and relaunches through Raycast for these new UX changes. No GitHub publication.

Optional CUDA wheels are a separate download/setup stage and do not justify
recompiling this unchanged UI artifact when installation finishes. The GPU
checkbox defaults off; CUDA availability is not inferred from this artifact.

## Mini stream and local Folder neural mode · later 2026-11-17 follow-up

Tag dragging was confirmed working by Lilia. The earlier third-view/neural
research checkpoint was followed by her instruction to implement both.

- API-first View gate: 6 checks passed across details, per-tab preferences,
  action presentation and density. All 10 tabs, restart, selection/cursor and
  old Gallery/Stream settings are covered. No schema change.
- Integrated UI/API gate: **239/239 UI**, **1/1** read-only Desktop/Folder
  provider check, **2/2** Details API and **2/2** initial semantic API passed
  (`target/build/details-semantic-integrated.log`). The optional real-model
  probe was intentionally ignored in that ordinary run.
- Initial compile errors were confined to the new test moving a field out of a
  Drop type. Initial UI failures were a missing new-action icon and unapplied
  egui texture deltas in two new fixtures; all were corrected. The provider
  fixture's initial assumption about upscaling a 2px image was corrected.
- Final all-targets Clippy `-D warnings` passed after collapsing two nested
  context-reset checks; formatting passed. No production warning suppressions.
- Synthetic semantic API checks explicitly cover missing/failed model, invalid
  replies, cancellation, exact priority, incomplete coverage and injecting an
  ordinary nonliteral candidate outside the broad index. The last candidate-pool
  change passed its targeted **2/2** API gate. Everything snapshot **7/7**;
  late image/text/table context checks **4/4**; Python incremental cache **2/2**.

### Real optional runtime evidence

The actual resident Python/ONNX worker was exercised through BeginJob/PollJob,
using the already installed verified runtime. All output omitted private paths.
No content was read, no model downloaded, no persistent private vector index
created. These are single-run wall-clock observations on this machine, not
controlled benchmarks, p95 or native visual acceptance.

Final synthetic source: 8192 fake paths; Russian travel, invoice and florist
queries. Cold end-to-end **30,925ms**, warm **50ms**, one-path-change **51ms**;
intended targets ranked first. Cache synthetic tests separately prove no encode
on reorder, one new encode on replacement and eviction of removed paths.

Final live Everything source: 8192 bounded paths, explicitly incomplete.
Observed end-to-end times **69,403ms** cold, **1,090ms** and **1,014ms** for
subsequent queries. `Katran` ranked first; `cat run` did **not** return the target
in this probe's top 20, despite improved candidate acquisition. This is a known
quality limitation, not a passed typo-recall test. The ordinary search's earlier
`cat run` success remains independent. Do not market the new mode as uniformly
better or full-disk semantic recall. Evidence: `folder-semantic-local-final.log`
and `folder-semantic-live-final.log` in ignored `target/build`.

The first real probe exposed expensive full re-embedding when the candidate
snapshot changed (61.765s and 44.292s, before the cache fix). The source changed
to incremental per-path caching, then only the affected model/IPC probes and
API/cache checks were rerun; the successful UI suite was not blindly repeated.

Installed ONNX Runtime **1.23.2** exposes CPU/Azure providers; the Folder worker
explicitly selects **CPUExecutionProvider**, four threads. No CUDA provider is
installed or selected. GPU acceleration was discussed, not implemented or timed.
Native acceptance of split layout and gold-field behavior belongs to Lilia.

The last picker-only layout adjustment passed its **5/5** focused UI checks and
UI-binary Clippy, reusing the unaffected integrated evidence. These include
configured Ctrl+Y inside the focused picker and physical backslash text/paste
handling. Public source includes the embedded Python worker in the build-input
fingerprint. No successful optimized compilation has yet been repeated for this
source; the final staging receipt follows.

### Primary installation

Source: local main `c5f592f9ddc22129fe585d2d80d1ed2ec6e03a6a`. One optimized UI
build completed in **1m20s** using the existing target/toolchain/features.
Pre/post compiled-input hashes matched, including the embedded Python worker.
Unchanged Everything helper and notices were reused and copy-hash verified.
Primary Raycast runtime now selects **`52d064dd97cb`**, UI SHA-256:
`52d064dd97cbb04e6f3133c8d5dc94f4627ae8a5f3ba9132fd43dbb95d91a28d`.
Previous `070eaf9b06b7` remains immutable; a fresh consistent v13 profile backup
passed integrity check. Private rollback receipt:
`runtime/rollback-details-neural.json`. No schema migration or shortcut
replacement occurred. The owner must fully exit the old process via the tray
and use normal Raycast launch to activate the new Display/settings entries.
The existing process was not closed or controlled. Nothing was pushed to GitHub.

## Complete tag reorder follow-up · 2026-11-17

The owner's latest feedback accepts the base tag/search usability, before the
new reorder, dense-list/detail and neural-search extensions. The base checkpoint
is separately saved in [Notion 3.8](https://app.notion.com/p/41b49fda329681d985ddeab05ac2ced1).

API-first reorder gate passed: snapshot 4, Desktop shelf 5, reorder API 1,
tag storage 5 and existing tag API 7 checks (`target/build/tag-reorder-api.log`).
After adding atomic checked-save, its API suite passed 2/2 and storage 5/5.
These use synthetic profiles, never reorder the owner's real tags as a test.
The targeted UI suite passed 22/22, including actual egui grip/drop frames,
full tag movement, editor following, conflict drafts and both stale editors.
Palette numeric actions also reject an obsolete list before dispatch. Within
the ordinary single-Service UI, validation and dispatch run sequentially;
the legacy numeric Filter/Assign API itself is not a cross-client identity lock.

Kiki independently reviewed the bounded domain changes and found no reproducible
integrity defect. Her shared conflict-message finding was fixed. Initial Clippy
failed only because a production helper included by a standalone storage fixture
was unused there; a test-module-only allow fixes that fixture without suppressing
production diagnostics. Final all-targets Clippy with `-D warnings` and formatting
passed. The known incremental hardlink warnings use copying fallback.

Final integrated check (`target/build/tag-reorder-integrated.log`): **228/228 UI**
tests, **2/2** reorder API, **2/2** scoped migration and **7/7** existing tag API
passed. The UI suite ran single-threaded to avoid competing native clipboard
fixtures. No native application window was opened or controlled for these tests.
Source checkpoint: `1a3fc6fb7abbf57287c84f688ec31d4be31d95f4`, local main only.

One optimized UI compilation completed in **1m19s**, using the existing
`target/build` cache, Rust/Cargo 1.95.0, Windows x64 and default features plus
`ui,clipboard-events`. Pre/post compiled-input hashes matched. Reused unchanged
Everything helper and dependency notices were copied and hash-verified.
Primary runtime pointer now selects **`070eaf9b06b7`**; UI SHA-256:
`070eaf9b06b759fe5dc36a6560f91f663218a158525fbcb6158d200b72407fd6`.
The previous `55356f72deb8` runtime remains immutable. A fresh consistent v13
SQLite backup passed integrity check; private rollback receipt:
`runtime/rollback-tag-reorder.json`. No migration, shortcut replacement, model
download or forced application restart occurred. Exit the old process via the
tray once and launch normally through Raycast to activate the new version.
Native visual acceptance of dragging remains open. No GitHub push/tag/release.

The third view is researched, not built. The offline 16-folder synthetic neural
probe is feasibility evidence only, not real-index quality or production latency;
see [its scope and numbers](FOLDER-SEMANTIC-RESEARCH.md). No model was downloaded
and no private-folder index or content was scanned for this research.

## Status

Independent CLI smoke was completed against the staged artifacts for
commit `12360d1ab4e4e9394b09d821e8f4935dcb1f6260` on `codex/katran-3.8`.
This is evidence for the commands and queries listed below. Native Windows UI
and product acceptance remain **PENDING OWNER**. Do not describe 3.8 as
accepted or publish it. After this isolated smoke, the owner separately
authorized main integration and primary local installation with existing data;
that later action is recorded in [TEAM.md](TEAM.md). It does not change the
scope of the smoke or imply native acceptance.

## Artifact identity

| Artifact | Staged path | SHA-256 |
| --- | --- | --- |
| CLI | `target/preview-3.8-build-12360d1/katran.exe` | `45c9d3cb934154d4acbcee3a039124dc94ceef30625c49cd11c60b4d257b2f4d` |
| UI executable | `target/preview-3.8-12360d1-portable/portable/runtime/versions/9266bfb24bfd/katran-ui.exe` | `9266bfb24bfdd7afe0f87dd03061119535ada4365dbd5446eed58688af144e99` |
| Portable package ZIP | Staged portable package | `0d13110e1889d50d09a1949e8a15f2b5dee5f741c5ed666277083b0d2f36e985` |

The CLI and UI hashes were checked against the supplied artifact identities.
The CLI was launched only with `serve-stdio` and a fresh temporary synthetic
profile. The UI executable was hash-checked but not launched. The full portable
package contract (schema 13, source inventory, three-file update set) was
reported passed by the integrator; the prior candidate package path was
rejected and is not the deliverable.

## Independent CLI smoke

The isolated profile had Downloads and capture-on-start disabled. No source
discovery, clipboard, capture, or UI commands were sent. The profile and its
synthetic folder were removed after the run.

- Saved different slot-1 definitions for Images and Text, read back each
  definition, then exited and reopened the exact CLI on the same profile.
  Both tab-specific styles/names survived restart and did not appear in the
  other tab's set.
- In Folder context, `RunAction(folder_favorites)` and
  `RunAction(folder_history)` returned their corresponding intents. In Images
  context, both were rejected as `unknown_action`.
- A synthetic temporary directory was added to favorites and history and read
  back from both lists.
- `ListKnownFolders` returned all six entries as resolved. Only volume and
  OneDrive-presence flags were retained below; no user paths were recorded and
  no destination was opened or modified.

| Known Folder | Result | Volume | Under OneDrive |
| --- | --- | --- | --- |
| Desktop | Resolved | C: | Yes |
| Downloads | Resolved | C: | No |
| Documents | Resolved | C: | Yes |
| Pictures | Resolved | C: | Yes |
| Videos | Resolved | D: | No |
| Music | Resolved | D: | No |

## Everything ranking snapshot

Two `SearchFolders` requests were sent to the exact staged CLI against the
existing local Everything index, limit 100. Times are single local request to
response observations, not benchmarks. Top results below show basenames and
categories only; absolute paths are intentionally omitted.

| Query | Returned | Repository rank | Incomplete | Time | Top-result summary |
| --- | ---: | ---: | --- | ---: | --- |
| `Katran` | 100 | **1** | Yes | 534.9 ms | First two basenames were `Katran` (repository first); next three were recovery-noise directories. |
| `cat run` | 13 | **1** | No | 476.2 ms | First two basenames were `Katran` (repository first); next results were Visual Studio runtime directories. |

For `Katran`, the result limit is full and the API marks acquisition incomplete;
the 100 returned items are not a complete index count. `cat run` returned 13
items with no stream reported truncated. Compared with the earlier debug
snapshot in [SEARCH.md](SEARCH.md), where `cat run` ranked the repository 12th,
the staged artifact ranked it first. This is one local index snapshot, not a
general recall guarantee.

## Checks not performed by this smoke

- Native tag editor placement, live rendering in the main interface, and
  keyboard behavior.
- Physical Ctrl+F / Ctrl+H, Ctrl+Shift+K, and contextual Ctrl+Shift+, input.
- Opening a real Known Folder destination or changing any user data.
- Manual Windows product acceptance by Lilia.

These remain pending. API results and successful packaging do not substitute
for the owner's native UI review.

## Native-feedback follow-up: local tag editor and Ctrl+Y

The owner confirmed that live styling works but clarified the palette editor
contract and requested a configurable Standard-folders shortcut. The following
targeted evidence covers the follow-up implementation, not native acceptance:

- API: `folder_standard_action` and the shared catalogue each passed 1/1.
  The new action is Folder-only, defaults to Ctrl+Y and persists a changed
  binding across restart. Shortcut policy tests passed 4/4, including preservation
  of an older user-defined Ctrl+Y assignment.
- Headless UI: the aggregate run passed 220 cases and initially failed three.
  Two new menu fixtures needed a real key release between presses and enough
  idle frames for egui sizing; their actual control/absence assertions were
  retained. The corrected tag-menu module passed 7/7, including the full shell
  route, rendered Name control, absence of preview/footer, captured-tab
  persistence, failed draft retention and Escape back-navigation.
- The Standard-folders UI test passed custom binding from Folder and an open
  picker, removal of the previous key after rebinding, and retention of Ctrl+Y
  by the focused query field. Settings accordion/live-save checks and the
  existing Actions shortcut recorder also passed in the aggregate run.
- The unrelated Windows synthetic cut/paste test reported access denied once
  in the parallel aggregate. Its isolated, single-thread rerun passed 1/1;
  the cause of the parallel sensitivity was not established. No source change
  or real clipboard/user-file operation was made to address that fixture.
- Final Clippy all-targets (`-D warnings`), formatting and whitespace checks
  passed. Schema v13, dependency lock, model/runtime, media and broader storage
  contracts are unchanged; their previous evidence was not rerun.

The primary local-main installation uses one newly built UI executable after
these checks. Its exact source checkpoint, hash and previous runtime are recorded
in the follow-up activation record in `TEAM.md` and the ignored runtime receipt.
Native owner acceptance of the new palette and physical keys remains pending.

The follow-up executable was built once from local-main source checkpoint
`ee164a3754d06c20d3a1104ddc664be1ce2e2649`, with pre/post build inputs matched.
It was hash-verified after copying and selected for the primary Raycast launch
as runtime `55356f72deb8`; SHA-256
`55356f72deb8c19ba4fc09d4760cdd5439da2b939eb4656786bd391ed389b89f`.
The existing profile remains in place, schema v13 unchanged, with a fresh
consistent backup and the preceding runtime retained. No GUI was launched by
the agents and no remote publication occurred.

# Katran 3.8 · FEATURE-5 / Мастерская + Iris

## Current: frame-step panel stability · 2026-11-18

Owner reported step-only panel/time flicker in runtime4acbfb44eca9; other new
keys worked. /root/fenna (gpt-6.1-terra/high) owns the sole UI source change:
paused-mode panel/transport stability and confirmed-position display. Frozen;
targeted UI15/15 passed. /root owns equivalent test-API compatibility correction,
lint/docs/local checkpoint/build/primary installation complete: source6fcc9f3,
runtime5c01f6c1060f, owner follow-up pending. Native/API/GPU unchanged;
reuse their passing gates. No team expansion or publication.

## Current: video seek/frame keys · 2026-11-18

Owner accepted ad534e0b6952, then requested Ctrl/Shift/Alt modified arrows.
Reused existing sessions, no team expansion:
- /root/mira · gpt-6.1-terra/high: native StepFrame/confirmation and CPU/GPU
  synthetic test. Final gate1/1 passed, frozen.
- /root/fenna · gpt-6.1-terra/high: relative-seek/hold UI, no queued repeats,
  serial gating/cancellation; frozen after API/native gate.
- /root · lead: policy/dispatcher contract, key ownership, boundary correction,
  tests/docs/lint/build/primary installation complete; source7fec1f5 installed as
  runtime4acbfb44eca9. Manual keyboard acceptance pending, no publication.

## Current: P, seek/layout polish and optional GPU · 2026-11-18

Existing sessions reused; no expansion:
- /root/fenna · gpt-6.1-terra/high: media_preview_ui.rs, dynamic overlay/seek
  confirmation/toggle method; frozen for final UI gate.
- /root/mira · gpt-6.1-terra/high: media_playback_mpv.rs and GPU WGL helper;
  native gates3/3 plus corrected orientation/GPU gate2/2 passed, frozen.
- /root/nemi · gpt-6.1-terra/high: settings.rs/settings_ui.rs/settings tests;
  default-off field and targeted fresh-settings save, frozen.
- /root · lead: input ownership/P, shared policy/service/Cargo/status integration,
  API tests, lint/docs/build/primary install complete: runtime ad534e0b6952 from
  source5498cd1. Manual follow-up pending; no publication.

## Current: libmpv and overlay controls · 2026-11-18

Owner confirmed the first candidate plays in Desktop but fails in Folder and
requested full-area video, translucent controls, idle hiding and Space autoplay.
The first candidate is not accepted. Reused sessions, same models/efforts:

| Session | Ownership | State / next step |
| --- | --- | --- |
| /root/mira · Terra/high | media_playback_mpv.rs | Frozen; muted Unicode MP4/MKV probe passed, 1/1 |
| /root/fenna · Terra/high | media_preview_ui.rs | Frozen; 8 player UI tests passed, including measured centering/width |
| /root/nemi · Terra/high | tests/media_playback_api.rs | Frozen; Folder Unicode source/context regression passed |
| /root · lead | lib/service/Cargo switch, setup, docs, integration/install | Lint/gates passed; runtime3322b2bcfddb installed; owner check pending |

libmpv is pinned to Afio's existing runtime. Current bridge renders CPU RGBA,
not Afio's OpenGL path; no performance/GPU claims. Setup reuses the verified
local archive. Native controls are observed through events; acceptance of audible
sound, owner videos and overlay appearance is still pending. No publication.

## Native video preview follow-up · 2026-11-18

Owner authorized local native video+audio in Space and Mini stream, with explicit
pause/seek/volume and stop on close/change. This is after v3.8.0 tag; no new push,
tag or public release. Existing sessions reused, scopes acknowledged:

| Session | Model/effort | Owned files and next step |
| --- | --- | --- |
| /root/mira | gpt-6.1-terra/high | media_playback_windows.rs; native backend/probe frozen for gate |
| /root/nemi | gpt-6.1-terra/high | media_playback/service, api/lib/jobs/file_types, policy/API tests; frozen for gate |
| /root/fenna | gpt-6.1-terra/high | media_preview_ui and rendering branches; frozen, 6 UI checks passed |
| /root | current lead | Wirev2, GUI lifecycle/shared fields, integration gates/docs/build/primary install |

Native MP4/MKV/lease probe passed 1/1; API/policy/wire passed 14/14 before GUI.
UI passed 272/272 including six new player tests. Clippy all-targets passed.
Lead is completing routing verification, local checkpoint/build and primary install.
No desktop automation or model/real-media tests.

## Public 3.8 release authorization · 2026-11-18

Lilia accepted the combined local UI follow-up and explicitly authorized pushing
main, tagging v3.8.0 and launching the existing public release automation. Earlier
no-push/no-release entries below are historical. Prepare a single release run;
checks/build/package/publish remain mandatory. Owner explicitly says **do not
wait** for this run: return its URL/ID after dispatch and stop. Do not describe
queued automation as a completed published release. AI setup on another PC is
covered by docs/ai-setup.md; fresh-machine acceptance remains a known limitation.

## Native acceptance · 2026-11-18

Owner restarted and answered **«Всё работает»** for the combined rename/pin/tag
image-retention and comma→Slash Tags sorting check. Installed runtime remains
`efc8c786e60f`, source `5135443`. The follow-up native check is accepted within
those reported scenarios. No product rebuild is required for the subsequent
build-storage cleanup/tooling policy.

## Installed combined follow-up · 2026-11-18

Source `5135443`, one optimized UI build **82.31s**, Windows x64 / Rust 1.95.0 /
locked offline / existing `target/build` / ui + clipboard-events. Compiled
source/resource fingerprints matched before/after; copied UI/helpers verified.
Primary runtime **`efc8c786e60f`**, SHA256
`efc8c786e60fbba74a6590c409a49749bb5f31fddd75067c5ed702211d99dca0`.
Atomic pointer replaced `cb740a2f72e7`; previous runtime and fresh consistent v13
backup retained in private `runtime/rollback-tag-sort.json`. Schema unchanged,
no AI/model/runtime setup changes, no GitHub push/tag/public release.
Running app untouched. Owner asked to exit through tray, launch via Raycast,
check rename/pin/tag images and comma→Slash tag groups. Native acceptance pending.
For rollback after saving Tags mode, select a supported sort first or restore
compatible preferences; the old runtime cannot parse the new settings enum.

## Active follow-up · 2026-11-18

Owner found remaining image flashes on rename, pinning and tag assignment, and
requested parallel tag-order sorting. Existing Nemi `/root/nemi`
(gpt-6.1-terra/high) delivered the general cache/metadata bridge; files frozen.
Existing Kiki `/root/kiki` (Junior/Luna high, reused session, no replacement)
acknowledged and delivered domain Tags sorting/tests; files frozen. Polly owns
shared API/settings/actions integration, keyboard presentation, validation,
docs and primary staging. No team expansion. First domain Tags gate 4/4 passed.
Combined API regression 20/20 passed; UI 265/266 plus the unrelated timing
fixture isolated retry 1/1; all-targets Clippy/format/diff checks passed.
Next: one primary artifact and owner visual checks.

Initial owner instruction: 2026-11-17, preparation and implementation without publication. The later workflow correction authorizes local main integration and primary installation with existing data; see the activation record below. No public release or push. Product contract: [PRODUCT.md](PRODUCT.md).

Lead chat: `01a1ea4b-8cbb-7bf2-9d11-2efe604474d7`, runtime `/root` (current session; no separate Lead spawned). Baseline `c7c0b8e`, Katran 3.7.1, schema v12. Existing untracked user files are outside this change.

## Runtime roster

Current inline-editor source `0c507dc`, one optimized UI build **22.74s** in the
existing cache, matched compiled-source/resource hashes. Primary runtime
`cb740a2f72e7`, SHA256
`cb740a2f72e7bf7f95ed26283c35f0f84fcc5075c33b26ecd02e3358b42c117c`.
Copied UI/helpers verified; pointer changed atomically from `981c10a95b8a`.
Previous runtime and consistent schema-v13 backup retained in private
`runtime/rollback-tag-modal.json`. Running app untouched; owner asked to exit
through tray/Raycast relaunch and check sliders, dismissal and pinned Ctrl+Enter.
UI 262/262, existing pinned API 3/3, Clippy/format passed. Native acceptance is
pending; no GitHub push/tag/public release.

Inline-editor follow-up (2026-11-18): same Fenna `/root/fenna`
(gpt-6.1-terra/high) owns tag_menu_ui lifecycle/checked-save wiring; same Nemi
`/root/nemi` (gpt-6.1-terra/high) owns images/api_client metadata cache bridge;
same Iris `/root/iris` (gpt-6.1-sol/low) delivered readonly pinned-form UX contract.
Polly `/root` owns pinned_ui, shared lifecycle/hide/tab integration, tests/docs
and primary staging. All scopes acknowledged; files are frozen. UI 262/262 and
Clippy passed. No roster expansion, native automation or parallel redesign.
Next: one verified UI build, primary installation and owner checks.

Current primary source `82619cb`, runtime `981c10a95b8a`; one optimized UI build
1m26s with matched source/resource and copied-file hashes. Previous
`fb5e69b83a48` plus consistent v13 backup retained in private
`runtime/rollback-paste-sequence.json`. Owner restarted herself and confirmed the
same file now inserts in Notion browser through Enter. New-session logs confirm
verified publication/delivery 687→687 and successful input; a four-file batch
also shows verified 691→691/input success. The reported failure is closed;
native acceptance is scoped to the owner's tested editor/file. Documentation and
Notion updated, no new build for these evidence-only notes and no GitHub push.

Settlement implementation frozen: same Mira `/root/mira` gpt-6.1-terra/high
delivered post-teardown proof and 13/13 synthetic tests. Same Azuros `/root/azuros`
gpt-6.1-sol/xhigh reviewed once without blocking findings. Polly's dispatcher
zero-sequence copy gate passed 2/2 (expanded vectors), all-targets Clippy passed.
No native Clipboard test or desktop automation. Next: one UI artifact from the
existing cache, primary staging, owner retry and stage-log comparison.

Notion-browser incident follow-up: owner confirmed manual Ctrl+C→Ctrl+V works
for the same file/editor, while Enter fails. Polly read existing bounded stage
logs and identified five file publications cancelled as `clipboard_changed`
before SendInput. Mira `/root/mira` (existing gpt-6.1-terra/high) owns only
`copy_out.rs` publication settlement and synthetic tests; Polly owns evidence,
API integration/gates/docs and primary staging. No new agents, native clipboard
inspection or desktop manipulation. See `PASTE-SEQUENCE.md` for the observed
boundary and the approved strict verification contract.

Current Enter/keyboard source `fed4815`, one optimized build 1m44s (new Windows
Accessibility feature compiled in the existing cache). Compiled inputs matched
before/after. Primary runtime `fb5e69b83a48`, SHA256
`fb5e69b83a48571f83cd03dfad796c6d176c4b5d298a4ae6dddc7838e1623466`.
Copied UI/helpers verified; primary pointer changed atomically from
`4a4340fd4d26`. Previous runtime and integrity-checked schema-v13 backup retained
in private `runtime/rollback-enter.json`. Running app untouched. All affected
gates/review complete; next: owner tray exit/Raycast launch, record Text shortcut
again and native B→Katran/Notion/chat acceptance. No GitHub publication.

Enter implementation handoff: Mira native observer/activation, Nemi lease API
and Fenna file publication are frozen; first API gates passed. Owner then reported
Ctrl+Shift+Slash/Shift recording: Polly owns parser/VK aliases and persistence
tests, Fenna owns the existing physical recorder (7/7 tests). Azuros reused the
same `/root/azuros` gpt-6.1-sol/xhigh for bounded read-only review and found no
remaining blocking code issue after fresh-summon consume correction. His exact
combined API regression passed. No roster expansion or native UI/Clipboard test.
Polly integrates diagnostics, remaining gates, docs and one primary UI build.

Enter implementation is now explicitly authorized by Lilia (2026-11-17).
Same existing sessions: Nemi `/root/nemi` (gpt-6.1-terra/high) owns overlay domain,
immutable paste lease and synthetic API tests; Mira `/root/mira` (same model/effort)
owns native foreground observer and activation/input guards; Fenna `/root/fenna`
(same model/effort) owns file Clipboard HWND lifetime and publication facts.
All three acknowledged scope/result and agreed native/domain interfaces before
parallel edits. Polly owns shared Service/feature/UI integration, docs and build.
No roster expansion, desktop automation or real personal Clipboard testing.
Native receipt interoperability remains a measured/manual boundary, not inferred
from SendInput. Garden refresh failed; cached profiles were used as reported.

Current installed source `caa99c6` adds the AI settings/visible installers. One
optimized UI build completed in 1m20s with identical pre/post compiled-resource
hashes. Primary runtime is `4a4340fd4d26`, executable SHA256
`4a4340fd4d267dc9887739a737740f20bcb78f52e21bf5fa9222a772a2bb2026`.
Copied helpers/notices and installed UI hashes matched; pointer changed atomically.
Prior hotkey runtime `8c447197c96e` and a new consistent schema-v13 backup remain
available through private `runtime/rollback-ai-components.json`. Running Katran
was left alone. Next owner action: tray exit, normal Raycast launch, Settings →
AI and tab-hotkey native checks. Fresh-machine bootstrap remains untested;
Enter audit is complete but its fixes remain separate. No GitHub push/release.

AI-component follow-up: same Mira `/root/mira` (Terra/high) owns setup/verify
scripts and package whitelist, Nemi `/root/nemi` (Terra/high) owns independent
CPU resolver/Folder adapter, Fenna `/root/fenna` (Terra/high) owns Settings AI.
Polly owns dispatcher/visible launcher/bundle, integration/gates/docs/install.
All three acknowledged their scopes and delivered frozen files; no new agents.
Azuros `/root/azuros` (actual gpt-6.1-sol/xhigh) completed `PASTE-AUDIT.md`.
Next paste patch is target acquisition plus immutable pending-paste lease and
bounded activation; a separate clipboard-owner fix is justified. No Enter fix
or recipient acceptance is claimed. Clean-machine AI setup remains manual QA.

Global hotkeys source `ada44aa` was built once in 1m24s and staged as primary
runtime `8c447197c96e` (SHA256 `8c447197c96e164a7a65cc5c26b743ab01f2818ea549a41d5a4b5ae741421546`).
Backup receipt `runtime/rollback-tab-hotkeys.json`; previous `17ab92129b84`
retained. The running app was untouched. The subsequent AI change requires one
new source build; unchanged hotkey checks/artifact are retained as evidence.

Global-tab follow-up: Nemi `/root/nemi` (gpt-6.1-terra/high) finished domain,
persistence and API tests; Fenna `/root/fenna` (gpt-6.1-terra/high) finished
Settings recorder/status UI. Mira `/root/mira` (gpt-6.1-terra/high) delivered
read-only `PASTE-RECON.md`. Polly `/root` owns native integration, shared files,
checks, docs and primary installation. Iris `/root/iris` (gpt-6.1-sol/low) and
Kiki `/root/kiki` (requested Luna/high) are retained, not replaced or duplicated.

Owner explicitly approved one targeted expansion for Enter audit:
Azuros Claudapi `/root/azuros`, actual gpt-6.1-sol/xhigh, spawned after hotkey
API/UI gates passed. Scope acknowledged: read-only code audit plus
`PASTE-AUDIT.md`; no desktop, clipboard, build or code mutations. No blockers;
recipient acceptance requires owner-operated native checks. Garden refresh was
unavailable; the cached profile was used. Next: ranked causes and bounded patch
plan. This is not an Enter-fix acceptance or permission to publish.

Persistent-index work complete: source `4c882ff`, primary runtime `17ab92129b84`,
one 1m23s optimized build with verified compiled inputs and installed hash.
Nemi's SQLite/cache test handoff and Polly's three-process CUDA proof are closed.
New consistent v13 backup and prior runtime retained. Next: owner tray exit and
Raycast relaunch; first new-version neural query creates the on-disk cache once.
No UI/model/package changes and no GitHub publication.

Persistent-index follow-up: same Nemi session owns worker SQLite/fingerprint and
Python cache tests; Polly owns profile cache path, bounded API cache diagnostics,
isolated real-process restart probe, integration/docs/staging. No new agents.
Synthetic cache 5/5 and existing semantic API 3/3 passed; real CUDA probe across
three fresh Services/workers proved 8192 encoded initially, then 8192 reused/0
encoded after restart, then 8191 reused/1 encoded after a changed descriptor.
All-targets Clippy passed. GUI/schema/model/GPU environment are unchanged; reuse
their evidence and rebuild only the affected UI artifact after source freeze.

Final state: all follow-up implementation/setup work complete. Source `58c86ce`,
primary runtime `a0f03404416d`, corrected embedded CUDA worker and profile-isolated
GPU component verified through an 8192-path synthetic API probe. Same named
sessions are complete and reusable; no new workers. Prior UI/API/Clippy evidence
retained for unchanged inputs, minimal worker rebuild 1m23s. Lilia's next step:
tray exit/Raycast relaunch, manual Text/pin/page feedback and optional GPU checkbox
in General → Folders and storage. Backups and old runtimes retained; no GitHub push.

Current primary UX checkpoint: `cd4d5bf`, runtime `c363daae5864`, matched hashes,
one optimized build 1m24s and verified profile backup. Text/pin/keyboard and
Enter/top-10/page UX are staged for Lilia; same previous runtime retained.
Mira's optional GPU download/setup remains active, with real CUDA/API proof
pending. Polly will reuse this artifact for a runtime-only setup completion.

Latest native-feedback follow-up: Lilia confirmed Mini stream and neural search
work and requested Text titles/pin markers/Ctrl+arrow, Enter-only neural search,
10-item pages and optional GPU. Fenna owns row presentation and Settings checkbox;
Nemi owns settings/Page API/snapshot/GPU worker switching; Mira owns isolated GPU
setup/resolver and real provider verification; Polly owns input/pagination UI,
shared metadata titles, integration/testing/docs/staging. Same sessions reused,
no additions or substitutions. UI 245/245 and all-targets Clippy have passed;
GPU setup/provider evidence and final staging are tracked in `VERIFICATION.md`.

Final follow-up staging complete: source local main `c5f592f`, primary runtime
`52d064dd97cb`, one optimized build 1m20s with matched compiled inputs including
the embedded worker. Fenna/Mira/Nemi handoffs and targeted checks are closed;
Polly verified actual CPU runtime/model and bounded live-index behavior, with
known quality/initial-index latency limitations in `VERIFICATION.md`. No new
agent was created, no GPU runtime was installed. Current next step: Lilia exits
the old tray process and relaunches through Raycast for native Mini stream and
Folder search feedback. Previous binary/settings-aware backup are retained.
No GitHub push, tag or public release. Earlier records below are historical.

Latest owner follow-up: tag dragging is now confirmed working natively by Lilia.
She requested actual implementation of the researched third view, plus Ctrl+Y
inside the open Folder picker and physical-backslash neural search with a gold
field. Same sessions reused: Nemi owns View/Actions/persistence and semantic
command/jobs/resident runtime; Mira owns provider-aware preview workers and
Everything snapshot; Fenna owns Mini stream UI/settings/action icon; Polly owns
asynchronous details metadata, Folder picker UX, source fingerprint coverage,
integration/checks/docs/staging. No new agents or model substitutions.
Domain View gate passed before UI, and neural API gate before actual-model probe.
Details UI is integrated; current validation/staging evidence follows in
`VERIFICATION.md`. Historical completion records below describe earlier stages.

Reorder source frozen at local main `1a3fc6f`. Nemi's domain and Fenna's UI
are complete; Kiki's bounded independent review is closed. Final integrated
checks: UI 228/228, reorder API 2/2, scoped migration 2/2, existing tag API 7/7;
all-targets Clippy and formatting passed. Polly owns the single optimized build,
hash-bound primary staging and final documentation receipt. Native acceptance
of drag reordering remains the owner's next step.

Staging complete: one 1m19s optimized build, matched compiled-input hashes,
primary runtime `070eaf9b06b7` (full SHA-256 in `VERIFICATION.md`). Previous
`55356f72deb8` retained; new consistent schema-v13 backup verified. Private
receipt `runtime/rollback-tag-reorder.json`. Polly's integration/staging is
complete; next step is the owner's normal tray-exit/Raycast relaunch and drag
feedback. No running application was closed and no GitHub changes were pushed.

Current follow-up, 2026-11-17: the base is accepted for the owner's tag/search
workflow and recorded in [Notion 3.8](https://app.notion.com/p/41b49fda329681d985ddeab05ac2ced1).
Fenna owns Settings drag UI and revision-checked saves in both tag editors;
Nemi owns reorder/checked-save domain, public commands/dispatcher and synthetic
tests (temporary explicit shared-file ownership transferred from Polly for this
bounded change). Mira delivered `DETAILS-VIEW-RESEARCH.md`; Fenna delivered
`DETAILS-VIEW-UX.md`. Kiki independently reviews the reorder boundary; Polly
integrates, maintains this map, records the offline semantic feasibility probe
and stages one verified artifact. No new agents were created. The third view
and Folder AI remain research only. Historical status rows below describe the
completed base, not the current follow-up workload.

These are real canonical runtime session identifiers returned by spawn_agent. Reuse them with followup_task; do not spawn replacements. Completed sessions retain their handoff, not permanent persona memory.

| Persona | Runtime ID | Requested model / effort | Ownership | State / next step |
| --- | --- | --- | --- | --- |
| Polly / Appolion | `/root` | current lead session | `api.rs`, `lib.rs`, version/docs, integration and verification | Integrate agreed domain contracts; maintain this map |
| Fenna Ferrule | `/root/fenna` | `gpt-6.1-terra` / high | tag settings/material/menu/shelf UI, workspace picker, contextual actions/shortcuts/icons and UI tests | Implementation and focused fixes complete; UI 220 passed; source frozen |
| Mira Socket | `/root/mira` | `gpt-6.1-terra` / high | `everything.rs`, `known_folders.rs`, adapter tests | Implementation complete; real index syntax verified; final artifact search independently checked by Kiki |
| Nemi Index | `/root/nemi` | `gpt-6.1-terra` / high | tag slots/service, numbered schema, scoped browse/pin cleanup, `folder_search_rank.rs`, tag/migration tests | Implementation and scoped migration checks complete; word-boundary ranking fix verified |
| Iris Vale | `/root/iris` | `gpt-6.1-sol` / low (= requested light) | UX contract only; no parallel UI edits | Acknowledged and delivered handoff to Fenna; reuse for targeted review |
| Kiki Click | `/root/kiki` | `gpt-6-luna` / high | independent acceptance scenarios and integrated-result verification | Exact release hashes and isolated two-process CLI smoke passed; native owner acceptance pending |

All five requested models successfully launched. The runtime allows root plus three simultaneously working agents; initial Iris/Kiki starts returned `agent thread limit reached`, then succeeded after reconnaissance tasks completed. Keep the five existing sessions and schedule work within this concurrency bound. No persona/model substitution occurred. Garden CLI used its local snapshot after network failure; all five full cards were subsequently fetched through connected Notion and stored in `legion/`.

Project session inventory: earlier chats `Katran 3.7` (`01a1db19-4256-7f33-aa64-ecb61c5f5f57`) and `Katran 3.4` (`01a1ce3d-72a0-7e30-9209-3c245a393a3d`) are not loaded. Runtime inventory at start exposed only `/root`; no resumable FEATURE-5 worker sessions existed in this team tree.

## Agreed boundaries

- Mira acquires/deduplicates candidates; Nemi owns pure `rank_folders(query, &mut [EverythingFolder])`; final display limit follows ranking. Bounded acquisition must expose incompleteness. No project-name hardcode, cloud requests, or AI requirement.
- Tags: both definitions and assignments are scoped by tab. Migration preserves v12 definitions, styling and links in every tag-capable context; subsequent changes are independent. Nemi owns schema and all SQL changes; Polly owns public enum/dispatcher integration. Synthetic old-profile fixture only.
- Fenna owns UI after domain behavior is verified; Iris supplies UX contract without separate redesign. Kiki independently checks the integrated result. Native Windows acceptance belongs to Lilia; no desktop automation authorized.
- Latest owner clarification: `Ctrl+Shift+,` edits the highlighted concrete tag inside the `/` palette, using compact shared controls with no preview or settings footer button; preserve the Actions recorder. Folder AI remains deferred. The owner now explicitly approved rebindable `Ctrl+Y` for Standard folders in Folder.

## Evidence / open boundaries

- Domain checks: library 150 passed / 1 ignored; catalogue snapshots 4, Desktop shelf 5, hardening 10, Known Folders API 1 passed. Tag storage 3 and scoped migration 2 passed; tag API 7 passed after correcting a synthetic fixture to assign in its intended tab. The migration suite includes rollback/retry after an injected synthetic schema collision. Folder UI and final integrated build checks are still pending.
- Everything initially failed with Windows error 623 in restricted execution. Read-only elevated execution succeeded. Live verification caught a real installed-version syntax mismatch (`nopath:` works, `no-path:` did not); fixed before the candidate build. See `SEARCH.md` for original, intermediate and final exact-artifact API snapshots. The latest word-boundary fix was independently verified in that final snapshot.
- Installed runtime and personal data remain unchanged; deliver an isolated runnable artifact with source identity for manual verification later.
- Local branch: `codex/katran-3.8`. First compile exposed a ranker closure type mismatch; fixed before test execution. Later test failures were fixture assumptions about disabled Text and formerly global assignments; no failed check is counted as passed.
- Final integration coverage: all 116 integration targets accounted for, 379 passed and 6 intentionally ignored. Library 151 passed / 1 ignored before the final ranking refinement; the affected ranker 2/2 and Everything adapter 6/6 passed afterwards. UI 220/220 plus 15 focused tag UI regressions passed. Launcher 4/4 passed. Clippy all-targets with `-D warnings` passed; the synthetic package contract passed. Dependency notices for 302 packages were generated before release compilation. Native visual acceptance remains open.

## Candidate artifact

- Source commit: `12360d1ab4e4e9394b09d821e8f4935dcb1f6260`.
- One optimized build completed in 1m37s using the existing `target/build`
  cache, Rust/Cargo 1.95.0, Windows x64, default features plus
  `ui,clipboard-events`, without the optional bundled Apple resource.
- Input hashes were captured before compilation and matched afterwards.
  The producer receipt and executable hashes are in
  `target/preview-3.8-build-12360d1/receipt.json`.
- Complete Portable staging directory:
  `target/preview-3.8-12360d1-portable/portable/`.
  Launch its `Katran.exe`; the portable flag gives it a separate local profile.
  The installed launcher remains on `d7eae2862b31`.
- UI SHA-256: `9266bfb24bfdd7afe0f87dd03061119535ada4365dbd5446eed58688af144e99`.
- CLI SHA-256: `45c9d3cb934154d4acbcee3a039124dc94ceef30625c49cd11c60b4d257b2f4d`.
- Portable ZIP SHA-256:
  `0d13110e1889d50d09a1949e8a15f2b5dee5f741c5ed666277083b0d2f36e985`.
- Actual package contract passed: helper/notices, schema v13 and the existing
  three-file updater format. The optional model and FFmpeg runtime are not in
  this local preview package. No installer was run, no tag/release created,
  and nothing was pushed.
- The earlier `target/preview-3.8-12360d1/` directory is an incomplete rejected
  packaging attempt, not the candidate. Packaging expected an empty string
  rather than null for the absent optional Apple digest; correcting that
  receipt metadata and packaging to a fresh directory reused the same
  already verified binaries. No recompilation occurred.
- Kiki independently confirmed the exact UI/CLI hashes, scoped tag style/name
  persistence across two CLI processes, Folder action contexts, synthetic
  favorites/history and six read-only OS Known Folder resolutions. Both real
  `Katran` and `cat run` searches ranked this repository first. Details are
  recorded separately in `VERIFICATION.md`. Native UI acceptance belongs
  to Lilia and is not implied by any API or headless result.
- Owner reminder: native acceptance does not itself authorize a GitHub push,
  tag or public release. Primary local installation was explicitly authorized
  by the later correction recorded below.

## Primary launch staging · owner correction 2026-11-17

The owner rejected the empty-profile preview as the normal acceptance route and
instructed Polly to integrate local main and install to the primary Raycast app
with existing data. This supersedes the initial no-install restriction.

- Verified the actual Start Menu/Raycast link points to `runtime/Katran.exe`
  with the existing repository `katran-data` profile.
- Created a consistent SQLite backup of schema v12 with SQLite's online backup
  API and verified its integrity. Exact private paths and pointer backup are
  recorded locally in ignored `runtime/rollback-3.8.json`.
- Copied the already verified runtime and notices to
  `runtime/versions/9266bfb24bfd`, verified every copied file hash, and atomically
  changed the primary launch pointer from `d7eae2862b31` to `9266bfb24bfd`.
  No compilation or shortcut replacement was needed. The materialized Apple
  font remains in the existing profile; optional local runtimes are unchanged.
- Previous runtime and pointer backup remain available. Full rollback after
  migration requires stopping Katran and restoring the pre-v13 database, not
  merely changing the executable pointer.
- The existing 3.7.1 process was left running. The owner must fully exit it once
  through the tray and use normal Raycast launch for 3.8 activation. Migration
  is intentionally deferred to that startup to avoid concurrent old/new writers.
- Native owner acceptance remains pending. Nothing was pushed to GitHub.

## Native-feedback follow-up · 2026-11-17

- Lilia confirmed live tag styling works, but rejected the palette shortcut
  opening Settings and the added footer button. Fenna (`/root/fenna`, same
  Terra/high session) owns the compact local editor and shared controls.
- Lilia requested a small worker for the separately approved Ctrl+Y binding.
  Kiki (`/root/kiki`, existing Luna/high session) owns the bounded command,
  shortcut and Folder-picker wiring. This is a tiny implementation assignment;
  it is not independent acceptance of her own change. No roster expansion.
- Polly owns the one generic shortcut-loop exclusion in `ui.rs`, integration,
  targeted checks, a single optimized build and primary runtime activation.
  Fenna and Kiki have disjoint implementation files.
- API/persistence and existing schema v13 are reused. No migration or model
  change is required; unchanged broader evidence remains valid. Source changes
  require the affected domain/UI checks and one rebuilt UI executable. The
  owner performs native acceptance using the primary profile on local main.
- Follow-up gate passed: new action API 1/1, catalogue 1/1, shortcut policy 4/4,
  corrected tag-menu module 7/7, full-shell rendering and configurable Folder
  shortcut cases passed; final Clippy all-targets and formatting passed. See
  `VERIFICATION.md` for the split aggregate/targeted results and one unrelated
  Windows fixture's parallel access-denied observation.
- Follow-up source checkpoint on local main:
  `ee164a3754d06c20d3a1104ddc664be1ce2e2649`. One optimized UI build completed
  in 1m19s with matched pre/post compiled-input hashes; no second compilation.
- Primary Raycast launch now selects `55356f72deb8`; UI SHA-256
  `55356f72deb8c19ba4fc09d4760cdd5439da2b939eb4656786bd391ed389b89f`.
  The stable launcher/profile path is unchanged. Runtime `9266bfb24bfd` remains
  available for rollback. A fresh consistent schema-v13 database backup passed
  integrity check; the private receipt is
  `runtime/rollback-local-tag-editor.json`. No migration was required.
- The old running process was not closed or manipulated. The owner should fully
  exit it once through the tray, then use normal Raycast launch. Native visual
  acceptance of this follow-up is still pending; nothing was pushed.

Build-storage incident closed locally: target **387.59 → 31.89 GiB**, removed
355.70 GiB, zero failed entries; active/prior runtime and database backup verified
unchanged. Polly completed guarded maintenance/tooling directly, without team
expansion. Automatic post-install budget hook and manual batch instructions are
in docs/devops.md. No Rust rebuild/push/tag/release. Native Tags/cache acceptance
was supplied by Lilia («Всё работает»).

## Video capture follow-up · 2026-11-18

Polly reused /root/mira, /root/nemi and /root/fenna (gpt-6.1-terra/high).
Mira owns media_playback_mpv.rs native raw capture and seek settlement; Nemi owns
media_capture.rs, media_service.rs capture methods, jobs_service.rs and API tests;
Polly integrates shared command/action/wire declarations and pause semantics.
After API/native gates, Fenna owns ui.rs, api_client.rs, palette.rs and icons.
Each area is frozen before shared integration. No new masks or public publication.
Native CPU/GPU1/1 and API21/21 passed before frontend implementation; UI gate and
primary installation evidence are maintained in VERIFICATION.md.

Capture integration complete: sourcefb3fdec; primary runtimed615d492be2e.
Automated API/native/UI/Clippy gates recorded in VERIFICATION.md, including
focused fixes after aggregate gate. Polly owns native owner feedback and any
follow-up; all three worker areas are frozen. Owner check pending; no publication.

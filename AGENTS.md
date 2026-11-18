# Katran development rules

## Katran 4.0 · Sequential run · 2026-11-18

Follow the Notion 4.0 run order and the [actual team map](docs/patch-4.0/TEAM.md).
Stage 1 extracts one selected track with the existing FFmpeg runtime through
ExtractVideoAudio. It is independent of the video player session. Keep source
revision checks, separate staged output and the cancellation fence before
publication. See [the audio contract](docs/patch-4.0/AUDIO-EXTRACTION.md).
The first standalone AI feature is static image upscale, including saved frames.
Before it, limit Settings → AI work to installing and discovering the selected
model/runtime while preserving Emoji and Folder compatibility. Identify the
exact ComfyUI workflow, node and weights before choosing the model. Translation
follows upscale, with BULL and QuickLook last. This run is local.

AI operations belong in Ctrl+K → AI Commands. Give each model and fixed
operation its own independently rebindable command that starts directly.
For ×2 and ×4, multiply both output dimensions by exactly that factor.

The Translate shelf accepts only managed image results with internal
translation membership. Keep generic ordered collections in the independent
`baloh-collection` crate. Russian/English are the primary translation
languages; OCR should also cover Japanese, Korean and Chinese. Keep .bull
serialization and QuickLook after the one-page and batch translation gates.
The local one-page command recognizes Japanese and outputs English with
Qwen3 4B Instruct on the GPU. Reject incomplete output and partial GPU placement.
It publishes a separate PNG and Translate membership together. Invoke only
detected local components, without implicit model downloads.

## Ready text preview retention · 2026-11-18

Owner authorized closing the residual Links blink after accepting the Mini stream
geometry fix. Presentation-only browse-generation changes must advance async
request epochs and reject obsolete replies, but may retain successful ready
text/parsed document/view mode for the same validated content identity in the
same tab/folder/catalogue namespace. Never reuse by row ID alone: revision and
source identity (path/kind/source) matter. Clear on item/source/revision/namespace
change, errors, stop or disable. Do not remove late-response guards to hide a
blank preview. Document this distinction for the next team; no schema/player/
public-release changes are authorized by this UI follow-up.

## Mini stream preview slot stability · 2026-11-18

Owner authorized fixing the investigated metadata jump. details_view_ui reserves
an exact preview rectangle before rendering a clipped child; do not rely on
allocate_ui_with_layout(desired_size) alone to reserve a minimum height. Async
provider content must not move metadata when it arrives. Preserve the split
ratio, bounded preview clipping and existing scrolling/input/provider behavior.
See docs/patch-3.8/MINI-STREAM-LAYOUT-INVESTIGATION.md for measured pre-fix cause
and verification. UI-only: domain/storage/player behavior is unchanged; no schema
or public publication is authorized.

## Local video actions and frame capture · 2026-11-18

Owner accepted the Alt-step UI stability fix in runtime5c01f6c1060f. The next
local follow-up adds contextual Ctrl+K Video actions: rebindable video_pause
(default P) and capture_video_frame (no default). Do not hardcode P outside the
shortcut catalogue or add seek commands to this section. CaptureMediaFrame and
ToggleMediaPause accept an optional exact session token. Capture requires a ready
Paused/Ended session, reserves the native frame and uses screenshot-raw video rgba
at source resolution. Do not save the scaled UI texture or reopen/seek a decoder.
The background job validates session/context/source/timeline before saving a
ManagedImage through normal storage, even if Images is hidden, then uses normal
copy_out image publication. Clipboard failure/change preserves the saved image
and returns explicit partial success. Existing playback/cursor/tab stay intact.
Read docs/video-preview.md for bounds and verification. No schema change or
public publication is authorized by this local work.

## Local native video preview · 2026-11-18

Post-v3.8.0 local follow-up: Space and Mini stream use one explicit video/audio
session via `media_playback`, `media_service` and `media_playback_mpv`. The first
Media Foundation candidate failed owner Folder/Unicode checks and is replaced.
GUI opens through `BeginJob(OpenMedia)`; source preparation and guarded
commit belong to the dispatcher. Switching object/surface, closing, hiding or
Settings closes playback; metadata-only tag/pin updates preserve it. Space now
autoplays once per target; Mini stream waits for Play. Controls overlay the canvas
and hide during idle playback. libmpv is separately pinned/verified; the current
default bridge uses bounded CPU RGBA frames, not Afio's GPU rendering path.
Owner confirmed Folder/Desktop playback in runtime3322b2bcfddb. The follow-up adds
P pause/resume, confirmed-seek UI retention and large-control layout corrections.
Optional Settings.video_gpu_acceleration defaults off; WGL/libmpv OpenGL rendering
and observed hwdec-current are reported separately. Initialization failure falls
back to CPU with a reason; runtime driver failure stops with an explicit error.
Bounded RGBA readback remains: do not claim zero-copy or benchmarked acceleration.
Owner accepted that combined runtime. Video keys now add Ctrl/Shift arrows for
1s/10s seeks and Alt arrows for native paused frame steps. API StepFrame and
Snapshot.step_pending guard one in-flight step; hold repeat waits for a confirmed
new frame and ends on release/context/input-owner change. Do not promise monitor-
Hz stepping or emulate frames by guessed FPS. See the video contract/evidence.
Step UI must mask transient native Playing/Paused state: retain panel/label
geometry and use confirmed frame positions for timeline/time until completion.
Do not reintroduce ordinary idle-autohide while a frame step/hold is active.
Read [the contract](docs/video-preview.md) before changing playback or preview
routing. No schema change or new GitHub publication is authorized for this work.

## Public 3.8 release authorization · 2026-11-18

Lilia accepted the combined local UI follow-up and explicitly authorized pushing
main, tagging v3.8.0 and launching the existing public release automation. Earlier
no-push/no-release entries below are historical. Prepare a single release run;
checks/build/package/publish remain mandatory. Owner explicitly says **do not
wait** for this run: return its URL/ID after dispatch and stop. Do not describe
queued automation as a completed published release. AI setup on another PC is
covered by docs/ai-setup.md; fresh-machine acceptance remains a known limitation.

## Local build storage budget · 2026-11-18

Use only `target/build` for Cargo tests/builds. Working dependency caches are
retained, but storage is bounded: target budget 32 GiB, incremental budget 8 GiB.
After manual build/test batches run `scripts/maintain-build-cache.ps1` for a plan
and `-Apply` when needed; standard build-working already runs guarded maintenance
after installation. No unconditional cargo clean. Never prune runtime, models,
profile data or backups as build waste. Refuse active builds/running candidates,
linked paths and unverified installed artifacts. See `docs/devops.md` for policy
and initial storage incident; cleanup does not justify a product rebuild.

## Tag sort and resident images · 2026-11-18

Tags sorting is a domain `SortMode::Tags`, ordered by current scoped slot number,
untagged last. SQL grouping precedes LIMIT; cross-Service tag changes invalidate
cursors. Folder rejects it, virtual emoji catalogue keeps fixed order.
See `docs/patch-3.8/TAG-SORT.md`, including settings-aware rollback.
Same-namespace generation changes may carry only resident decoded image textures
by id/edge/content revision; still advance epoch and cancel stale queued work.
Do not carry arbitrary Shell visuals or relax late-reply checks.

## Inline tag modal/cache contract · 2026-11-18

Tag editor dismissal uses State::close(ctx), including focus-loss/hide and tab
summon, to release nested popup/input ownership while preserving failed drafts.
Only a successful checked inline style save can use accept_tag_style_update;
the exact Interaction generation/context bridge must preserve content-revision
and browse-context invalidation. Never remove generation from image identity
unconditionally or retain an old pagination cursor. Ctrl+Enter in pinned
properties uses the ordinary SetPinnedProperties path. See
`docs/patch-3.8/TAG-EDITOR-UX.md` for evidence and owner acceptance limits.
Explicit successful pin/tag/pinned-property updates use
`accept_item_metadata_update` with the same proven image-generation bridge but
always requery shelf metadata for changed search/order/filter membership. Never
replace that requery with only patching a row label or dropping invalidation.

## File Clipboard receipt settlement · 2026-11-17

Do not authorize Enter from a sequence captured before file Clipboard teardown.
`copy_out` settles after CloseClipboard/owner destruction using bounded exact
fresh-publication proof; failed proof is copy success with sequence=0, preserves
exports, and blocks automatic input. Delivery guards remain strict with no late
general file fallback. See `docs/patch-3.8/PASTE-SEQUENCE.md` for native incident
evidence and synthetic/native limits. Never add a fixed sequence delta or sleep.

## Enter follow-up · local 3.8 · 2026-11-17

Begin/Complete paste use immutable owner/target/visit leases. Native foreground
observation starts only in production UI before window creation, not ordinary
Service/test constructors. Never fallback to a stale summon on tracking failure,
retarget pending input, force foreign child focus or retry partial SendInput.
File publication owns a real HWND through CloseClipboard; publication facts are
bounded formats/counts only. `sent` means input submission, not recipient
attachment. See `docs/patch-3.8/PASTE-IMPLEMENTATION.md` for evidence/limitations.

## Local optional AI setup · 2026-11-17

Settings → AI uses `AiComponentsStatus` / `LaunchAiComponent`. Installation and
verification require explicit user action and a visible terminal, never a silent
background install. Embed version-bound helpers so executable-only updates can
install optional components without a repository. Do not equate file presence
with inference success or overwrite existing runtimes. Folder has an independent
CPU runtime; absent-only fallback preserves existing verified emoji installations.
See `docs/patch-3.8/AI-COMPONENTS.md` for evidence and clean-machine limitations.

## Local 3.8 global tab shortcuts · 2026-11-17

`SetTabGlobalHotkey` persists canonical per-tab global bindings; `SummonTabOverlay`
always shows the enabled target tab through the standard tab/view restoration
path and validated overlay capture. Never route it through the toggle command.
The native UI reports registration separately from persistence; hidden tabs keep
bindings unregistered. See `docs/patch-3.8/TAB-GLOBAL-HOTKEYS.md`. Local main and
primary installation only; no public release/push. Enter paste interoperability
is separately under owner-approved Azuros audit, not accepted as fixed.

## Owner workflow correction · 2026-11-17

For Katran, integrate completed local changes into local `main` and stage the
verified artifact into the primary application launched through Raycast, using
the existing real profile. A separate empty-profile preview is not the default
owner acceptance route. Preserve the previous immutable runtime and a consistent
pre-migration database backup for rollback. Do not force-terminate the running
application; an already open old process may need the owner to exit once before
the next normal launch activates the staged version. Reuse verified binaries;
installation does not justify recompilation. This decision supersedes the
earlier 3.8 instruction against replacing the installed build. Local main and
local installation do not authorize any GitHub push, tag or public release.
Native product acceptance still belongs to Lilia.

## Local 3.8 implementation · owner instruction 2026-11-17

Latest owner instruction persists the bounded Folder neural index across restarts.
Separate `folder-ai-index/{cpu,cuda}.sqlite3` files contain hashed descriptors and
validated vectors, bound to actual model/tokenizer/preprocessing/runtime identity.
Only missing/changed descriptors are embedded; absent entries are pruned. Cache
failure is optional and must preserve RAM search. Query text/plain folder paths
must not be stored by this cache, and tests must use a separate temporary cache
directory even when reading the installed model/runtime. No catalogue schema bump.

Latest owner UX instruction: Mini stream Text shows the first nonempty content
line unless explicitly renamed; all pinned rows show standard pin dots. Text
Ctrl+Left/Right switches Markdown/source while browsing the split pane. Neural
Folder search submits only on Enter, initially 10 results and 10 more on Down at
the last row, with snapshot-bound pagination. Optional GPU indexing is a persisted
checkbox using an isolated CUDA component, never a mutation of the emoji runtime
or silent CPU fallback. No public push/release is authorized. See current details
and Folder neural docs plus `VERIFICATION.md` for actual CUDA evidence.

Later owner instruction implements the previously researched Mini stream and local
Folder neural mode. `View::Details` must be discoverable through Display and Tab
view settings; a cursor-driven pane must not set full-screen preview or selection.
Folder picker physical backslash toggles neural mode, suppresses paired typing,
and shows gold styling; Ctrl+Y/configured Standard shortcut works inside the open
picker. Neural search reads bounded names/paths only, uses the installed optional
runtime, and exposes incomplete coverage. See `docs/patch-3.8/DETAILS-VIEW.md` and
`FOLDER-NEURAL.md`. Prior research-only statements below describe earlier scope.
Rollback must account for old binaries rejecting persisted `view: details` even
though schema remains v13. Back up settings/profile before primary staging.

Owner extension: drag-reorder complete tag definitions and assignments in Settings,
with revision-checked saves in both editors. No schema bump. Lilia confirmed the
base tags/search experience useful; the new third dense list/detail view and
optional Folder neural search remain separate research, not accepted implementations.
See `docs/patch-3.8/DETAILS-VIEW-RESEARCH.md` and
`docs/patch-3.8/FOLDER-SEMANTIC-RESEARCH.md`. Preserve ordinary search and do not
download a model or index personal folders merely to evaluate this possibility.

FEATURE-5 / Мастерская + Iris is implementing per-tab tag definitions and
assignments, an inline live tag editor, algorithmic Everything folder ranking,
direct Folder favorites/history and Windows Known Folders navigation.
See `docs/patch-3.8/TEAM.md` for the existing runtime roster and file ownership,
and `docs/patch-3.8/PRODUCT.md` for the complete contract. Reuse these sessions.
The scope does not authorize a public release, push, or replacing the installed
working build. Folder AI search is deferred until ordinary search is assessed.
Ctrl+Shift+Comma remains contextual: in the `/` tag palette it edits the
highlighted tag locally, without opening Settings or showing a material preview
or settings footer button. Actions retains its shortcut recorder. The owner now
approved rebindable Ctrl+Y for Standard folders in Folder.
The public baseline below remains 3.7.1 until a separately authorized release.
The local candidate uses schema v13: scoped tag definitions and assignments,
with transactional v12 migration after the standard consistent backup. See
`docs/patch-3.8/TAGS.md`; old binaries require the pre-v13 backup for rollback.

## Public release baseline · 3.7.1 · 2026-11-16

## Owner decision · unified Katran · 2026-11-15

Lilia replaced the earlier two-application and A7 private-only decision for the 3.7 line. Continue from the public 3.7 source as one application; emoji AI search is in this codebase and the model/runtime is an optional, separately downloaded local component. If missing, show a useful error and installation instructions. No Notion authentication or paid gate in 3.7; revisit entitlement and private distribution in 4.0. Earlier statements below that forbid AI code in public or require a separate E app are historical and no longer govern this work. Published 3.7.0 assets remain immutable.

For Desktop menus in 3.7.1, `Actions` resolves the selected or cursor asset once to classify background action availability; `SortActions` supplies Sorting without scanning Desktop assets. Both are metadata queries. Real wallpaper actions must still validate revisions and bytes at their execution boundary. Lilia confirmed native responsiveness after the change and comma toggling of Sorting in the installed build.

The 3.7.0 release source introduced PRODUCT-10 hardening. The former private E
build is historical migration input; current development uses one Katran host.
Read `README.md`, `docs/development.md`, and `docs/releases/next.md` for its
current implementation and evidence. Lilia's [author guide](https://app.notion.com/p/41649fda329680eeb371d42dbee517d8)
is valuable product context; it currently labels itself 3.2.3, so reconcile
individual claims with the code before using it as a 3.4 technical contract.
Earlier dated sections below record decisions at the time, not the current
release or authorization to push new work.

PRODUCT-10 source contracts in 3.7.0: opt-in stdio wire v2 encodes
unsafe JSON integers losslessly as canonical decimal strings; `CopySelectionAction`
owns emoji import plus clipboard outcomes; Desktop cursors bind browse context;
managed preview uses the asset path; durable Desktop pin tables are in numbered
schema v12; package manifests derive the schema version and include build
identity/input hashes; managed media uses staged publication, pending receipts,
action-boundary integrity checks, and a bounded Wallpaper Engine control wait.
N1 now prepares copy/import/paste and moves in background jobs and fixes repeated
Ctrl+Tab queuing, but final `safe_recycle` / `finish_cut` remains Service-bound;
do not claim end-to-end nonblocking. Synthetic debug medians are not release or
real trash measurements. A worker owner-job lease and ready-commit deferral are
still needed. The separate Avilon child-process review must cover its
spawn-to-assignment window. Targeted evidence and limitations are listed in
`docs/releases/next.md` and the internal
PRODUCT-10 status ledger. A7 applies
only to private E and must not add model,
AI command, UI, or implementation to this public source tree. A9's private E
runtime manifest contract has targeted synthetic evidence; the current source
handoff check has no unresolved boundary findings. N1 Folder transfer remains
partially open. Publication of later changes requires a new owner instruction.
The former public and E builds shared Pods/Desktop/settings storage. The local
launcher now selects the unified 3.7.1 candidate via `runtime/current-version.txt`
(`be05300517b4` on 2026-11-16); the shared profile reached schema v12 on 2026-11-15 with a
pre-migration backup and passing integrity check. A compatible local public v12
binary is staged separately. Public 3.4.1 and prior E `c1555b47dc04`
cannot open the migrated profile without controlled rollback.
For image background Actions, saved Images blobs have extensionless digest paths:
`EventKind::ManagedImage` admits them at the metadata boundary, while external
and Desktop images use their path type. Actual bytes and revision are validated
at the action boundary before applying Katran or Windows wallpaper.
The public category divider omits empty subgroup labels (scoped UI regression).
Avilon Windows media stop closes the assigned decoder process tree; its current
evidence is limited to the stop fixture, and does not close the spawn-to-assign
window.
Local 3.7 work also adds reusable Avilon palette/navigation policy from Telegram Glass.
Pods has a read-only, fully-qualified Unicode 16 emoji catalogue grouped from the bundled official emoji-test file; virtual IDs must never enter destructive or metadata mutation paths. Inside Pods only, physical backslash toggles it and Ctrl+backslash selects one of the two current collections; other tabs retain independent uses of the physical key. The catalogue category picker uses Shift+physical backslash only in the full catalogue; its session-only group filter must remain scoped to search and pagination. See `docs/pods.md`.
External-original moves from Downloads, Screenshots and explicitly permitted
references use the verified, handle-bound recycle path. `finish_cut` reports a
per-item recycle receipt and recovery location; it must not directly dispose
of the locked original. The final recycle/cleanup stage remains Service-bound.
Catalogue emoji use the ordinary revision-checked copy, previous-window paste and native drag paths. Ctrl+C additionally imports them into saved Pods; Enter uses the ordinary previous-window paste flow, and all-Pods native drag offers Unicode text alongside the existing `pods.txt` file. No AI search command, UI or model belongs in this public source tree; Katran E is a separate private distribution.
The 3.7 source adds the Desktop background library; see
`docs/releases/next.md` before changing background storage or tab routing.
Its cards now use the standard shelf Event, selection, and Actions path via
`src/desktop_projection.rs` and `src/desktop_selection.rs`; the separate
Desktop gallery was removed in local checkpoint `95eac10`.
Desktop browsing must remain metadata-only; do not hash or decode every managed
image during selection or Actions palette construction. Validate selected bytes
at the action boundary, and detect image/video types from managed paths rather
than display names (which may have no extension).
New shelf tabs must satisfy the standard Event interaction contract: browse,
navigation, multi-selection, pins, tag slots where pinning is available,
copy/drag-out, Folder transfer, preview, and delete. Declare any exception at
the domain command boundary and test it through the API before adding a GUI path.
Desktop pin/tag metadata is stored separately from capture-event rows, keyed by
its stable managed-asset ID; `src/desktop_pins.rs` owns that projection state.
Wallpaper Engine video handoffs use a durable ASCII-path copy and register
successful sends as `wallpaper_video` Desktop assets. The separate silent action
removes only the handed-off copy's audio stream. The last registered video is
protected from Desktop deletion; external Wallpaper Engine state is not tracked.

For each release or change to an architectural contract, update this current
baseline, the affected README/development-map passage, and the relevant release
note in the same local checkpoint. Keep historical decisions dated rather than
silently rewriting their original context. New work uses small local commits;
GitHub publication requires the owner's instruction for that work.

API First is mandatory, as explicitly required by the project owner.

Clipboard events were accepted by the owner after browser/filesystem testing
and merged from `experiment/clipboard-events`. `clipboard-events` is now a
default feature and the normal installed build uses the Windows listener.
Legacy polling is available only with `--no-default-features --features ui` for
explicit comparison; do not silently fall back to it on listener failures.

Start with the current README and [development map](docs/development.md).
Documents shelf-02/shelf-03 are historical snapshots, not current launch instructions.
For parallel agents, assign disjoint module files; one integrator owns shared
Command/dispatcher/schema/catalogue edits. See the map for extension recipes.

GitHub pushes use `python scripts/push-private.py`, which reads GITHUB_TOKEN
from the environment or ignored .env.local. Do not repeatedly request 1Password
approval when that credential is present. Never print or commit the token/file.

The owner performs Windows UI checks herself. Do not use computer-use automation
or manipulate her desktop unless she explicitly asks. Verify API/code with tools
and provide a runnable build for manual visual feedback.

- Implement every product capability in the Rust domain/command API first.
- Verify behavior programmatically through that API before building its GUI.
- GUI, CLI, capture adapters and Afio integration are clients of one dispatcher.
  Never implement a separate action or storage path inside a visual component.
- GUI input method and presentation must not determine domain behavior.
- Keep Katran independent of Afio. Afio is a native integration client.
- Keep success/error semantics explicit, including per-item batch failures.
- Use synthetic data for tests; never silently overwrite or delete source files.
- Keep technical contracts, build instructions and limitations with the code.

## Build and release discipline

Follow [DevOps execution rules](docs/devops.md). Reuse verified artifacts and
successful checks for unchanged inputs; retry only the affected stage. Preflight
metadata/licenses/tools before expensive compilation. Do not run minute-by-minute
LLM polling loops, duplicate clean builds or new release gates without a concrete
reason. Global owner rules in `~/.codex/AGENTS.md` also apply across projects.

## Agent collaboration

The owner accepted the optimized architecture, QuickLook, Actions and Tab as
version1.5.0 on2026-11-10 and explicitly authorized pushing this baseline to GitHub.
Do not infer authorization to push future experimental work from this release.

Use existing Garden masks. Read C:/Users/lilya/.codex/garden-personas/README.md,
run garden.py list and garden.py profile NAME before assigning a subagent.
The local SQLite mirror is the normal source; cards.md may not exist. Polly/Appolion leads;
mask names do not create persistent memory. Supply task contracts and durable
handoff/context instead of repeatedly spawning context-free replacements.

Owner refinement on2026-11-10: actively delegate suitable bounded work to the
Garden team to conserve the Pro budget. Reuse already-created agents within the
session for follow-up work instead of repeatedly creating replacements. Routine
implementation belongs to junior/mid masks; senior review needs a concrete reason.
Set effort explicitly: Terra medium/high for substantial implementation, low for
tiny fixes; Sol low; Luna high/xhigh; dedicated deep analysis such as Azuros xhigh.
Astra requires the owner's explicit approval before each invocation.
Keep disjoint file ownership and shared API integration with the lead; verify the
domain API before editing GUI clients. Do not create agents merely for names.

## Screenshots original-file exception · owner decision 2026-11-10

Screenshots is a live view of originals in the configured ShareX folder.
`DeleteSelection` may delete these originals after configured-folder containment and recorded content-revision checks.
Single-file deletion is immediate; multiple distinct IDs require confirmation. Report each
item failure. Existing `DeleteEvents` remains shelf-only. Images and other
external references retain the original non-deletion contract. Never delete
personal files as part of tests or reconciliation.

## Historical: Katran 2.5 local implementation · 2026-11-11

Work on `codex/katran-2.5` uses local Git checkpoints; no GitHub push is authorized.
The owner subsequently authorized local integration: main was fast-forwarded to
the verified UI/Shift+Tab feedback checkpoint c014984. GitHub remains unchanged.
Folder replaces All and remains session-only. Pinned guards every destructive
domain path, including Folder aliases. Managed Folder/Downloads/Screenshots
deletions use recoverable handle-bound staging and recycle-only Windows calls;
there is no permanent fallback. Recovery journals are excluded from discovery.
Check `docs/patch-2.5/TEAM.md` and `HANDOFF.md` before assigning followup work.
The accepted public version remains 2.0; a local 2.5 preview is not owner acceptance.

Owner update 2026-11-11: prior local Pods/native-paste/recovery behavior confirmed working. User explicitly authorized updating GitHub main and Notion, without release/tag; next release3.0. Tray/global show-hide are the current followup and still need native owner acceptance. Earlier no-push notes describe prior checkpoints.

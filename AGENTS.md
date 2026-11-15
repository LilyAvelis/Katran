# Katran development rules

## Public release baseline · 3.7.0 · 2026-11-15

The public 3.7.0 release source includes PRODUCT-10 hardening. Distinguish the
public release from the locally installed private E build.
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
Public and E share Pods/Desktop/settings storage. The local launcher selects E
`4f53749f7293`; the shared profile reached schema v12 on 2026-11-15 with a
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

# Katran development rules

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

## Katran 2.5 local implementation · 2026-11-11

Work on `codex/katran-2.5` uses local Git checkpoints; no GitHub push is authorized.
The owner subsequently authorized local integration: main was fast-forwarded to
the verified UI/Shift+Tab feedback checkpoint c014984. GitHub remains unchanged.
Folder replaces All and remains session-only. Pinned guards every destructive
domain path, including Folder aliases. Managed Folder/Downloads/Screenshots
deletions use recoverable handle-bound staging and recycle-only Windows calls;
there is no permanent fallback. Recovery journals are excluded from discovery.
Check `docs/patch-2.5/TEAM.md` and `HANDOFF.md` before assigning followup work.
The accepted public version remains 2.0; a local 2.5 preview is not owner acceptance.

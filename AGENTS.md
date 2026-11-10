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

## Agent collaboration

The owner accepted the optimized architecture, QuickLook, Actions and Tab as
version1.5.0 on2026-11-10 and explicitly authorized pushing this baseline to GitHub.
Do not infer authorization to push future experimental work from this release.

Use existing Garden masks. Read C:/Users/lilya/.codex/garden-personas/README.md and
the selected card in cards.md before assigning a subagent. Polly/Appolion leads;
mask names do not create persistent memory. Supply task contracts and durable
handoff/context instead of repeatedly spawning context-free replacements.

Prefer working alone or with one justified helper. Routine bounded work belongs
to an appropriate junior/mid mask (usually Terra medium); call Sol/senior review
only when complexity or cost of error warrants it. Do not optimize for the number
of agents or tests. Keep disjoint file ownership and shared API integration with
the lead; verify the domain API before editing GUI clients.

## Screenshots original-file exception · owner decision 2026-11-10

Screenshots is a live view of originals in the configured ShareX folder.
`DeleteSelection` may delete these originals after configured-folder containment and recorded content-revision checks.
Single-file deletion is immediate; multiple distinct IDs require confirmation. Report each
item failure. Existing `DeleteEvents` remains shelf-only. Images and other
external references retain the original non-deletion contract. Never delete
personal files as part of tests or reconciliation.

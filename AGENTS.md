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

Development builds for the 2.0 work are tested locally. Do not push them to
GitHub; publication waits for the owner's 2.0 release instruction. Local Git
diffs and checkpoints remain useful for review and rollback.

The owner sets product scope; the lead agent owns integration and architecture.
Use Terra (`gpt-6.1-terra`, medium effort) by default for simple file creation,
small edits, tidy-up and validation tasks. Delegate nontrivial bounded
implementation modules to Sol (`gpt-6.1-sol`, low effort), or the effort explicitly
requested by the owner. Luna may handle minor tasks when useful. Keep module ownership disjoint and shared API edits with the
integrator. Verify the domain API before editing GUI clients.

## Screenshots original-file exception · owner decision 2026-11-10

Screenshots is a live view of originals in the configured ShareX folder.
`DeleteSelection` may delete these originals after configured-folder containment and recorded content-revision checks.
Single-file deletion is immediate; multiple distinct IDs require confirmation. Report each
item failure. Existing `DeleteEvents` remains shelf-only. Images and other
external references retain the original non-deletion contract. Never delete
personal files as part of tests or reconciliation.

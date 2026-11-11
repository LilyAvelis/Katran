# Garden reusable components

Owner direction, 2026-11-11: begin extracting reusable solutions while the
separately tagged Katran 3.0 release proceeds. Work is local on
`codex/garden-framework`; it is not included in the release tags.

## First component: garden-actions

`crates/garden-actions` contains the shortcut policy extracted from Katran:
normalization, host-provided action definitions, lookup, validation of saved
overrides and immutable assignment planning with conflict replacement.
It has no dependencies, filesystem access, egui, Windows APIs, SQLite or Katran
commands. It returns values and typed errors; the host executes and persists.

The important distinctions are preserved:

- Missing override means use the default; an explicit `None` means unbound.
- A replacement removes intersecting alternatives and preserves the others.
- Protected commands cannot be displaced by confirming replacement.
- Reset checks the restored default for conflicts too.
- The host supplies reserved-key policy, including action-specific exceptions.
- A failed plan never changes the caller's map. The host must save successfully
  before replacing its live state.

Two different synthetic catalogues and a neutral executable example exercise
reuse. These establish a small Rust API, not compatibility with every project,
GUI framework or operating system. `/` retains Katran's alternative-chord
meaning, not sequential input.

## Host boundary

Katran retains its action catalogue, Russian wire errors, Enter migration,
JSON version/envelope, storage paths and write/rename behavior. Its dispatcher
continues to own assignments. Input capture, global OS registration and visual
shortcut recording are separate adapters.

The crate starts at internal version 0.1.0, `publish = false`, MIT. No crates.io
publication or new application release is authorized by this extraction.
Owner-supplied lily artwork is not bundled into the library.

## Second component: garden-egui

`crates/garden-egui` now owns ActionRow/ActionPresentation, primary-shortcut
presentation and measured KeyCaps. Katran's thin adapters supply its fonts,
semantic colors and exact Standard fallbacks; popup styling remains in Katran.
The shared rows are already used by Actions, Sort and Open With through that
adapter. They return egui responses and do not execute commands. The host owns
icon loaders, texture lifetime and theme state. Egui is pinned to 0.36.2.

## Use from another Rust project

Initially use local path dependencies; no external package registry is needed:

```toml
[dependencies]
garden-actions = { path = "../Katran/crates/garden-actions" }
garden-egui = { path = "../Katran/crates/garden-egui" }
```

Use only the first dependency if the application does not use egui. Supply a
catalogue and reserved-key callback to `plan_set`, then save its returned map
before adopting it. For rows, construct `ActionStyle`/`KeyCapsStyle` from the
application's own design tokens; never copy a Katran dispatcher into a widget.

The console example is runnable with
`cargo run --offline -p garden-actions --example shortcuts`. It does not open a
window, register a hotkey or modify settings. Each crate carries its own MIT
license; dependencies retain their own licenses. README usage and limitations
are included beside each crate.

## Verification and checkpoints

- `garden-actions`: six policy tests, two distinct catalogues and the neutral
  example compiled; crate/all-target Clippy passed. Invalid or noncanonical host catalogues are
  rejected before assignment, preventing aliases from hiding conflicts.
- Katran adapter: 23 targeted API tests and two original shortcut unit tests
  passed. The new storage-failure test confirms the planned binding does not
  become live when persistence fails. Walter's bounded comparison against
  `9fe7214` found no P1/P2 semantic changes in the policy extraction.
- `garden-egui`: two headless layout tests cover narrow/wide rows, DPI scaling,
  custom textures and ancestor scroll clipping. Physical-pixel image snapping
  uses the original half-pixel tolerance.
- Katran after UI extraction: all 113 UI/headless tests passed, including the
  existing production-catalogue geometry and scrolling cases. Workspace
  all-target Clippy with `ui,clipboard-events` passed. No native window was
  opened or owner desktop manipulated.

The three synthetic PowerShell contracts for release packaging, preview
snapshotting and local installation also passed. Provenance includes the two
crates’ manifests, source files and licenses, and excludes nested build/cache
files. A changed library input invalidates the matching local snapshot.

The implementation checkpoints are local Git commits; release tags continue to
reference their original source. Library extraction is not evidence that the
separate public release pipeline completed.

## Next reusable pieces

The Sort palette and Ctrl+K can next share an injected choice model and modal
input ownership when a second host needs those behaviors.
Everything, Open With, QuickLook and native overlay remain optional Windows
adapters; they do not belong in the pure policy crate. File ownership, source
revisions, pinned protection and recovery remain Katran domain behavior.

See [source inventory and boundaries](reusable-library-map.md) and
[UI design patterns](design-patterns.md). Verification results are recorded with
the integration checkpoint, rather than inferred from this design document.

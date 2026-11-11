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

## Next reusable pieces

After this component is integrated and verified, the next bounded extraction
is egui ActionRow/KeyCaps with host-supplied visual tokens. The Sort palette and
Ctrl+K can then share an injected choice model and modal input ownership.
Everything, Open With, QuickLook and native overlay remain optional Windows
adapters; they do not belong in the pure policy crate. File ownership, source
revisions, pinned protection and recovery remain Katran domain behavior.

See [source inventory and boundaries](reusable-library-map.md) and
[UI design patterns](design-patterns.md). Verification results are recorded with
the integration checkpoint, rather than inferred from this design document.

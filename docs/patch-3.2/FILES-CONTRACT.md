# Files 3.2 · foundation and integration handoff

The first API slice is implemented on `codex/katran-3.2`. `AddFiles` creates
reference-only records in the v9 `files_pointers` table; the Files tab is visible
by default and its catalogue projection is separate from Images. Dragged files
that are not native images use this API. Native Windows appearance is unverified.

## Product contract

- Files stores a reference to a regular file, not a managed duplicate. Removing
  its record must not remove the original. A missing original remains an unavailable
  pointer that the owner can remove; never search the disk for it automatically.
- Files Ctrl+C and drag transfer the real file. Ctrl+Tab copies to the selected
  Folder and retains the Files pointer. Shift+Tab moves to Folder and removes the
  pointer only after a verified successful move.
  Pinned objects and aliases of pinned originals remain copy-only. Individual
  failures must be reported and must retain unsuccessful pointers and sources.
- Files is visible by default. Existing category routes and behavior remain intact.
  Clipboard, drag, and Folder return routing and permitted types need a single
  explicit decision before changing capture behavior. Do not re-ingest Katran's
  own Clipboard publications or silently route failures as imports.

## Isolated helper

`prepare_pointer` checks an absolute path, rejects final symlinks/reparse points
and non-regular files, canonicalizes its locator and hashes its bytes. It does not
copy or delete. `BeginJob(AddFiles)` performs preparation away from the UI thread
and returns one result per input. Commit rechecks path, type, size and modification
time, then inserts or reuses the pointer in a writer transaction. No name is
reserved and no original is modified by this step.

This snapshot is **not** an authorization to mutate the source: path and metadata
can race or be replaced, including while hashing. Hash and timestamps do not prove
file identity; identical bytes or paths do not establish pointer deduplication.
The effect must use existing handle-bound source validation, pinned-alias protection,
and no-overwrite conflict handling at commit time, with per-item outcomes. Recheck
the destination and its containment at the effect boundary as required by that
adapter. Do not use the helper as proof that an original can safely be deleted.

## Integrated behavior and remaining limits

1. The current identity is the exact canonical path locator in `files_pointers`.
   Re-adding that path preserves the original event and metadata; aliases/hard
   links and external renames are not inferred. A missing original leaves an
   unavailable pointer. Re-adding a changed file at the same path currently keeps
   the old revision; a separate explicit refresh contract is needed.
2. External Clipboard CF_HDROP native images use the existing image route;
   regular non-images prepare and commit through Files pointer domain. Both
   preparation and commit check the category eye. File export preserves
   CF_HDROP, bitmap and drop effect while adding a bounded private owner marker.
   A failed marker publication is reported as a warning; late-sequence ownership
   cannot be guaranteed in that case. Native interoperability remains for the
   owner's manual check.
3. The existing transfer implementation handles destination conflicts, content
   verification and recoverable source handling. A move removes a pointer only
   after confirmed source disposition; failed and partial moves report their
   outcome. The pointer helper is never used as pathname-only proof for deletion.
4. Synthetic API tests cover persistence/restart, pointer-only deletion,
   direct copy/move, pinned aliases, mixed failures, async preparation and mixed
   Clipboard capture. Native cross-volume acceptance belongs to the owner.

Open decisions include Folder-return routing and explicit refresh of a changed
original. Identical external Text/Links arrivals use
exact-value event reuse in the separate corrective ticket.

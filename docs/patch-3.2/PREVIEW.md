# Katran 3.2 local preview

This is an implementation checkpoint on `codex/katran-3.2`, not an accepted
release. The installed/public version remains 3.1.1. No GitHub push, tag or
distribution package is implied by this document.

## Manual Windows review

1. Open Files. It is visible by default. Add files with **＋ Файлы**, by dragging
   them in, or by externally copying regular files. Files stores references;
   deleting a Files card must leave the original intact. Missing originals stay
   visible as unavailable pointers.
2. Select a Files card and use `Ctrl+Tab` to copy into the selected Folder.
   The Files pointer remains. Use `Shift+Tab` to move; the pointer disappears
   after success. A pinned source is copy-only. Review per-item failures for a
   mixed selection and name collisions.
3. Use `.` in any numbered tab to toggle its pinned-only view. Double-pressing
   the tab number still does the same. Type into a text field to check that
   punctuation stays in the editor.
4. Pin a card, open the tag menu with `/` or the footer tag icon, and assign one
   of nine slots. `Ctrl+T` opens that same menu. Filter by a slot and use All to
   clear only the tag filter. Unpin the card: its assignment disappears, while
   the slot definition remains. Re-pin does not restore the assignment.
5. In `Ctrl+,` → Теги, edit a slot name, color and Color/Dots material. Check
   Apply/Cancel and the Standard/Blue preview. Inspect real cards in both
   themes, including a selected card and cursor. The settings preview does not
   reproduce native GPU glass.
6. Copy identical Text and Links twice from an external app: each value should
   retain one card. Copy a Text or Link card out of Katran: that publication
   should not add a card. Externally copied equal Tables retain their existing
   behavior. Test a mixed file/text Clipboard publication if available.

Only synthetic files and data are used in automated checks. The owner performs
native UI and Clipboard checks on her own desktop.

## Domain and integration contracts

- `AddFiles` and `BeginJob(AddFiles)` prepare references away from the UI and
  commit through the dispatcher. The Files catalogue uses `files_pointers`;
  external clipboard files and drag use the same pointer domain for non-image
  regular files. Native images retain the image route. Source files are not
  copied on pointer creation.
- `CopySelection` and `TransferSelection` retain their Folder contracts. The
  former keeps the Files pointer; the latter removes it only after a successful
  verified move. Pinned originals and aliases stay copy-only.
- Nine slot definitions persist independently of object assignments. A card
  gets at most one slot. Assignment requires pin; unpin deletes assignment in
  the pin transaction. Filtering intersects the current category and only
  exposes pinned assignments. `GetTagSlots` batches card lookup.
- Text/Links exact-value dedup reuses an existing ID without merging historical
  duplicates. Private owner tokens distinguish Katran's own Text/Links/files
  Clipboard publications from later external copies; sequence receipts remain
  an additional short-circuit. Tables keep their existing owner marker.

## Known boundaries

- Native appearance, GPU glass, contrast, DPI and Windows Clipboard
  interoperability require the owner's manual review. The marker can fail to
  publish after a successful file export; the API reports a warning, and a
  later sequence then lacks the owner proof.
- A Files pointer uses canonical path identity. External rename is not
  followed. Re-adding changed content at that path currently retains the
  existing pointer/revision; an explicit refresh command is a separate task.
- The old `SetPinnedTags` API remains for compatibility, but the 3.2 UI uses
  one-slot commands and no longer exposes the legacy multi-tag editor.

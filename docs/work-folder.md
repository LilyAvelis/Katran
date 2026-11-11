# Folder · local Katran 2.5 preview

Katran 2.5 is an in-development local preview. The accepted public release is
still 2.0.0. In this branch, Folder replaces All as the live view of the folder
you choose. It shows immediate regular files and refreshes its listing; the
chosen path is session state, not a copied/imported collection.

## Choose and browse

- **Ctrl+Shift+K** opens folder search through Everything. Enter selects the
  highlighted result. **Ctrl+H** and **Ctrl+F** switch the picker to recent
  folders and favorites.
- **Tab** toggles Folder and the remembered shelf tab. **Ctrl+.** adds or
  removes the current Folder from favorites.
- Files from Folder can be previewed in Katran where supported. The preview is
  read-only.

Everything must be installed and running. Search uses the bundled Katran
backend; Katran does not bundle the Everything application.

## Copy, move, and return files

Owner clarification2026-11-11 supersedes the old copy-only return behavior:

- **Shift+Tab / TransferSelection** moves in both directions. Category→Folder
  verifies the destination before source disposition; pinned objects remain copy-only.
  Folder→category first commits a complete typed import, then recycles the verified
  original. Unsupported/partial imports keep their source and report per-item errors.
- **Ctrl+Tab / CopySelection** copies in both directions, retains the current view
  and selection, and permits repeated copies. Destination name collisions create
  numbered names rather than overwrite. Folder image imports use independent
  managed blobs, so later moves to different folders are possible.
- Folder image import is bounded by quota and256MiB per source and preserves the
  original modified date. It never points the new Image at the Folder source.
- The explicit Shift+Tab move also supports legacy external references, after
  holding their recorded-hash source and verified destination. This is the owner's
  named move authorization, not a change to generic shelf deletion or Clipboard Cut.
  Managed-storage aliases are rejected as move sources; copying remains allowed.
  A same-folder external reference can be removed from the shelf without deleting
  the already-present file. Pin aliases, source revisions and managed roots remain protected.
- No selected Folder reports «Папка ещё не выбрана». Ordinary **Tab** changes view;
  Ctrl+C remains external copying. The two direct modes never read the Clipboard.

- **Ctrl+X** stages selected shelf objects for a move. It does not remove the
  source immediately. Open Folder and press **Ctrl+V** to complete a verified
  transfer. Pinned items stay copy-only. Partial outcomes are reported per
  item. Same-folder Clipboard transfer remains a no-op.
- **Ctrl+V** in Folder also materializes files from the external clipboard as
  copied files. External clipboard cut/move is not supported.
- **Ctrl+Shift+C** in Folder explicitly adds selected files to Katran. Current
  supported imports include fresh TXT/MD, CSV, images, and bookmarks HTML as
  per-URL link objects. Unsupported formats return an error. Imported files do
  not silently inherit old metadata.
- Negative Folder selection IDs are session-local. Clipboard Cut is explicitly unavailable
  for these entries; explicit Shift+Tab move, copy and revision-bound recycle deletion are supported.

Folder deletion, where available through the managed-file action, stages the
verified original and asks Windows to recycle it. Recovery may expose a manual
path if automatic restoration cannot safely put a file back. Downloads and
ShareX Screenshots managed originals also use Windows recycling in the 2.5
branch. No permanent-delete fallback is intended.

## Information panel

The header has only the application title, search and tabs. Folder paths,
sorting, operation details and recovery records are in the fixed-height bottom
information bar. Click a section for a bounded details panel; long paths and
errors do not reduce the gallery height by expanding the header.

## Current limits

Properties and tags presentation is implemented; explicit return import creates
fresh typed objects for the supported formats. Raycast clipboard filtering is not implemented:
no reliable source marker has been established, so intentional emoji are kept.
See the [preview notes](releases/2.5.0-preview.md) for verification state and
the [preview build procedure](patch-2.5/BUILD.md) for the unaccepted artifact.

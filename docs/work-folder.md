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

- **Ctrl+X** stages selected shelf objects for a move. It does not remove the
  source immediately. Open Folder and press **Ctrl+V** to complete a verified
  transfer. Pinned items stay copy-only. Partial outcomes are reported per
  item. Same-folder transfer is a no-op.
- **Ctrl+V** in Folder also materializes files from the external clipboard as
  copied files. External clipboard cut/move is not supported.
- **Ctrl+Shift+C** in Folder explicitly adds selected files to Katran. Current
  supported imports include fresh TXT/MD, CSV, images, and bookmarks HTML as
  per-URL link objects. Unsupported formats return an error. Imported files do
  not silently inherit old metadata.
- Negative Folder selection IDs are session-local. Cut is explicitly unavailable
  for these entries; ordinary copy and revision-bound recycle deletion are supported.

Folder deletion, where available through the managed-file action, stages the
verified original and asks Windows to recycle it. Recovery may expose a manual
path if automatic restoration cannot safely put a file back. Downloads and
ShareX Screenshots managed originals also use Windows recycling in the 2.5
branch. No permanent-delete fallback is intended.

## Current limits

Properties and tags presentation is implemented; explicit return import creates
fresh typed objects for the supported formats. Raycast clipboard filtering is not implemented:
no reliable source marker has been established, so intentional emoji are kept.
See the [preview notes](releases/2.5.0-preview.md) for verification state and
the [preview build procedure](patch-2.5/BUILD.md) for the unaccepted artifact.

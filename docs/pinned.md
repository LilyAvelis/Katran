# Pinned objects · local Katran 2.5 preview

Katran 2.5 is under development; the accepted public release remains 2.0.0.
Pinned objects form category-local working sets. Pinning protects a selected
object from Katran's destructive shelf and managed-file actions. Folder files
cannot themselves be pinned, but a Folder alias of a pinned real file remains
protected.

Pinned properties include a display name, an opener choice, and tags. The
property editor is available with **Ctrl+E** on a selected pinned object.
**Ctrl+T** opens the tag filter; it filters the current category's pinned set.
Tags do not cross categories. The saved opener is an opaque application handler
ID; native browser URL opening has not been confirmed.

History and folder favorites are separate: clearing history does not clear
favorites. Folder favorites are managed from Folder with **Ctrl+.**; search,
history and favorites are available in the folder picker with **Ctrl+Shift+K**,
then **Ctrl+H** or **Ctrl+F**.

The 2.5 database migration to schema 8 is additive and has a pre-migration
backup. Existing external file timestamps remain unchanged. Katran-owned objects
use their ingest time as the modified time on late export. This preview is not
accepted as a public release; see the [2.5 preview notes](releases/2.5.0-preview.md).

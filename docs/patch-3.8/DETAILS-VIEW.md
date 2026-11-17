# Mini stream / Мини-поток · implementation 2026-11-17

The third view is now implemented, after the earlier research checkpoint.
Select **Actions → Display → Mini stream**, or **Settings → General →
Appearance → Tab view → Mini stream**. Russian label: **Мини-поток**.
It is saved independently for each tab. No new shortcut was assigned.

The left pane is a virtualized 40-point list, approximately 38% of the width,
with type icons, titles, tag material, selection and cursor indicators. The
right pane follows the cursor, showing preview above independently scrollable
metadata. Below 480 points the panes stack vertically. Rows remain fixed-density;
density commands leave this view and the other views' preferences unchanged.

All providers retain their ordinary domain interactions: selection modifiers,
keyboard navigation, Actions, copy, drag, pin/tag and delete restrictions. Opening
the view does not activate full-screen preview or change the selected set. Space
continues to use the ordinary preview behavior. Multi-selection actions still
apply to the selected set; details describe only the cursor object.

Images use the bounded visual worker; Files may show a thumbnail or a labeled
icon fallback. Text, Markdown, Links and Tables reuse existing read-only async
workers. Links show their actual URL rather than a fabricated website screenshot.
Pods render the emoji and fetch Unicode metadata with a latest-only worker.
No new network request, automatic media playback or file opening was added.
Unavailable content is reported without removing the object from the list.

Worker keys now include browse context epochs, preventing late A→B→A responses
from repopulating a different context. Desktop uses its managed ID; working-Folder
snapshots use their revision-bound paths. Virtual emoji do not enter file routes.
Metadata omits absent/zero dates and internal IDs. No decoding or hashing of every
Desktop asset is performed merely to browse this view.

## Compatibility and rollback

Schema remains v13. Old Gallery/Stream settings load with existing defaults.
Older binaries do not recognize the newly persisted `details` view value and may
reject settings even though they understand schema v13. For controlled rollback,
stop Katran and restore the pre-change settings/profile backup, or explicitly
replace `details` values with `list` in the saved settings before using the older
binary. Do not assume changing only the executable pointer is sufficient.

See [architecture research](DETAILS-VIEW-RESEARCH.md) and [UX research](DETAILS-VIEW-UX.md)
for the initial reasoning. Native visual acceptance belongs to Lilia.

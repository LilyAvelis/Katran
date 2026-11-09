# Drag-out and Space preview · 1.0.1

Space toggles the large centered in-window preview; Enter and double-click still
open it, Escape closes it. Arrow keys in preview move one item at a time rather
than by grid rows. The current visible ordering/search is respected. The adjacent
previous/next previews are requested at lower priority than the current image.
This is an in-window fit view, not a pixel zoom editor or OS fullscreen toggle.

Drag a selected gallery card to export the selected group; dragging an unselected
card selects and exports that item. In large preview, drag the displayed image to
export that image. The Windows OLE adapter permits COPY only; originals are never
moved, and the system Clipboard is not replaced by the drag operation.

API commands: `toggle_preview`, `navigate_preview {delta}`, and
`drag_events_checked {items:[{id,revision}]}`. UI gestures call these commands;
the same snapshot validation and copy preparation used by Copy protects drag
payloads. Invalid/stale items and partial failures are surfaced explicitly.
Escape or releasing the mouse before preparation completes can cancel the drag.

OLE receives a Shell data object built from absolute file PIDLs, supporting a
group across directories. It provides a standard CF_HDROP payload. See Microsoft's
[drag-and-drop responsibilities](https://learn.microsoft.com/en-us/windows/win32/com/drag-and-drop).
The adapter owns the STA apartment, PIDLs and COM references for the operation.
Data preparation/OLE run in the existing exclusive dispatcher job thread, not
the rendering thread.

Successful transfer snapshots remain under `katran-data/drag-cache/exports` so a
target can read them after drop. This cache has a separate budget equal to the
configured blob quota. Previous drag batches retire only after a successful new
drop; cancellation preserves them and cleans the abandoned preparation. Clipboard
export batches are kept separately. Older deferred consumers are not guaranteed
after a subsequent successful replacement. No browser upload completion is claimed
by the native drop result.

Tests verify API navigation, standard file-list extraction from the real Windows
Shell data object for two synthetic files in different folders, and synthetic
drop-source cancel/release states. They do not run DoDragDrop, change Clipboard,
automate the desktop, or verify any website. ChatGPT/Grok/Telegram/Explorer drop
acceptance must be checked manually by the owner. Website attachment-count and
format limits still apply; drag-out does not bypass them.

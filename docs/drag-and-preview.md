# Drag-out and Space preview · 1.0.2

Space toggles the large centered in-window preview; Enter and double-click still
open it, Escape closes it. Arrow keys in preview move one item at a time rather
than by grid rows. The current visible ordering/search is respected. The adjacent
previous/next previews are requested at lower priority than the current image.
This is an in-window fit view, not a pixel zoom editor or OS fullscreen toggle.

Drag a selected gallery card to export the selected group; dragging an unselected
card selects and exports that item. In large preview, drag the displayed image to
export that image. The Windows OLE adapter permits COPY only; originals are never
moved, and the system Clipboard is not replaced by the drag operation.

API commands: `toggle_preview`, `navigate_preview {delta}`, and two-phase drag:

1. `drag_events_checked {items:[{id,revision}]}` validates and prepares owned
   snapshots, returning `drag_token`, `count` and per-item `failures`. It does not
   enter OLE. There is at most one pending preparation per Service; replacement,
   cancellation and Service drop discard its unpublished files.
2. `run_prepared_drag {token,owner_window}` consumes that token once and publishes
   through the native adapter. Invalid tokens cannot consume another preparation.
   The caller must run this on the thread that owns the HWND, outside egui locks.
   An invalid/foreign owner or wrong thread returns an error before entering OLE.
3. `cancel_prepared_drag {token}` explicitly abandons a prepared gesture.

UI gestures call these commands; Copy's snapshot validation/preparation protects
drag payloads. Escape or releasing the mouse before native start cancels the drag.

OLE receives a Shell data object built from absolute file PIDLs, supporting a
group across directories. It provides a standard CF_HDROP payload. See Microsoft's
[drag-and-drop responsibilities](https://learn.microsoft.com/en-us/windows/win32/com/drag-and-drop).
The adapter owns the STA apartment, PIDLs and COM references for the operation.
Data preparation runs in the exclusive dispatcher worker. The native modal loop
runs on the window thread via eframe's `raw_input_hook`, before `Context::run_ui`.
It releases winit's mouse capture on that same thread before OLE takes over. Never
move DoDragDrop into a generic worker: its input queue does not own the window's
captured mouse, so the gesture can remain pending after release. Egui's drag state
is reset after return; the service becomes available on success, cancel or error.

A bounded 160-pixel thumbnail is prepared in the background using the preview
cache and attached via `IDragSourceHelper::InitializeFromBitmap`. Windows displays
it beside the cursor on compatible targets. Preview failure is cosmetic and
reported as a warning. No image decode takes place inside the modal OLE loop.
See Microsoft's [ReleaseCapture contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-releasecapture)
and [drag-image helper](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-idragsourcehelper-initializefrombitmap).

Successful transfer snapshots remain under `katran-data/drag-cache/exports` so a
target can read them after drop. This cache has a separate budget equal to the
configured blob quota. Previous drag batches retire only after a successful new
drop; cancellation preserves them and cleans the abandoned preparation. Clipboard
export batches are kept separately. Older deferred consumers are not guaranteed
after a subsequent successful replacement. No browser upload completion is claimed
by the native drop result.

Tests verify API navigation, worker-to-window-thread handoff with an injected
native runner, one-shot tokens, cancellation/error cleanup and continued API
access. They also verify standard file-list extraction from the real Windows
Shell data object for two synthetic files in different folders, invalid owner
rejection, and synthetic drop-source cancel/release states. They do not run DoDragDrop, change Clipboard,
automate the desktop, or verify any website. ChatGPT/Grok/Telegram/Explorer drop
acceptance must be checked manually by the owner. Website attachment-count and
format limits still apply; drag-out does not bypass them.

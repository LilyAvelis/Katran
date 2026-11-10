# Tabs and ShareX · 1.1.0 preview

First foundation for 2.0; this is not the complete 2.0 release.

## Contract

`SetTab {tab: all|images|screenshots}`, `ToggleTab` and `BrowseShelf
{query,limit,after_id}` use the single dispatcher. Tab toggles All and the last
non-All tab (initially Images). Gallery/Stream is independent of the tab.
Tab memory is process-local. Filtering preserves selection, including hidden
items; copy/drag resolve the exact selected IDs and revisions through the API.
Images includes supported image filename extensions, excluding source `sharex`;
Screenshots includes that source; All includes all shelf records. Search and
pagination apply to the chosen tab. File type is currently extension-based.

Actions `images` (Ctrl J), `screenshots` (Ctrl H), and `all` are rebindable.
`tab_toggle` (Tab) and `commands` (Ctrl K) are protected; settings is rebindable.
Navigation/editing keys retain their existing contextual input reservations.

## Configure and scan

In Settings, set the absolute ShareX screenshot folder and save. Empty disables
scanning. The folder is chosen explicitly; Katran does not change ShareX settings
or assume its installation location. Existing subfolders are included.

`Settings.sharex_folder` defaults to null for old preferences. `ScanSharex` is the
same command used by GUI background reconciliation (approximately every two
seconds, while the process runs). It returns `scanned` and per-file `items`
with explicit success/errors. It operates independently of Clipboard capture.
The Windows Clipboard listener remains the default without a polling fallback.

A supported nonempty image must be readable with unchanged size and modification
time across two scans. This is a readiness heuristic, not proof that a writer
has closed a file. PNG, JPEG, WebP, GIF and BMP are supported. Source files are
referenced, never copied or deleted. The normal preview decoder handles viewing.
Each observed revision is offered once per scanner lifetime; durable path/content
receipts prevent unchanged files being duplicated after restart or resurrected
after shelf deletion. Changed content may produce a new shelf event.

This first adapter polls folder metadata; it is not a native directory event
watcher. Traversal skips symlinks and stops with an explicit error above 20,000
entries or on directory enumeration failure. Large trees cost a metadata scan
every interval and temporarily occupy the single dispatcher worker. Intermediate
file revisions, edits preserving size and mtime, and brief files between scans
may be missed. Content hashes are read when a stable file is offered. No scanning
continues after Katran exits. Downloads and video playback are not implemented.

## Manual acceptance

1. Close the old Katran yourself; launch the installed 1.1.0 Test.
2. Click All / Images / Screenshots; from Screenshots press Tab twice and verify
   return to Screenshots. Check Ctrl J / Ctrl H and rebind them in Settings.
3. Configure the ShareX screenshot folder, save, wait for two scans. Check old
   screenshots and a newly captured screenshot, including a nested date folder.
4. Preview, copy and drag screenshots. Select images in different tabs and check
   the combined selection and transfer. Delete a screenshot card and verify its
   source remains and the card stays removed after restarting Katran.
5. Check capture still works when minimized and scanning errors remain visible.

Automated tests use synthetic folders and files; no desktop or personal Clipboard
contents are manipulated.


## Revised model · 1.1.1 Test · owner decision

Screenshots is a live view of originals in the currently configured folder,
including nested date folders. Missing originals are hidden after reconciliation;
the newest recorded revision of each path is displayed. Availability changes
trigger refresh without requiring a new file arrival. Folder entries carry the
ShareX folder label; this identifies the configured source, not proof of the
program that created every file in that folder.

Images remains the Katran shelf. Automatic Clipboard captures with known ShareX
origin are excluded. Historical ShareX Clipboard copies are preserved in storage
but hidden in the shelf projection; no automatic destructive migration occurs.
Unknown Clipboard origins cannot reliably be recognized as ShareX.

`DeleteSelection {ids,confirmed}` is the user-facing delete command. Any ShareX
original is deleted immediately when selected alone. Multiple distinct selected
objects require confirmation via Enter. The batch confirmation explicitly warns
that files are deleted from disk permanently (not the Recycle Bin). The command
checks current configured-folder containment and recorded content revision, then
reports success/error per item. Mixed selection follows each object's ownership.
`DeleteEvents` remains the legacy shelf-only command; reconciliation never deletes
source files. Existing export snapshots may continue to support pending paste.

The earlier 1.1.0 shelf-only screenshot-deletion description above is superseded.
Manual checks: remove an original externally and wait for refresh; overwrite one
and wait for stable revision; verify folder/date subfolders; confirm group Delete warns
about originals and Escape leaves them untouched. Then confirm deletion
of disposable screenshots via Enter and verify disk removal; a single disposable
screenshot must delete immediately. Do not test on valuable files.

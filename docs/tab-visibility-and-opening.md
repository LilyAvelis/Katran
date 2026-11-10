# Tab visibility and file opening

This document describes the domain/API behavior for tab switches and opening
existing shelf items. GUI wiring and physical Windows acceptance are tracked
separately; this is not an acceptance report.

## Tab visibility

`Settings.hidden_tabs` defaults to `[]`. `All` is fixed and always enabled.
The `Tabs` command returns the enabled tabs in their compact visible order.
Number keys `1` through `9` address that compact order; hiding a tab closes the
gap without changing the saved full tab order. The settings are persisted.

```json
{"api_version":1,"request_id":"tabs","command":{"type":"tabs"}}
{"api_version":1,"request_id":"hide-images","command":{"type":"set_tab_enabled","tab":"images","enabled":false}}
{"api_version":1,"request_id":"toggle-downloads","command":{"type":"toggle_tab_enabled","tab":"downloads"}}
{"api_version":1,"request_id":"open-tab-settings","command":{"type":"show_tab_settings"}}
```

Disabling Screenshots or Downloads prevents that source's scanner from running.
Downloads also updates the legacy `downloads_enabled` setting so older settings
and clients retain their behavior. Disabling Images stops clipboard image
ingestion, including image files from clipboard references before snapshot
preparation. It does not disable Screenshots or Downloads: images in those live
source views remain independent. `All` omits saved ordinary image items while
Images is disabled, but `Get` by item ID remains available and no saved history
is deleted. Re-enabling Images makes those items visible again.

If the selected tab is disabled, selection returns to All. The remembered tab
used by the tab toggle is repaired to the first enabled tab, or All when no
numbered tabs remain. Commands selecting a disabled tab return `tab_disabled`;
attempting to disable All returns `fixed_tab`.

## Opening and revealing files

The later [application picker](open-app-picker.md) replaces the shortcut's native
dialog with Katran's own panel. `OpenFile` with `open_with` remains the explicit
native fallback; the palette action now returns an application-list session.

The versioned command is `OpenFile{id, operation}`. Operations serialize as
`open`, `reveal`, and `open_with`. The operation targets the existing item ID;
it does not create a second shelf item. Shell work is scheduled through the
domain job path by clients that need asynchronous execution.

```json
{"api_version":1,"request_id":"open","command":{"type":"open_file","id":42,"operation":"open"}}
{"api_version":1,"request_id":"reveal","command":{"type":"open_file","id":42,"operation":"reveal"}}
{"api_version":1,"request_id":"open-with","command":{"type":"open_file","id":42,"operation":"open_with"}}
```

The file is validated immediately before the Windows Shell request. A missing
source returns `file_unavailable`; content changed since its recorded revision
returns `source_changed`. A successful receipt means the Shell request was
accepted, not that the external application finished loading the file.

For reference items, Open and Open With target the original file. Reveal
selects that file in Explorer. For cached blobs, Open and Open With first create
a named independent snapshot under `<data-root>/open-cache/exports`. Its
extension is preserved. The snapshot survives restarts and shelf deletion, and
is charged to a separate reject-new budget equal to the configured blob quota.
There is no automatic cleanup because an external application may still hold
the path; explicit cleanup is needed when that budget is exhausted. Reveal
targets the actual owned blob path, not an export snapshot.

`OpenCurrent` remains as a compatibility command for the legacy internal image
preview. It is separate from `OpenFile` and should not be treated as an
external Shell open operation.

## Windows owner acceptance checklist

After the UI client is wired, check the following in a normal installed build:

1. In Settings → Tabs, hide and restore each numbered tab. Confirm All remains
   present, number shortcuts compact around hidden tabs, and the selection
   falls back to All when its tab is hidden.
2. Disable Screenshots and Downloads and confirm their scanners stop; re-enable
   them and confirm scanning resumes.
3. Disable Images and confirm clipboard image ingestion stops, existing ordinary
   saved images disappear from All but remain retrievable by ID, and live
   Screenshots/Downloads images continue to appear.
4. For a reference file, check default Open, Ctrl+Enter reveal in Explorer, and
   Ctrl+Shift+Enter Open With.
5. For a cached image, check Space internal preview and the external operations.
   Confirm the opened copy has its original extension and edits to it do not
   alter the shelf blob. Confirm Reveal selects the actual blob.
6. Confirm QuickLook remains an explicit, unbound action. Check a missing or
   changed source reports an error instead of opening stale content.

No desktop automation is part of this checklist. API job tests use fake Shell
launchers and synthetic files; physical Windows acceptance belongs to the
owner.

## Verification · 2026-11-10

Full API/headless run: 230 passed, 2 intentionally ignored (the existing explicit
latency report and isolated real-bridge exercise). The final capture-policy
refinement additionally passed all 5 asynchronous capture tests. Clippy with
`-D warnings`, rustfmt and the synthetic installer contract passed.

Installed release-profile working build: `d65f0034f32f`, version label remains
1.5.0 pending the owner's release decision. Stable launcher registration completed
without launching the UI. File-opening Shell behavior and the settings appearance
still require the owner's physical Windows acceptance. No new GitHub push.

The installer now waits for the GUI-subsystem registration process and checks its
actual exit code before switching the build pointer. General Settings saves merge
the latest tab visibility/order, preserving changes made through Actions or the
main tab bar while the settings draft was open.

## Palette simplification · owner decision 2026-11-10

Direct tab navigation and tab enable/disable entries are hidden from Ctrl+K.
The palette keeps All ↔ previous tab and opening tab settings. Visibility belongs
to Settings → Tabs; direct dispatcher routes and saved shortcut overrides remain
compatible. This also applies to Text, Links and Tables.

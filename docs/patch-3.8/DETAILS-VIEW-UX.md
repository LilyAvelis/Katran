# Details View · UX proposal · 2026-11-17

Implementation follow-up: [Mini stream](DETAILS-VIEW.md). The proposal/status
below preserves the earlier research checkpoint.

Status: research and design contract, not an implemented or accepted feature. The reference is the owner's Raycast screenshot: a dense list at left, a large content area at right, and an information section below it. Katran retains its own visual language and existing actions.

## Geometry and behavior

- A third shelf presentation alongside Gallery and Stream: one compact, vertically scrollable row per object on the left, and the cursor object's preview and metadata on the right. Existing search, category, sorting, selection, action bar and overlay remain available; this is a view of the same results, not another collection.
- Initial split approximately 38% list / 62% detail, with independent scrolling and a visible divider. Keep rows single-line with type icon, title and existing relevant status/tag cues; truncate long titles with a full-name tooltip. The right pane uses remaining height, placing preview above a separately readable information block. At narrow widths avoid clipping or unusable panes: define a tested minimum width and provide a single-pane fallback with a clearly accessible detail toggle. Exact dimensions and resizing behavior need native acceptance, not screenshot-derived assumptions.
- The current cursor drives details; moving through the list changes the preview without requiring a selection. Multi-selection remains the existing command selection and does not silently follow the cursor. Clicking a row sets the same cursor/selection behavior as the existing shelf; show cursor and selected states distinctly.
- Support every shelf tab and provider through its existing event/preview interfaces, including Folder, Desktop, Pods and virtual catalogue items. Present useful metadata appropriate to the object: name, type, origin/source, applicable path/link, timestamps, size and tags when actually available. Omit unavailable fields; never fabricate them or expose internal identifiers by default. Full path or URL must be readable/copyable without accidental navigation.
- The current `Interaction.preview` is a full-screen mode that changes navigation; this persistent detail pane follows the cursor without turning that mode on or changing selection. Pin/tag values are separate projections. Virtual emoji and Folder IDs have different semantics; a sign check alone is not an identity or capability check.

## Keyboard and focus

- Up/Down navigate the visible result order through the existing interaction command; keep cursor visible. Preserve existing selection modifiers, copy, drag, delete, pin, tags, Actions, Enter and preview behavior, including provider-specific domain restrictions. Switching presentation does not clear selection, search, sorting or filter.
- The list owns navigation by default; explicit focus transfer to the right pane permits scrolling, selection and copying of text. While a text field or selectable detail has focus, ordinary editing keys belong to that field; shortcuts and Esc follow existing modal priority. Returning focus to the list retains the cursor. Do not claim a new shortcut until checked against the existing catalogue and approved.
- If the cursor disappears after filtering, deletion or refresh, use the existing interaction API's valid cursor policy; detail must clear or follow the new cursor rather than showing a stale object. Multi-select should make the detail's cursor target clear, not imply a batch preview.

## Loading, errors and large collections

- Render rows from metadata only. Fetch/decode a preview only for the current object, asynchronously where existing clients support it; immediately identify the selected object and show a loading state in the right pane. Late responses must be keyed to object ID, revision and current view context so rapid navigation cannot show stale content. Changing the cursor cancels or ignores obsolete work.
- Existing bounded read-only text/table workers and image workers are starting points, not permission to duplicate their queues. File visuals can be a thumbnail, application icon or generic icon; identify a fallback honestly rather than presenting an icon as a content preview.
- Links show the actual URL and recorded information, without inventing a site card, title or thumbnail. Reuse the existing opt-in favicon preference; opening the view must not fetch page content or create new network traffic. Video is not presented as playable unless a separately verified existing route supports it.
- Distinguish empty shelf from no search results, loading, unavailable/deleted/stale object, preview error and unsupported preview. In unsupported cases show the type, icon and available metadata with existing applicable actions; do not treat inability to decode as absence of the object. A failed preview must not block list navigation or unrelated actions.
- Virtualize visible rows and bound thumbnail/preview memory and queued work. Preserve pagination/cursor semantics and stable scroll as results grow; avoid listing, hashing or decoding an entire source solely for this view. Desktop remains metadata-only during browsing; validate bytes/revision only at the actual action boundary.
- Preview rendering and metadata calls must use existing dispatcher and provider contracts; virtual emoji IDs stay read-only and cannot enter destructive paths. Native acceptance should cover text, image, video, link, table, emoji, files and folders, managed Desktop objects, large results, rapid cursor changes, partial/unavailable sources, and keyboard focus.

## Open decisions

Confirm the final view label, switcher and shortcut; minimum width/fallback; whether divider resizing persists; and which metadata each existing provider can supply without expensive work. These are design decisions for owner review, not permission to invent new storage or bypass API First.

For the provider matrix, current worker limits, API routes and backward-compatibility implications, see [DETAILS-VIEW-RESEARCH.md](DETAILS-VIEW-RESEARCH.md).

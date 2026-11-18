# Mini stream metadata layout investigation · 2026-11-18

Owner reported a brief size/position jump in the right metadata panel when changing items or pinned state. Investigation only; no product fix was applied.

Synthetic headless probes used egui 0.36.2, a temporary profile, and synthetic items. Logs: `target/build/mini-stream-geometry-investigation.log` and `target/build/mini-stream-links-investigation.log`. The initial probe compared an unavailable preview to a full-height preview: metadata text y was 69, 69, 384, 384 across two frames each. This demonstrates content-dependent placement, not a transient on its own.

A second probe used two real, locally ingested synthetic Links, then changed pinned state and resized. On switching to the second link the metadata scroll content max rect began at y=45 on the first frame and y=181 on the following frame, with the same scroll child ID. The first link similarly moved y=45 to y=62. Metadata width for the second link stayed 496 px. Pinning on that settled link kept its y=181 in both measured frames while content height grew from 643 to 682; resizing gave stable y=215 across both measured frames. The test reproduced an asynchronous same-provider first/next-frame position jump, not the owner’s precise 50 ms duration, every provider, or a pin-only transient.

The source chain is `src/bin/details_view_ui.rs` (`details_panel`): the preview uses `allocate_ui_with_layout(width, preview_height)` and `set_max_height`, followed by the metadata ScrollArea. In egui 0.36, that allocation provides a maximum child rectangle but advances the parent by the child’s actual `min_rect`; the requested size is not a minimum. `src/bin/text_preview_ui.rs` renders a short header and an initially empty async body; once text arrives the preview child’s used height changes, moving the metadata below it. The metadata ScrollArea ID stayed constant in the probe, so a changing ID was not the cause in this case. Its default `auto_shrink=[true,true]` remains a separate content-dependent sizing risk; the probe does not establish that as the cause of this transient.

A narrowly scoped candidate fix is to reserve the preview slot’s full computed height immediately (e.g. establish an exact-size parent slot and render a clipped child inside it, or an explicitly verified minimum height), while retaining provider preview behavior and the split ratio. Consider fixing the metadata ScrollArea’s available width/height via `auto_shrink([false,false])` separately if measurements show a remaining size change; preserve intended scrollbar and wrapping behavior. Add a headless regression using the same temporary synthetic Links and current production rendering: measure preview slot and metadata position/viewport on first and subsequent frames across loading, cursor switches, pinned/unpinned and viewport sizes; check stable scroll identity, clipping, and actual async result transition. Owner’s real Windows acceptance remains necessary. No changes to sorting, data sources, playback, or domain storage are implicated by these probes.

## Authorized correction · 2026-11-18

After the investigation the owner requested implementation. details_panel now
reserves an exact preview rectangle in the parent, draws the unchanged provider
inside a clipped child, and lets metadata consume the remaining viewport with
auto_shrink([false,false]). The last setting prevents short field sets from
collapsing the metadata viewport; it is not presented as the original async-jump
cause. Normal ScrollArea scrolling/wrapping and stable salt are retained.

The regression uses a temporary profile and actual async loading of two synthetic
Links. It measures the reserved preview rectangle, metadata clip rectangle and
scroll child identity before and after loading, cursor changes, pin/unpin field
changes, unavailable-image transition and two widths (310/500). Geometry stays
identical at each width and both rectangles stay within the supplied panel.
Details5/5, text-preview11/11 and media-preview15/15 passed. Clippy UI/tests
-D warnings passed3.60s; fmt/diff checks passed. Raw logs in target/build:
mini-stream-layout-details.log, mini-stream-layout-text.log,
mini-stream-layout-media.log and mini-stream-layout-clippy.log.
Only UI layout changes; domain/native/media runtime evidence is reused.
Native appearance and every owner-specific pin-only path still require manual
acceptance; no claim to have reproduced the exact50ms duration or every provider.

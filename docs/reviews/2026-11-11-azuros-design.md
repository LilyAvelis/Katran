# Azuros design/lifecycle review and remediation

Review requested by owner, same Garden mask Azuros Claudapi, Sol/xhigh. Static immutable review of036a53d against81abc6f; no native desktop, Clipboard, personal files or Cargo. The owner withdrew the intermittent-navigation report: an open Settings window intentionally owns input. No separate unexplained focus bug was claimed fixed.

## New findings on036a53d

- P2: Sort was missing from early workspace/transfer and raw Tab ownership. Tab/ShiftTab/CtrlV could act behind the modal. Fixed with shared modal ownership, sort routing before early handlers and same-frame comma/Tab guard. Regression checks retain tab, files and Clipboard when modal owns input; arrows/Enter select and reveal the highlighted shared ActionRow.
- P2: native picker join and image decoding ran inside Settings render, stopping the main loop. Picker now returns a channel immediately; import uses a bounded foreground domain worker for prepare and an authorized commit. Pending-picker/capture, cancellation and no-precommit-write cases are synthetic.
- P2: JPEG EXIF orientation was discarded. The decoder orientation is applied before resize/PNG/hash; synthetic6/8 tests inspect dimensions and color corners.

## Control review3f10a7c

Azuros statically confirmed the original five findings and these three closed, but found two defects in the newly requested image Action: availability hashed original content during menu render, and admission checked revision before the worker reread a different buffer. Both fixed in6bcb2a7: metadata-only availability; shared prepare_event checks SHA256 of the exact decoded bytes for ref/blob and recorded metadata before/after for Folder. Azuros separately verified closure of these two boundaries. This is not a claim that every possible application bug is excluded.

## Original five findings from81abc6f

1. Every new Clipboard publication disarms an old cut; only an exact validated non-cut copy snapshot can remain staged. Sync/job text regression keeps oldA untouched after copyingB.
2. Sync and worker OpenWith share execute_selected; Link uses exactURL instead of cachedTXT. Fake handlers verify Unicode URLs.
3. Unicode text publication and sequence receipt share one native Clipboard guard; injected backend checks publish→sequence→unlock and failure cleanup.
4. Only consistent completed recycle receipts with absent staged content are excluded before recovery limits.1000 completed plus one unresolved remains available; uncertain receipts remain visible and no journals are deleted.
5. Portable/setup include ES and notices. The update ZIP remains the exact old three-file contract so2.0 clients can receive3.0 directly. The application embeds ES for a verified content-addressed fallback when no adjacent helper exists. Synthetic updater→resolver roundtrip does not launch the helper. Required asset notices are included in the authenticated combined notices file.

## Evidence

112 headless UI cases passed; focused text publication/recycle/image revision cases passed; cut receipts, Link handlers, theme/action/cancellation and old updater checks passed. Related direct/folder transfer, text/table/smart-copy/overlay regressions passed. A synthetic portable/update package passed helper/hash/notices/three-file whitelist checks; fixture executables were placeholders, never described as real distribution artifacts. All-target clippy passed. Native paste/dialog/visual behavior remains owner acceptance. Everything requires the separately running Everything index; the embedded helper does not install it.

Subsequent owner clarification adds explicit ShiftTab move versus CtrlTab copy, including repeated Folder roundtrips. That separate change receives its own checkpoint and API evidence; it was not part of the immutable snapshots above. Publication remains unauthorized in this task.


## Owner's dual transfer clarification and guard review

Implemented inae9b03b: explicit ShiftTab move and CtrlTab copy in both directions;
copy preserves view/selection, image return imports create independent blobs, and
legacy external references can move only through the direct command. Clipboard
Cut keeps its prior original non-deletion rule. Batch2×3-folder cycles, repeated
copy, pinned sources and managed-root rejection passed synthetic Windows tests.

Azuros found one new P2 in that new reference-move path: a supported legacy alias
inside managed storage could otherwise delete a shared owned blob. Fixed inc423be3
with canonical source containment before authorization and before disposition,
plus the held handle's actual path check. The regression seeds a legacy alias of
a pinned owned blob, requires move to fail and copy to succeed, and verifies the
owned file/event remain intact. Azuros statically confirmed this finding closed.

Latest evidence for transfer:113 UI tests;10 direct transfer API cases; existing
Folder import/copy, work-folder, shortcut, clipboard-cut and universal-file gates;
all-target clippy. Synthetic Trash is used for temporary test files: no personal
originals or Windows Recycle Bin were exercised. Native transfer/paste UX remains
owner acceptance. The source-only checkpoints are local and no publication occurred.

# Notion browser paste incident · 2026-11-17

Owner feedback after activating `fb5e69b83a48`: Ctrl+Shift+Slash works; Enter
still fails for files in Notion **in a browser**, while the same file copied in
Katran with Ctrl+C and manually pasted into the same editor succeeds. This is
evidence that this tested payload is accepted by that editor; it does not
establish every file type or recipient route.

The retained local stage logs identify an earlier failure boundary: five
single-image file attempts published successfully (CF_HDROP, CF_DIBV5, copy
effect, owner marker; no optional-pixel warning) then failed `clipboard_changed`
before SendInput. Publication → delivery sequences were 608→610, 615→617,
622→624, 629→631 and 636→638. No file names, paths, image bytes or document
content were needed to establish this. A bounded stage-only snapshot is retained
under ignored `target/build/notion-paste-incident.json`.

`publish_file_formats` read its receipt sequence before dropping the open
Clipboard guard and temporary owner HWND. That receipt could be stale after
publication teardown. The observed +2 is not a permitted fixed adjustment or
proof of the exact producer of each increment. Windows documents sequence
changes and removal of Clipboard ownership during window destruction, not a
constant delta to add. Existing text publication already has a separate narrow
owned-text verification; it must not be applied blindly to arbitrary file data.

The bounded correction is to finish publication teardown, verify the exact
just-published file payload under a read lock and record only its stable current
sequence. Delivery retains its existing strict sequence/focus/identity guards.
No late general file fallback, extra formats, arbitrary delay or bypass is
authorized by this evidence. A failed post-publication proof must preserve the
already-published export paths for manual paste while withholding Enter's
authorization (sequence=0 plus bounded diagnostic status).

Primary contracts:
[sequence number](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber),
[DestroyWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-destroywindow).

## Implemented correction

The file backend now closes Clipboard and destroys its owner before settling a
receipt. A bounded read-only lock verifies NULL owner without a native error,
the exact fresh marker, HDROP and actually published DIBV5/effect bytes. Allowed
formats are the published set plus recognized system bitmap syntheses; unexpected
formats reject proof. The nonzero sequence must match before/after the reads and
again after closing the read guard. The delivery boundary is unchanged and still
rejects a subsequent sequence change. No fixed delta, delay or late-file fallback.

Failed proof returns `sequence=0` and a static `sequence_verification` category.
The successful primary publication remains successful copy: both the current and
previous export batches are preserved for manual paste; automatic input is not
authorized. Human-readable warnings may contain native errors and are not added
to bounded diagnostics.

Synthetic `copy_out` tests passed 13/13, including post-teardown changed sequence,
foreign replacement, extra format, unstable sequence and export retention. The
dispatcher gate includes zero-sequence successful copy → `copy_unverified` with
zero input sends. All-targets Clippy passed. Azuros's one bounded review found no
blocking proof/resource/retention issue. No native Clipboard test was run against
the owner's personal data; the exact producer of +2 and Notion acceptance still
require the next owner attempt.

## Installed artifact and owner confirmation

Source `82619cb`, one optimized UI build 1m26s, matched compiled-resource hashes.
Primary runtime `981c10a95b8a`, SHA256
`981c10a95b8a9d74f4dcda749dd50a4ddac0506d185d0f0b4580f4c36f5e246d`.
Previous `fb5e69b83a48` and an integrity-checked schema-v13 backup are retained
through private `runtime/rollback-paste-sequence.json`. No model/settings migration,
process closure or GitHub publication. The owner restarted the app herself.

The owner subsequently confirmed: the same file now inserts through Enter in
the same Notion browser editor. New-session stage logs show a single image with
`sequence_verification=verified`, publication/delivery 687→687 and successful
input without a tracking warning. Another four-file batch recorded verified
691→691 and successful input; that log alone is not a separate recipient ack.
The bounded stage-only confirmation is in ignored
`target/build/notion-paste-confirmation.json`.

This closes the reported same-file browser-Notion failure. It confirms the stale
receipt/automatic-delivery boundary and the working correction, without claiming
which Windows internal operation produced each old +2 increment or universal
support for all file types/editors.

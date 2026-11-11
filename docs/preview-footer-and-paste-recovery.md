# Preview footer and text paste recovery ·2026-11-11

Preview keyboard/pointer hints are presentation-only and now use the free center
of the34-point footer. They are removed from image/Pods/document canvases;
Pods Unicode metadata remains below the emoji. Hint text adapts to content and
image zoom, clips/ellipsizes in the center segment, and offers the full tooltip.

The owner's local journal contained BeginPastePrevious/CompletePastePrevious
followed by clipboard_changed. Enter was routed to paste; the final native
sequence guard rejected it. The actor responsible for the revision change was
not established. Windows sequence changes can include delayed format rendering:
https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber

For the latest own native Unicode publication only, a sequence mismatch may now
be revalidated against a private in-memory UTF-16 digest and length. It requires
an ownerless clipboard, only native plain-text formats1/7/13/16, valid bounded
memory and NUL termination, and a stable nonzero revision during verification.
The sequence and target identity/foreground are checked again before SendInput.
Different text, rich/custom formats, missing/older receipt, a live owner or any
native error remains a rejection. No automatic replay, clipboard replacement,
normalization of emoji or fallback for file/table publication is introduced.
The live-owner restriction deliberately avoids requesting delayed foreign data.

The logger records paste_delivery with expected/observed revisions and error
code, never clipboard contents. Native receiver acceptance remains an owner
check; this change does not prove why an external revision happened or promise
all future paste attempts succeed.

Synthetic checks: publication ordering; exact emoji/combining UTF-16 with a new
revision; different text, rich/custom formats, old/missing receipt and mid-check
revision changes; overlay and Pods transfer API regressions. UI116 tests passed,
plus a center/clipping footer test added afterward. UI/lib Clippy passed. No
real clipboard was replaced for these tests.

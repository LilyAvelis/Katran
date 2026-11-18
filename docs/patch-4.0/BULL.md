# BULL local v1 candidate

A BULL file contains an ordered sequence of PNG pages. The reader requires
only the file. The format does not store source paths, translation metadata,
or Katran catalogue state.

| Offset or sequence | Representation |
| --- | --- |
| Header | Eight ASCII bytes `BALOHCOL` |
| Version | Unsigned 32-bit little-endian integer `1` |
| Count | Unsigned 32-bit little-endian integer, 1–32 |
| Each page | Unsigned 32-bit little-endian byte length, 32 raw SHA-256 bytes, PNG bytestream |

The file budget is 256 MiB including framing, with up to 64 MiB per PNG.
The Rust codec rejects invalid framing, extra bytes, incorrect hashes and
invalid PNG chunk structure. Image viewers apply their own decoded dimension
and allocation limits. SHA-256 checks integrity and does not authenticate the
author.

Katran saves a translated sequence as one collection under Translate.
The slideshow walks successful outputs in input order. Failed input
positions and errors remain in the current job receipt. Cancellation
before publication leaves the originals and shelf intact. The first local
candidate uses shelf display order only to interpret the Shift gesture.
The gesture's direction, or Ctrl click order, determines the saved sequence.

Ctrl+K shows **Pack images into BULL** for a selection of 1–32 images and
**Unpack BULL** for one saved collection. Both use the same rebindable action.
Packing creates a new card in Images, or in Translate when packing saved
translations there. It writes separate PNG pages in selection order and
preserves the originals. Collections start unpinned and can be pinned or
tagged. Unpacking puts PNG cards on the collection's shelf and keeps the
BULL itself. Repeating the operation reuses existing extracted cards from
that collection, recreating only missing pages. The default date ordering
shows a freshly extracted sequence in stored page order. The same file
works with the existing QuickLook adapter. This version supports images;
Folder authoring and heterogeneous entries are subsequent work.

The Windows QuickLook adapter lives in `integrations/quicklook`. Katran exports
a named snapshot to the external viewer. Its internal page viewer can reopen
the same collection without an external app.
Katran marks collection cards with a BK badge. When paging, the preview
retains the last ready page in a fixed viewport until the requested page
arrives. Page changes do not flash a loading label over the ready image.
Wheel zoom anchors at the pointer, Ctrl+wheel at the center, and
dragging pans the enlarged page. A successfully loaded new page starts fitted.
The external QuickLook viewer also uses a clipped viewport, retains the last
ready page and supports anchored zoom. Its updated plugin was built and
checked with the bounded parser fixtures and WPF image decoding.
The local .NET Framework 4.6.2 adapter was built against the installed
QuickLook 4.4 bridge and placed in its user plugin directory. It loads on
QuickLook startup. The synthetic reader checks cover valid two-page data,
invalid SHA-256, truncation, extra data and dimension/count limits. A sample
PNG decoded through WPF. Native interaction is a separate acceptance check.

The local batch check translated the selected range from bottom to top
with an invalid PNG between two valid pages. One collection held both
successful pages in the requested order after reopening the profile, and the
job receipt kept the original position of the failed page. The API test
also verified the named PNG QuickLook snapshot using a test launcher.

# BULL local formats

An image BULL v1 contains an ordered sequence of PNG pages. The reader requires
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

## Links v2

Typed Links files start with `BALOHCOL`, unsigned 32-bit little-endian
version `2`, four ASCII bytes `LINK`, and an unsigned 32-bit little-endian
count of 1–32. Each entry has a 32-bit little-endian payload length followed
by a 32-byte SHA-256 digest and the payload. The payload contains four fields
in order: exact HTTP(S) URL, title, description, and optional PNG preview.
Each field starts with a 32-bit little-endian byte length. The first three
are UTF-8 with respective limits of 8 KiB, 4 KiB and 16 KiB. The preview is
at most 2 MiB, with length zero representing an absent image. Empty title
and description represent absent metadata. The file budget is 256 MiB.

Packing selected Links writes the URLs and any custom card names into the
portable file. Custom names take precedence as the displayed title. Existing
local collections can recover those names from their original shelf records
when viewed or merged. A separate job fills missing titles and descriptions
for public HTTPS pages and saves them if the collection revision still
matches. Saved previews stay in the file. Link enrichment makes no preview
image requests. Viewing
uses the saved bytes locally. Ctrl+C copies only
the visible URL and bypasses clipboard capture. Alt+Enter opens that URL in
the default browser. Copy the shelf card to send the whole BULL. The external
QuickLook adapter currently opens v1 image BULLs. Katran also browses Links
and Text collections.

**Rename link** changes the current title in a Links BULL on the shelf.
The original member card, when present, uses the same name after unpacking.
An earlier exported copy retains the bytes it was exported with.

The background HTML deadline is 7 seconds, independent of packing. Merge
reuses stored metadata and may enrich entries with a blank title or description.
A copy made before enrichment remains a valid independent snapshot. A later
copy includes the metadata applied by then. Unpack or deletion takes precedence
over a pending replacement. Incomplete enrichment leaves the URLs usable.

## Text v2

Text uses the v2 header with four ASCII bytes `TEXT` as its type. Each record
starts with the little-endian 32-bit byte length and 32-byte SHA-256 digest,
followed by 1 byte to 1 MiB of exact UTF-8 source without embedded NUL.
Whitespace and line endings survive unchanged. Katran can render Markdown or
show the source, and Ctrl+C copies the exact current source in either mode.

## Shared shelf behavior

An image BULL is an ordered selection for Upscale and Translate. The same
rebindable commands also accept loose images. A BULL input always yields
a separate BULL, even with one page. Upscale saves to Images and Translate
saves to Translate. Successful pages retain their order. A failed page is
reported by original index and is omitted from the output. Cancelling
before publication leaves the source collection and shelf intact.
Image BULLs in Folder can also use the lossless file framing with the
`images` family. Both framings use the image jobs without unpacking the
collection into the shelf or the source folder.

Katran saves a translated sequence as one collection under Translate.
The slideshow walks successful outputs in input order. Failed input
positions and errors remain in the current job receipt. Cancellation
before publication leaves the originals and shelf intact. The first local
candidate uses shelf display order only to interpret the Shift gesture.
The gesture's direction, or Ctrl click order, determines the saved sequence.

The same rebindable Ctrl+K action packs selected images, merges two or more
selected BULLs, or unpacks one BULL on Images, Translate, Links or Text. Pack and merge
replace the selected cards with one collection. The new card appears first
in newest-first arrival order, and its cover is the first page in the saved
selection order. A merge concatenates the pages of the selected BULLs and
keeps each one's internal page order. Collections merge with the same entry
type. The combined limit is 32 entries.

A collection is pinned if at least one selected card was pinned. The
collection receives a tag only if all selected cards have the same assigned
tags in that shelf. Original cards keep their own pins and tags while hidden.
Unpacking replaces the BULL with those same cards, including after a merge.
Changing a collection's tags affects that card. Legacy and translation
collections can contain pages without original shelf records. Unpacking
those pages adds PNG cards from the portable BULL. Exported BULLs contain
ordered pages without Katran pin or tag metadata. Images, Links and Text have
separate collection types. Delete removes the shelf card and its member
records, while Unpack restores the original shelf cards. Folder pack and
merge recycle the selected files after verifying the new archive. Folder
unpack restores entries directly into the current folder and recycles the
archive after verifying every output.

Raw file BULL uses `BALOHCOL`, little-endian version 2 and the `FILE` tag.
A byte gives the length of the ASCII family label (`images`, `video`, `audio` or
`ext-<suffix>`), followed by the label and a complete BS v1 payload. Readers
validate every entry's name against the declared family. The payload
preserves file names and bytes. See [BS v1](BS.md).

Enter inside the viewer pastes only the currently displayed entry to the
previous window using Katran's standard verified clipboard flow. Copy or drag
the shelf card to transfer the collection as one file.

The Windows QuickLook adapter lives in `integrations/quicklook`. Katran exports
a named snapshot to the external viewer. Its internal page viewer can reopen
the same collection without an external app.
Ctrl+C in Katran's page viewer copies the single visible image at its stored
resolution. The clipboard includes Katran's ownership marker, so capture does
not add another card. Use Copy on the shelf card to transfer the whole BULL.
Katran marks collection cards with a BK badge. The internal viewer shows the
collection name above centered arrows and a page counter. Space or the
top-right cross closes it. When paging, the preview
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

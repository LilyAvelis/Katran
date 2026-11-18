# BS v1

BS stores an ordered sequence of independent files. File names and bytes
travel together. The format stores no source paths or Katran catalogue state.

| Field | Representation |
| --- | --- |
| Magic | `BALOHBS` followed by one NUL byte |
| Version | Unsigned 32-bit little-endian integer `1` |
| File count | Unsigned 32-bit little-endian integer, 1–1,024 |
| Each record | Name length and data length as unsigned 32-bit little-endian integers, 32 SHA-256 bytes, UTF-8 name, exact file data |

The record digest covers both encoded lengths, the name bytes, and the file
data in that order. BS supports 128 MiB per file and 256 MiB including framing.
Empty files and repeated names are valid. Names are Windows-safe single leaf
names up to 255 UTF-8 bytes. Readers check bounds, integrity and end of file.

Files and Folder use **Pack / unpack collection**, the same rebindable Ctrl+K
action used by BULL. Supported images form an image BULL. Video, audio and other
files with a common extension form a file BULL. Mixed files form a BS.
Merge accepts collections of the same kind and family, flattens their
entries and preserves selection order.

On the shelf, pack and merge replace selected cards. Unpack restores their
original cards and metadata, using archived bytes for changed or unavailable
references. Delete removes the collection and its hidden member records
without removing referenced disk files.

Space opens one file at a time with a centered thumbnail or Windows file icon,
name, type and size. Alt+Enter materializes the selected entry in the bounded
open cache for Windows. Ordinary data files use their registered application,
other files can use the application picker, and executable or shortcut types
are shown in Explorer. Copy or drag the shelf card to transfer the whole BS.
A `.bs` file added to Files from disk can be viewed in Katran.

Folder packs or merges into a verified new archive, then recycles the
selected files. Loose files, including images, retain their names and exact
bytes. A failed or partially completed recycle reports each affected file
and its recovery location when available. Unpack restores files directly
into Folder. It chooses a free name when a file already exists and recycles
the archive only after every output has been verified. Partial publication
reports the files already restored and keeps the archive.

Ctrl+E renames managed BULL and BS cards without requiring a pin. The name
travels with the exported collection. In Folder, Ctrl+E renames the archive
on disk and preserves its suffix.

Folder and Files display the first entry's thumbnail or file-type icon.
Video entries in file BULL and BS open directly in Katran's shared player.
The viewer plays the current video page; changing pages stops the old clip.
The viewer centers the collection name and page controls. Space or the top-right
close button dismisses it, and Alt+Enter opens the current entry.
The first page prepares during the 300 ms entrance. The player opens once the
entrance ends. During a video page turn the viewer keeps the last frame until
the next page is ready to display.

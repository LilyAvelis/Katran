# BS v1

BS stores an ordered sequence of independent files. File names and bytes
travel together. The format stores no source paths or Katran catalogue state.

| Field | Representation |
| --- | --- |
| Magic | `BALOHBS` followed by one NUL byte |
| Version | Unsigned 32-bit little-endian integer `1` |
| File count | Unsigned 32-bit little-endian integer, 1–32 |
| Each record | Name length and data length as unsigned 32-bit little-endian integers, 32 SHA-256 bytes, UTF-8 name, exact file data |

The record digest covers both encoded lengths, the name bytes, and the file
data in that order. BS supports 128 MiB per file and 256 MiB including framing.
Empty files and repeated names are valid. Names are Windows-safe single leaf
names up to 255 UTF-8 bytes. Readers check bounds, integrity and end of file.

In Files, **Pack files into BS**, **Merge BS collections**, and **Unpack BS**
share one rebindable Ctrl+K action. Pack and merge replace selected shelf
cards in the chosen order. BS files merge with BS files. BS and BULL cannot
share a selection or be nested through this action. Unpack restores the
original shelf cards and metadata. For changed or unavailable disk files,
Katran restores the archived bytes into Files while preserving the disk file.

Space opens one file at a time with its name, type, size and optional image
thumbnail. Open or Alt+Enter materializes the selected entry in the bounded
open cache for Windows. Ordinary data files use their registered application,
other files can use the application picker, and executable or shortcut types
are shown in Explorer. Copy or drag the shelf card to transfer the whole BS.
A `.bs` file added to Files from disk can be viewed in Katran.

Folder authoring is a separate step. Existing BULL image classification groups
supported image formats, including JPEG, PNG and WebP, into one image type.

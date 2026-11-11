# Emoji fonts

General settings offers Apple (the default preference), Windows Segoe UI Emoji,
and profile-local fonts imported through the native file picker. Save settings
to apply the selection; no application restart or Windows font installation is
needed. The choice affects the color Pods tiles and their Quick Look preview.
Clipboard content remains Unicode and the receiving application controls its
own appearance. Country flags retain the existing bundled SVG fallback.

`EmojiFonts`, `ResolveEmojiFont`, `ImportEmojiFont` and `Settings.emoji_font` are
the domain API. Import also supports BeginJob/PollJob: bounded native font
validation and copying run outside the GUI. Imported fonts are copied into
`emoji-fonts/<sha256>.ttf` in the active data profile; the source is unchanged.
Selection does not change merely by importing. Apple uses `emoji-fonts/apple.ttf`.
Missing Apple assets are reported explicitly and fall back to Windows rendering.
Font switching invalidates the color texture cache and discards previous jobs.

Private DirectWrite collections load these files only in Katran. They do not
replace or register fonts in Windows. Font files are limited to384MiB. IDs are
restricted to known presets or lowercase SHA256; profile reparse points are
rejected. Adding an arbitrary valid font does not guarantee that it contains
color emoji or supports every sequence.

## Local owner setup

At the owner's explicit request, the macOS26 Windows conversion was downloaded
from [samuelngs/apple-emoji-ttf](https://github.com/samuelngs/apple-emoji-ttf),
release `macos-26-20260722-484daf4e`. SHA256:
`18e48f1785564fbf511241e0963b265057bfe742036d8543406c6ce07e48ec0b`.
The file is256391076bytes. Its first family is named Segoe UI Emoji, so the
private collection is essential to distinguish it from Windows' own font.
Apple designs remain Apple's assets; the converter's code license is not a
grant to redistribute those designs. This local file and its source receipt
remain in ignored profile data, outside Git. Version 3.1 release builds include
a deterministic compact resource retaining all glyphs at the 96px bitmap strike;
larger previews scale that strike. Other bitmap sizes are omitted to preserve the
legacy updater's 100MiB archive limit. The resource is materialized privately on
first startup without replacing a user's existing apple.ttf. Source and resource
hashes are recorded in the build manifest. The converter's license does not
establish redistribution rights for Apple's artwork.

## Verification

API tests cover migration/defaults, persistence, unavailable fonts, path-ID
rejection, private import, unchanged sources, deduplication and asynchronous
invalid-font failures. Headless UI tests cover cache invalidation during an
in-flight render. Native rendering tests cover bounds and color sequences.
The opt-in `locally_selected_private_font` test accepts an explicit
`KATRAN_EMOJI_FONT_TEST` path, compares Apple and Windows output and writes a
contact sheet under target; it never accesses the desktop or clipboard.

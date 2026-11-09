# Katran application identity · 1.0.4

- `katran-logo.png`: generated square master, RGBA with transparent corners.
- `katran-icon.png`: 256px window/taskbar texture, embedded into the GUI binary.
- `katran.ico`: 16/20/24/32/40/48/64/96/128/256px Windows icon resources.
- `katran.rc`: icon and file/product metadata template. `build.rs` fills version
  from Cargo and compiles it only for `katran-ui` on Windows when UI is enabled.

Optional authoring export: `python scripts/export-icon.py` (Pillow). Builds use
the committed PNG/ICO and need no Python. Windows UI builds require the Windows
SDK resource compiler; missing resources fail the build rather than silently
shipping a generic executable icon. Non-Windows and CLI-only builds skip RC.

Before creating the window, Katran sets its stable AppUserModelID to
`LiliaAvelis.Katran`. The window gets the same image through the viewport builder.
No installer, registry edits, file associations, Start Menu entries or desktop
shortcuts are created. This remains a portable desktop executable; an installer
can be added separately. The owner verifies the actual taskbar/Alt+Tab appearance.

References: [Microsoft application IDs](https://learn.microsoft.com/en-us/windows/win32/shell/appids),
[resource compilation](https://docs.rs/embed-resource/3.0.12/embed_resource/fn.compile_for.html).

## Generation record

Generated with the built-in image generation tool. No API/CLI fallback used.
PNG/ICO variants are mechanical resizes/format exports of the same artwork.

Final prompt:

> Use case: logo-brand. Create ONE finished square Windows desktop application icon for Katran, an elegant local image shelf and media utility. Subject: a compact graceful dogfish shark (katran), expressed as a bold sculptural abstract silhouette with a distinct dorsal fin and sweeping tail, subtly echoing a letter K through negative space. Premium restrained desktop utility icon, confident and warm, friendly but not cartoon mascot. Very clean vector-like shapes with subtle depth, high silhouette legibility at 24px. Centered single ivory-white shark symbol on a rich deep petrol/teal rounded-square tile, with a small seafoam highlight on the lower edge. The tile nearly fills the square canvas with a little transparent outside margin, softened corners and understated bevel, no cast shadow outside. Do not add text, letters, wordmark, border ornaments, extra fish, interface, mockup, scenery or watermark. Straight-on flat app-icon composition, square 1:1, crisp edges. Deliver just the app icon, transparent background outside the rounded tile.

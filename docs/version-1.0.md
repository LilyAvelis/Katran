# 1.0 · gallery layout

Gallery tiles now follow Afio gallery_view.rs/layout.rs: allocate fixed cells,
fit the natural image into its own rectangle, and paint through a clipped painter.
The image, filename and origin badge have disjoint regions. Names and sources
truncate to one line with ellipsis and a full hover description. A loaded image
and a pending/error placeholder reserve identical geometry. Portrait/wide images
remain contained and centered. Default card width is 280 logical pixels; existing
saved preferences are respected. Settings still controls card width.

`gallery_card.rs` owns the shared geometry, column count, fitting and clipped text.
The shell uses the same column formula as the renderer for arrow navigation.
Virtual row height is based on actual tile width and includes the two text lines.
The stream also uses fixed clipped regions to prevent layout expansion.

Source metadata is the existing API origin field, shown in both gallery and
stream. Unknown historical sources remain unknown; no guessed attribution.
Selection, copy, delete and navigation continue through the unchanged API.

Validation: full synthetic/headless suite, including portrait/square/wide fitting,
multiple viewport widths, long Cyrillic names/domains at 100/150/200% scaling,
keyboard and background-job geometry regressions. Native appearance and physical
Windows interaction remain for owner verification. No desktop control was used.

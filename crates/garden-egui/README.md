# garden-egui

Small host-styled egui 0.36.2 presentation primitives extracted from Katran:
`ActionRow`, `ActionPresentation`, and measured `KeyCaps`. They draw and return
an egui `Response`; the host owns execution, selection, input, fonts, colors,
theme policy, and persistent state. No images or artwork are bundled.

Construct `ActionStyle` and `KeyCapsStyle` explicitly for each host theme. Supply
an `egui_lucide::Lucide` icon or override it with an existing `TextureId` using
`with_texture`. Lucide icons require the host to install a compatible egui image
loader; textures are managed by the host. Provide a primary shortcut for compact
keycaps and the complete shortcut text for the hover tooltip. `primary_shortcut`
selects the first alternative in a slash-separated string.

`ActionRow::regions` and `shortcut_budget` expose the same measured geometry used
for drawing. Labels and keycaps preserve the enclosing UI's clip rectangle, so
scroll containers remain authoritative. Oversized bindings are laid out in a
single bounded keycap. An `egui::Context` is used only through the supplied `Ui`;
the crate does not install fonts, alter styles, or perform platform effects.

Internal 0.1 crate; Rust 2024. Adapted from Katran under the accompanying MIT
license; Lucide SVGs are supplied by the separately licensed egui-lucide crate.

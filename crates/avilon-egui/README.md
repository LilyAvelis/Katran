# avilon-egui

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

`settings_layout` provides shared spacing tokens, responsive `SettingsRow` and
`SettingsControl`, and `SettingsSection` with an explicit host-supplied style.
Rows stack controls below descriptions in narrow windows. The library never
reads theme preferences. Run the neutral headless host with
`cargo run --offline -p avilon-egui --example design`.

Enable the optional `glow` feature for `optical_gpu`, a live OpenGL refraction
callback for the egui_glow renderer. Provide an application-owned egui texture,
its screen rectangle and UV rectangle, then insert the returned shape in the
painter before foreground content:

```rust
use avilon_egui::optical_gpu::{self, Material};
# fn paint(ui: &egui::Ui, texture: egui::TextureId, rect: egui::Rect) {
let shape = optical_gpu::shape(rect, texture, rect, egui::Rect::from_min_max(
    egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), 8.0, 0.5,
    Material { adjustments: [1.0; 4], selection_tint: None });
ui.painter().add(shape);
# }
```

The host retains and manages the source texture. This samples only that texture;
it does not capture the desktop or framebuffer, and cannot refract arbitrary
windows behind the application. The renderer caches GL objects per context and
render thread. Call `optical_gpu::destroy(gl)` on that thread with each matching
context current before destroying it; a failed shader compilation is cached until
that call. The host owns renderer lifecycle and error handling.

`theme::color` resolves semantic roles without storing preferences or changing egui
styles. For example, `color(Role::Surface, legacy, PaletteOptions { blue: true,
..Default::default() })` selects the blue palette; the host supplies its own
accent, selection, background and active-glass state. Standard mode retains the
supplied legacy color except when a selection override is explicitly supplied.

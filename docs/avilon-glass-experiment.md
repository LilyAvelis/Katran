# Avilon optical glass experiment

Status: local experiment, not part of Katran 3.0 release tags.

The three intended tiers are:

1. Windows Desktop Acrylic, already available in Katran.
2. GPU optical rendering over application-owned background pixels: refraction,
   bounded blur, directional edge light and optional chromatic dispersion.
3. Optical rendering over pixels behind the window. Obtaining that source,
   avoiding capture feedback, focus/monitor/DPI transitions and frame latency
   require a separate Windows integration experiment. It is not implemented.

The present prototype is a CPU **reference** for tier2, not its production
renderer. `avilon_egui::optics::render_lens` takes a caller-supplied ColorImage
and bounded options and returns pixels. It does not capture the desktop, use
Windows APIs, access files, or create a GPU device. Output is capped at512×512.
The Katran lab computes a512×288 preview only on Apply/Reset; other frames reuse
a texture. It compares an original lily against the refracted result. The lab
uses owner artwork only in Katran; the library has no bundled imagery.

`katran-ui.exe --glass-lab` opens an isolated lab App before the normal shelf,
profile, capture, tray and hotkeys initialize. `--glass-lab-render` exports a
single `target/avilon-glass-lab.png` without creating a native window. Default
Katran launch is unchanged. The experiment does not change the installed pointer.

Tests exercise zero-effect identity, unchanged background outside the rounded
lens, a synthetic gradient's refraction, sampling boundaries and invalid input
budgets. Native appearance and subjective comparison remain owner checks.

Next: port the same math to the egui Glow GPU callback/shader path, compare it
against the reference on synthetic images, then measure GPU frame time at actual
window sizes. Renderer capability and capture availability should govern future
mode selection. No full Apple Liquid Glass equivalence, desktop refraction or
GPU performance claim has been established by this preview.

Sources: [Microsoft Acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic),
[Composition brushes](https://learn.microsoft.com/en-us/windows/apps/develop/composition/composition-brushes),
[egui custom rendering](https://github.com/emilk/egui/blob/main/examples/custom_3d_glow/src/main.rs).

## Element GPU prototype

The current experimental branch additionally has an opt-in OpenGL330 callback
in Katran: gallery card surfaces and Actions/Sort frames sample the selected
lily/custom background beneath their text and icons. This does not sample other
cards, text, other windows, or the desktop. Shader/VAO resources are cached per
GL context and explicitly destroyed through App::on_exit while GL is current.
The callback restores touched program/VAO/texture state; missing texture or shader
failure leaves the existing normal fill intact. GPU frame timing and live GLSL
compilation still require native owner acceptance; headless Rust tests do not
establish them.

Blue theme exposes optical_strength0..100 through the existing SetTheme API;
zero is the default. Invalid values fail without saving, and a valid value
survives reopening. The in-app slider applies without an Acrylic restart.
Acrylic and element optics are independent. A supplied background is required.

`--optical-demo` opens the ordinary Katran UI with a separate
runtime/optical-demo-data profile, six synthetic PNG cards, the first lily and
strength65 on first run. Capture and Downloads discovery are disabled. It does
not read or write the original profile. Use Themes → Оптика элементов · GPU and
Comma/Ctrl+K to compare strengths and menus. Demo settings are retained between
runs. Do not use this demo profile as the production launcher/data directory.

Verification: theme API9 tests; existing113 UI/headless tests; additional callback
emission gate checks opt-in + background + GPU-host availability and Standard
fallback. UI-bin Clippy passed. The GPU image itself is not tested by those checks.

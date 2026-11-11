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

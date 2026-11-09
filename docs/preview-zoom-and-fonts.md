# Preview zoom and font fallback · 1.0.3

## Interaction

Large preview uses the same anchor math as Afio. Plain wheel zooms toward the
pointer; Ctrl+wheel zooms around the canvas center. Starting with 1.0.6, ordinary
primary-button drag grabs/pans an enlarged image, with grab/grabbing cursors.
Export is disabled above fitted scale (1×), including Ctrl+primary drag.
At fitted scale primary drag still exports. Middle-button panning remains available.
Gesture intent is decided independently of API/texture availability, and a pan
cannot become native export while a background capture job owns the API.
Gallery card drag-out is unchanged. Space/Escape return to the shelf;
arrows change images. Image changes and
preview close/reopen reset zoom and offset. Window sizing is unchanged.

The fit scale is 1×, maximum is 16×. Panning keeps image edges covering the canvas
where the enlarged image is larger; smaller axes remain centered. Edge constraints
can limit exact pointer anchoring. Rendering clips to the canvas, so image pixels
cannot cover the title, hints or footer. The existing 1400-pixel preview and
bounded worker/GPU caches remain in effect; zoom does not request unlimited
source textures or increase resolution beyond the cached preview.

## Rust API

`preview_view::State` owns factor/offset. `Interaction.preview_view` exposes it.
The one dispatcher accepts canvas-local logical coordinates:

- `preview_geometry {viewport:{width,height},fitted:{width,height}}`: constrain and
  return `factor`, `offset`, `left`, `top`, `width`, `height` for painting.
- `zoom_preview {multiplier,anchor:{x,y},viewport,fitted}`: anchored multiplicative
  scale with bounds, returning updated Interaction.
- `pan_preview {delta:{x,y},viewport,fitted}`: constrain a translation, returning
  updated Interaction.

All require an open preview. Nonfinite coordinates, invalid image/canvas sizes
and nonpositive multipliers return `invalid_argument` without changing state.
The GUI supplies input and paints the returned rectangle; it does not define a
separate zoom algorithm. No original files are changed by viewing.

## Text fallback

`src/bin/ui_fonts.rs` adds Windows font files as fallback to both proportional
and monospace text, following Afio's explicit font-registration pattern. Built-in
faces stay first to preserve current typography. Segoe UI/Symbol/Emoji and
available Chinese/Korean faces supply glyphs missing from those faces. Missing
optional files are skipped. Definitions and font bytes are cached once per
process. Fonts are read from `%WINDIR%/Fonts`; they are neither installed nor
copied into this repository. SVG action icons do not require a Nerd Font.

The headless glyph regression checks the real rendered atlas entries for arrows,
modifier/key symbols, Cyrillic and a sample emoji against the missing-glyph
entry, in both text families. API tests check pointer/center anchoring, scale/pan
bounds, invalid inputs and reset behavior. The owner performs native visual checks.

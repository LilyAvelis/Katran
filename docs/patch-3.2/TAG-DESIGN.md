# 3.2 tag design handoff

Iris Vale → Fenna / Polly, 2026-11-12. Implementable design specification;
native appearance is not accepted yet. This ticket owns this document only.

## Owner decisions and domain boundary

- Nine named slots, one active tag per object. New assignment/replacement is
  allowed only while the object is pinned. Owner correction: unpin removes
  the object-to-tag assignment entirely. The nine slot definitions retain
  their names, colors and materials. Repin does not restore the old assignment.
  This supersedes the earlier preserved-assignment interpretation.
- Pinned has no menu. `.` toggles pinned-only for the current category; the
  existing double category-number press remains an alternate route.
- Only tags get an Actions-style menu. Footer tag icon sits beside Sort;
  icon and keyboard dispatch to exactly the same menu state.
- Color and procedural texture only. No raster presets, image importer or
  material timeline. Tab-strip work is a separate ticket. Files is visible by
  default; existing-profile visibility migration remains the lead's contract.
- Dispatcher owns validation, persistence, assignment, filtering and per-item
  results. Widgets render returned state and invoke commands. No UI-local tag
  store or implicit pin operation. Legacy multi-tag mapping is not decided here.

## Ctrl+, → Tags section

Use the existing settings window, spacing and responsive row primitives. Show
nine ordered slot rows: number keycap, material swatch, name, Edit action.
The 3.2 schema initializes all nine slots with editable defaults. Names,
validation messages and empty-state labels follow the host's Russian UI
language. Slot numbers remain stable when names change.

Edit one slot inline in this section, not in a new native window. Field order:
name → base color → material (Color / Dots) → intensity → preview. Dots is the
proposed single initial generator, not a promise of a larger material library.
For Dots expose spacing and strength; seed is stable and persisted, not a
randomize-every-frame control. No angle control for isotropic dots. Proposed
prototype bounds: spacing 6–24 logical px, strength 0–20%, tint 0–30%; these
are visual tuning bounds to validate, not existing API field contracts.

Use an explicit Apply / Cancel draft: preview changes immediately inside the
editor, Apply persists through the API then updates live cards, Cancel restores
the saved preview. Invalid values keep the draft and show an inline error;
failed persistence never appears committed. A name editor owns punctuation and
digits; global tag shortcuts do not run while it is being edited. Save policy
must stay consistent with the surrounding settings if the host uses a shared
transaction; do not invent a second competing settings store.

Preview uses synthetic Text and image cards with buttons for Standard / Blue
glass and ordinary / selected / cursor / selected+cursor. It reuses the actual
card material function. A CPU preview cannot certify GPU/native glass; label
unavailable optical preview plainly rather than simulating a passing result.
On narrow settings widths fields stack and keycaps/swatch retain their size.

## Card material and state composition

Paint order: existing base/optical surface → bounded tag tint and clipped dots
→ content and metadata → independent selection outline → cursor marker.
Tag color never overwrites selection RGB/intensity or theme accent. Existing
selected optical parameters remain selected-only. Texture must not paint noise
over image pixels, glyphs or emoji. Use margins/metadata surface; image cards
without exposed margins retain a compact named tag chip in metadata. The same
indicator is available for Text, Images, Links, Pods and Files, including an
unavailable Files pointer. Name is also available through tooltip/Space/menu.

Without tag, Standard retains exact legacy fallback. Standard supports dots
without enabling optics. Blue glass retains its refracted surface; material
strength must not conceal it. When readability fails, raise the local text
surface opacity instead of lowering glyph opacity. Stored material parameters
survive theme/glass switching. Unpin removes the card's tag tint, dots and tag
chip after domain success; the slot definition remains unchanged in settings.
Repin restores assignment eligibility, not the previous tag or material.

Selected uses the existing independently colored outline, including intensity
zero. Cursor-only uses a small contrasting corner marker, not another tag-
colored outline. Selected+cursor shows both. Hover changes existing surface
feedback only; it is not selection. All decorations clip to the card; none
change hit targets, card geometry, ordering or domain state.

Cache a deterministic tile or neutral material resource by normalized material
parameters/seed and reuse it across cards. Anchor consistently in card-local
coordinates. Do not generate a random pattern each frame or allocate a shader
per object. Check DPI/scale for dot shimmer and moiré before accepting bounds.
Avilon may own the neutral generator/compositor; Katran owns slot IDs and data.

## Tag menu states and input

Reuse Ctrl+K Actions appearance and interaction: shared ActionRow/KeyCaps,
search, selected row, Up/Down, Enter, Escape and existing dismissal/focus rules.
The menu is anchored consistently for footer and keyboard invocation. Retain
clear mode headings so “filter” cannot be mistaken for “assign”.

| Mode | Rows and result |
| --- | --- |
| Filter | All; configured slots 1–9; active-filter mark and name. Enter applies the exact row. All clears only tag filter, never pinned-only. Empty results keep the filter visible and offer All. |
| Assign to selection | Configured slots 1–9; applying replaces one active tag on each eligible pinned object. Show selection count and current/mixed tag summary. No selection: disabled with reason. |
| Clear selected tags | Explicit action naming the selection count. Never confuse with All/filter reset. No existing tag: disabled. Clears tag only, never unpins. |

Pinned-only and tag filtering remain independent predicates. Changing either
must not silently change the other. Preserve selection according to dispatcher
results; do not report success merely because the menu closed.

For all-unpinned or mixed selections, disable new assignment/replacement for
the whole selection and explain “Assignment requires all selected objects to
be pinned”; offer no implicit pin or silent eligible-only subset. API clients
must still receive explicit per-item failures. An unpinned object has no tag
assignment to clear; disable Clear when none of the selected objects has a tag.
For a mixed selection, the explicit Clear action removes existing assignments
only, with per-item outcomes; it neither pins nor changes slot definitions.
After unpin, refresh menu summaries/filter results from the dispatcher. If the
object leaves a tag-filtered view, do not retain a stale tagged card in the UI.

Digit accelerators belong to the active menu context only. In search/name
editing digits and punctuation remain text. Final global assign/clear chords
remain unconfirmed; display catalogue-provided bindings, never hard-code a
proposed Ctrl+/ or Ctrl+Alt+/. Reopening shows current domain state, not stale
draft state. Batch failure stays visible with per-item results and retry for
failed items; successful items are not silently reapplied.

## Contrast, motion and owner visual matrix

Tag identity never relies on color or texture alone. Aim for 4.5:1 text contrast
and 3:1 non-text state cues against their immediate surface; measure rendered
composites on light/dark artwork, not RGB tokens alone. Selected/cursor remain
visible when tag and selection colors match. No animated texture. Respect the
existing reduced-motion preference; menu/material changes need no new motion.

All fixtures below are synthetic; Lilia operates Windows manually. No personal
images, Clipboard mutation or original-file deletion is required.

| Fixture/state | Owner checks |
| --- | --- |
| Text, full-bleed and transparent Images, long Links, large Pods, long/missing Files | Name readable, material outside content, stable geometry, Files pointer state still clear. |
| Each type × no tag/color/dots × Standard/Blue glass | Legacy no-tag baseline; dots visible but quiet; optics survives; same tag identity. |
| Ordinary/hover/selected/cursor/selected+cursor | Selected and cursor independently identifiable; Escape retains cursor per existing contract. |
| Matching tag/selection colors; selection intensity 0/100 | Outline/cursor/name remain visible; tag never modifies selection setting. |
| Light/dark static background and synthetic video; narrow window; 100/150/200% DPI | Contrast, clipping, no shimmer/moiré; no per-frame random changes. |
| Pinned tagged → unpin → pin again | Assignment, card tint/dots/chip removed on unpin; slot name/color/material unchanged; repin leaves object untagged until explicit assignment. |
| Unpin while a tag filter/menu is active | Object leaves the filtered result; counts/menu summaries refresh; no stale tag or restored assignment after restart. |
| Empty/pinned/unpinned/mixed selection; one/mixed existing tags | Correct disabled explanations; no implicit pin or partial silent assignment. |
| Footer icon / keyboard; filter / assign / clear; search editing | Same menu; clear distinction between clearing filter and object tag; Escape/input ownership. |
| Slot edit Apply/Cancel/API failure; restart; theme/glass toggle | Saved data stable; failure not reported as committed; no tag loss. |
| Reduced motion; repeated menu opening and fast filtering | No texture animation; no stale filter or input leakage. |

Before GUI wiring: synthetic API proof must cover slot validation/persistence,
one-tag replacement, assignment removal on unpin, unchanged slot definitions,
no assignment restoration on repin/restart and forbidden unpinned assignment. Native
visual review proves appearance only, not these domain guarantees.

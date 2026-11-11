# Experimental theme foundation

Owner design references show dark navy surfaces, thin cool borders, blue selection/focus, consistent search/buttons/Actions/settings, and optional lily artwork. They are useful visual references, not numeric layout/state specifications. The first prototype retains current geometry and command paths, with a separately selectable skin.

Settings → Темы offers Standard (default) and Blue Experimental. Selection applies immediately through SetTheme; accent RGB and bounded decorative curves are optional for Blue. Standard returns every caller's original legacy color and restores the previous visual baseline. Color roles are centralized in src/bin/theme_ui.rs; shared sections/actions/keycaps use these roles, as do the main shell, cards, palette and settings. A new view should use semantic colors rather than inventing its own constants. Additional rollout to all niche dialogs can be done incrementally.

ThemePreferences is a typed, version-compatible-default Settings field; Themes and SetTheme are dispatcher endpoints. Theme changes preserve all other preferences and interaction state. An unsaved settings draft cannot revert a newly selected theme. Appearance never changes capture, clipboard, file actions or shortcuts.

The prototype uses clipped native vector curves, not the supplied lily illustration itself and not live desktop blur. Exact floral artwork should be supplied as separate transparent PNG/SVG with clear display bounds and allowed opacity; keep it a replaceable decoration layer independent of content, hit testing and scrolling. Do not crop complete UI screenshots into background assets. Full recreation of every mockup and arbitrary theme/plugin scripts are outside this first skin.

Validation: theme API persistence/default/reset; Standard exact legacy color/visual roundtrip; decoration clipping; shared-shell geometry parity for Blue/Standard; existing UI tests. The initial shell fixture omitted its expected image landmark, corrected before final checks. Native appearance remains owner acceptance; no desktop automation. Standard stays the default. No release or GitHub push is inferred for this prototype.

A separate Azuros code review covers immutable 81abc6f: docs/reviews/2026-11-11-azuros.md. Its five P2 findings remain a separate engineering patch; they are not claimed fixed by colors.

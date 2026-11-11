# Katran UI design patterns

Reusable patterns from the accepted 3.0 appearance work. This is a cookbook for
new views and dialogs, not a feature roadmap. Keep each screen a client of the
existing domain/API contract.

## Theme through semantic tokens

Use `theme_ui::Role` for background, surface, raised surface, border, text,
muted text, accent and selection. Pass the exact legacy color at each call site:
the Standard preset returns that value unchanged. Avoid new view-specific color
constants. See [theme_ui.rs](../src/bin/theme_ui.rs) and the existing visual
rules in [themes.md](themes.md).

## Share action rows and keycaps

Use `ActionRow` for command-like choices: icon, clipped single-line label,
measured right-aligned keycaps, and full-shortcut tooltip. Give it presentation
data; let its caller execute the command. Actions and Sort already share this
component. See [action_row.rs](../src/bin/action_row.rs),
[palette.rs](../src/bin/palette.rs), and [KeyCaps in ui_style.rs](../src/bin/ui_style.rs).

## Keep status compact and anchored

The main status strip is 34 logical pixels tall. Measure the actual trigger or
anchor rectangle and place its popup from that rectangle; recompute after
resize. Do not guess from a historic window edge or allow a status control to
change the shelf's content geometry. See the footer in
[ui.rs](../src/bin/ui.rs) and measured popup anchors in
[palette.rs](../src/bin/palette.rs).

## Paint decoration behind opaque content

Background artwork is a cached texture painted with cover UVs over the full
viewport; crop to fit, never stretch. With native glass, background artwork and
surface fills use alpha, while text and icons remain opaque. If native Acrylic
is unavailable, use opaque fills. Treat artwork as decoration: it must not own
hit testing, scrolling, or layout. See [theme_ui.rs](../src/bin/theme_ui.rs).

## Give borderless windows a drag path and shortcuts a single owner

If a window has no native title bar, reserve a small explicit drag strip and
start the platform window drag from that hit area. For keyboard input, establish
one active modal/input owner. Run modal handlers before shelf handlers and
consume modal keys so they cannot trigger the underlying view. Text fields keep
their normal editing keys. See the settings drag strip in
[settings_ui.rs](../src/bin/settings_ui.rs) and handler ordering in
[ui.rs](../src/bin/ui.rs).

## Keep native pickers and domain work at their boundaries

Request a native picker without blocking the UI loop; pass its selected path to
the domain/API for validation and preparation, then commit through the domain
operation and report its result. The picker must not copy, mutate, or persist
product data itself. The async picker plus domain prepare/commit path is implemented and checked with
synthetic pending-picker, cancellation and source-independence cases. Native dialog
interaction remains owner acceptance. Existing
references: [settings_ui.rs](../src/bin/settings_ui.rs),
[theme_file_picker.rs](../src/bin/theme_file_picker.rs), and
[domain_jobs.rs](../src/domain_jobs.rs).

## Test geometry headlessly; leave native appearance to owner acceptance

Headless UI checks can reproduce viewport size, scale, layout rectangles,
shortcut dispatch, modal key consumption and action-row fitting. Keep those
checks deterministic and synthetic. Acrylic appearance, native picker feel,
window dragging, shadows and platform paste behavior require owner-run Windows
acceptance. A passing headless geometry check does not establish native visual
acceptance.

The [development map](development.md) defines the API-first and shared-dispatcher
boundary; use it before adding a new client path.

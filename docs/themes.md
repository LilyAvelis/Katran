# Experimental theme foundation

Owner design references show dark navy surfaces, thin cool borders, blue selection/focus, consistent search/buttons/Actions/settings, and optional lily artwork. They are useful visual references, not numeric layout/state specifications. The first prototype retains current geometry and command paths, with a separately selectable skin.

Settings → Темы offers Standard (default) and Blue Experimental. Selection applies immediately through SetTheme; accent RGB and bounded decorative curves are optional for Blue. Standard returns every caller's original legacy color and restores the previous visual baseline. Color roles are centralized in src/bin/theme_ui.rs; shared sections/actions/keycaps use these roles, as do the main shell, cards, palette and settings. A new view should use semantic colors rather than inventing its own constants. Additional rollout to all niche dialogs can be done incrementally.

ThemePreferences is a typed, version-compatible-default Settings field; Themes and SetTheme are dispatcher endpoints. Theme changes preserve all other preferences and interaction state. An unsaved settings draft cannot revert a newly selected theme. Appearance never changes capture, clipboard, file actions or shortcuts.

The prototype uses clipped native vector curves, not the supplied lily illustration itself and not live desktop blur. Exact floral artwork should be supplied as separate transparent PNG/SVG with clear display bounds and allowed opacity; keep it a replaceable decoration layer independent of content, hit testing and scrolling. Do not crop complete UI screenshots into background assets. Full recreation of every mockup and arbitrary theme/plugin scripts are outside this first skin.

Validation: theme API persistence/default/reset; Standard exact legacy color/visual roundtrip; decoration clipping; shared-shell geometry parity for Blue/Standard; existing UI tests. The initial shell fixture omitted its expected image landmark, corrected before final checks. Native appearance remains owner acceptance; no desktop automation. Standard stays the default. No release or GitHub push is inferred for this prototype.

A separate Azuros code review covers immutable 81abc6f: docs/reviews/2026-11-11-azuros.md. Its five P2 findings remain a separate engineering patch; they are not claimed fixed by colors.

Follow-up: shared footer is now one 34px IDE-style strip, with flat accessible SVG icon segments for context, sorting (outside Pods), operation details and Folder recovery. Tooltips name each action; existing detail panels retain their dispatcher commands. Selection count is shown only when nonzero. Capture lamp and persistent capture label are removed; pause/resume remains in Actions. New settings default capture_on_start to true, explicit saved values remain respected. Actions popup follows the actual trigger rectangle rather than a historical window-edge offset; resize geometry is checked headlessly. Blue widget weak fills and open states use theme tokens, with selection tinted by the chosen accent. Artwork remains pending a standalone designer asset.


## Local lilies, sort menu and native material experiment

The owner supplied five lily PNGs, now embedded under assets/themes/lilies. Blue → Фон selects solid or variants1–5; decode/texture is cached per variant, aspect ratio is retained with navy letterboxing. No external file path or per-frame image decoding. Lily artwork and glass are alternative backdrops; invalid combined settings are rejected by the shared Settings API.

General → Подсказка сортировки enables a non-interactive two-second confirmation after a successful sort action. Repeated choices replace the notice, disabling it removes pending notices. It does not take keyboard focus. The bottom status icon still opens operation details.

Comma now opens a separate persistent Sort modal, also available beside Actions. T arrival, M modified, S size, A name, E extension, N natural, O origin, R shuffle; Shift selects reverse (shuffle ignores reverse). Escape/outside click closes. Physical keys work across layouts; the open modal consumes other keyboard input. The accidental persisted paste_previous=Comma assignment is not rewritten; the Sort launcher consumes comma first on the shelf. Text fields retain their comma input. Sorts are omitted from general Actions and shortcut-editing rows; their domain commands remain available to API clients. Review found this persisted assignment, not a Windows global-hotkey collision, caused paste/hide on comma.

Blue → Компактное окно removes the app-drawn title/cross, retaining an8px drag strip above search. Standard retains its original header. Blue → Стекло is opt-in Windows Desktop Acrylic via DWM_SYSTEMBACKDROP_TYPE and corner preference. Enabling requires a full tray Quit and restart, because alpha viewport creation is startup-bound. A failed DWM request keeps opaque fills. Disabling glass/choosing Standard restores opaque painting; re-enabling then requires another restart. No claim of Apple Liquid Glass refraction. Native appearance, shadow/corner behavior and visible blur still require owner acceptance on the actual Windows renderer; HRESULT alone is not acceptance. Settings remains a normal separate window.

Evidence:101 UI tests,2 settings API tests,3 themes API tests passed; all-target clippy passed. Includes actual-trigger popup positioning, comma override precedence, reverse input, notice expiry/preference/focus, compact-header geometry, native-activation fallback and cached lily texture checks. No desktop automation. First checkpoint cede792 contains lily/sort behavior before native material wiring. Five Azuros P2 findings remain open.


## Combined backdrop and restart refinement

Supersedes the alternative-backdrop limitation above: lily_background and glass may now be enabled together through SetTheme/SetSettings. Artwork uses centered cover UV cropping (no stretch/letterbox) across the full root viewport, including the transparent bottom strip. With native Acrylic active, artwork is painted at160/255 alpha without an opaque base; Surface/Raised/Selection use200/255 alpha while text/icons remain opaque. Unsupported Acrylic keeps the opaque fallback. Selecting lily art no longer disables glass. The Sort modal now uses exactly the shared ActionRow used by Actions: SVG icon, clipped label, measured right-aligned keycaps.

Changing glass in Themes saves it first, then shows Restart now/Later in the Settings viewport. Artwork changes apply immediately without restarting. Confirmation queues a restart until the foreground/capture/paste operation is idle; RestartApplication is an API host intent with no data mutation. Native adapter starts the same executable with the same data argument and a parent PID marker. Successor waits up to30s for predecessor exit before single-instance acquisition; no forced termination or external shell. Cancel/Later starts no process. Native restart remains owner acceptance; only pure argument handling and API intent were tested automatically.

Enter now means paste_previous and Alt Enter means open. A versioned shortcut migration removes only known old paste_previous=Comma/Alt Enter and old open=Enter overrides. Unrelated bindings and explicit disables remain; conflicting explicit user assignments are not silently replaced. Migration marker is saved at the next shortcut edit. Text-edit focus retains Enter/Alt Enter; modal controls keep their own handling. The owner's old Comma override explains the missing Alt Enter dispatch, rather than a proven regression in native SendInput.

Validation:104 UI tests passed in one run; the additional cover/translucency test passed after bounding floating-point UV roundoff. Enter migration/default/custom/conflict tests, overlay API, four theme/restart API cases and all-target clippy passed. Existing shared ActionRow layout tests remain used. Native Windows recipient paste, combined glass appearance and confirmed self-restart await owner checks. All changes local, no release or GitHub push.


## Custom backgrounds and complete content surfaces

Text/Links thumbnail fills, Markdown code surfaces, Tables canvas/header/alternating rows/grid/text now use shared theme roles with exact legacy Standard fallback. Existing geometry and virtualization remain. Settings viewport disables native decorations, retains an8px drag strip, and Ctrl+Comma closes it (except during shortcut recording); Escape also remains.

Themes → Собственные фоны provides a native Windows image picker and a saved-background dropdown. ImportThemeBackground validates actual image decoding, bounds input32MiB/32million pixels/decoder128MiB, normalizes to PNG with long edge<=4096 and stores a SHA256-addressed managed copy under data/theme-backgrounds. Up to64 unique images; repeated content deduplicates, source files never change. Five bundled lily presets remain separate. ListThemeBackgrounds/GetThemeBackground serve saved metadata and trusted managed paths; ID/path containment, reparse entries and content checks reject invalid references. SetTheme/SetSettings validate the selected managed background. Imported copies are private profile data and not Git source/assets.

Custom selection and glass work together using the same cover cropping/translucent paint as lilies. Custom texture cache retains only the active custom image; catalogue is loaded once per settings session/import, not read/hashed every UI frame. An import/selection applies immediately, so no restart prompt is needed. The earlier glass confirmation/restart flow remains.

Validation: managed-background module synthetic cases cover invalid/oversized input, dedup, moved source, ID traversal and tamper; full UI run106 passed with the new Ctrl+Comma case corrected to read the API envelope and passed in its focused rerun. Five Themes API cases passed, including import/select/reopen and preserved presets; shared Text/Table surface headless gate passed. All-target clippy passed. Native file picker, borderless settings feel and custom art appearance await owner acceptance. No release/push, no native desktop automation or personal file imports performed.

## Independent selection color

`ThemePreferences.selection` is an optional RGB color independent of `accent`.
None follows the theme's existing selection/accent. `selection_intensity` is
0–100 (default35) and controls the interior tint; zero keeps the visible outline.
Both are persisted and validated through SetTheme/SetSettings. The Themes page
applies these controls immediately and offers a reset to the theme color.

Chosen cards use the separate selection outline in gallery, stream and Pods.
With optics enabled, only selected card surfaces mix that RGB into their
refracted application-owned background and edge light. Unselected surfaces and
the Actions/Sort optical treatment remain unchanged. Thumbnails, emoji and text
are painted above the optical surface. Without optics the same setting controls
the ordinary selection fill. Standard theme with no custom color retains its
legacy fallback. An independent border preserves the selection cue regardless
of interior intensity; custom colors do not change the main accent or links.

API tests cover migration, persistence, independent accent/selection values and
rejection above100 without losing saved settings. Headless tests check semantic
colors, selected-only optical parameters and narrow settings layouts. GPU
appearance remains subject to the owner's manual Windows check.

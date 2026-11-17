# Inline tags and pinned properties · 2026-11-18

Owner reported two recurring failures in the inline tag editor: leaving Katran
could leave the modal owning input after return, and dragging style sliders
continuously reloaded/flickered image thumbnails. The pinned properties form also
needed a clearer layout and keyboard confirmation. Local main/primary install;
no release or GitHub publication.

## Tag lifecycle

`tag_menu_ui::State::close(ctx)` closes the active menu/editor, nested popups and
focus, clears pending save/reload requests and requests repaint. It retains a
dirty/conflicted draft, its checked revision and error; reopening does not
silently discard or commit it. Inactive close does not steal other UI focus.
The editor now honors modal backdrop/close responses. Slash and Backslash without
modifiers dismiss the editor even when its field owns focus; paired typed key
text is consumed, pasted strings are not. Ctrl+Shift+Slash is unaffected.
Escape returns from editor to tags; ordinary palette Slash retains its existing
Filter All + close behavior, and palette Backslash dismisses without filtering.

Main-window focus loss and hide call this same helper before later input logic.
Tab-global summon also uses it, rather than only clearing `open`. Thus an active
tag modal cannot remain a hidden input owner after those transitions. No database
or storage path is added; checked saves remain `SaveTagSlotChecked`.

## Image reload cause and correction

A checked tag style save increments `Interaction.view_generation`. Image context
included that generation; every slider save therefore cleared textures, pending
preview work and geometry as though the browse context changed. Full shelf
refresh on each save also repeated browsing.

After a successful local style ACK, the UI compares Interaction immediately
before/after the save. `Images::accept_metadata_generation` preserves textures,
context epoch and in-flight work only when the old image context matches, the
generation is exactly the next one, and every other Interaction field is equal.
`Shelf::accept_tag_style_update` then accepts the new metadata and discards the
old generation-bound pagination cursor. Load-more follows the existing recovery
path to obtain a fresh cursor. Tag materials reload independently.

Other changes use normal refresh/invalidation. Changed file revisions still
request new previews; different tabs/folders/catalogues retain their context
isolation. The optimization is scoped to successful inline tag style saves,
not an unconditional removal of generation from cache identity.

## Pinned properties

The existing modal surface/glass/typography remain. The compact responsive form
uses full-width name and opener fields, separated hint/error text, Cancel on the
left and one primary Save on the right with Ctrl+Enter keycap/tooltip. Name gets
focus and selection once on open. The opener remains stable during async app
discovery.

Ctrl+Enter from the form and Save use the same `SetPinnedProperties` handler,
saving name and opener. The key is consumed before the shelf can reveal/open an
item. Plain Enter is not a second implicit save. An API error keeps the draft,
selection and modal; success closes popups/focus and refreshes through the normal
path. Escape/backdrop retain the standard nested-popup behavior.

## Evidence and remaining acceptance

Headless UI suite **262/262** passed. New tests cover focus-loss/hide input
release, Slash/Backslash/Escape and pasted text, dirty/conflicted drafts,
successive style updates retaining texture IDs and request queues, valid old
in-flight results, real revision/context invalidation, metadata cursor recovery,
full-width form, Ctrl+Enter persistence/consumption and API-error draft retention.
All-targets Clippy and formatting passed. An initial metadata fixture accidentally
started on Folder and was corrected to explicitly select Images; no production
change or repeated broad test was needed for that fixture.

No desktop automation or personal data was used. Native slider smoothness,
blur/reopen behavior and the form's final visual acceptance remain owner checks
in the primary installation.

# Katran 3.8 · UX handoff

## Tags

- Settings → Tags begins with a visible tab selector, initially the active tab. List every domain-supported tab, including hidden tabs; Folder has no tags. Explain that this edits the tag set and appearance, while assignment happens on the shelf. Never fall back to a different tab's set.
- Keep the existing settings rows and visual language. One editor opens immediately beneath its tag row; selecting it again closes it, selecting another moves it. Keep the number and name visible, truncate long names with a tooltip, and scope widget IDs by tab and slot.
- Retain all material controls and previews, including the existing initial texture-strength behavior. Changes update the preview and shelf live through the domain save; name editing may use debounce and flush. No mandatory Apply/Cancel.
- Show saving, saved only after acknowledgment, and inline errors. Preserve a failed draft and its original tab, with retry; flush on close or tab change without silently writing to another tab. Verify persistence across restart and shelf refresh after acknowledgment.
- Ctrl+Shift+Comma is contextual: preserve its existing Actions shortcut recorder and the intended separate Tags settings use; do not add a global binding or guess its Tags action.

## Folder picker

- Keep one picker and the existing ChooseFolder path. Provide visible Search, Favorites, History and Standard folders modes; Ctrl+F/H directly from Folder open the existing respective modes, and existing picker shortcuts remain available.
- Standard folders are a separate visible group, with localized names and actual Windows Known Folder paths. Report individual unavailable folders rather than constructing fallback paths. Ctrl+Y remains a proposal, not an approved shortcut.
- Search rows emphasize folder name and show full path and tooltip. Preserve distinct loading, empty and error states. Existing focus, arrows, Enter and Escape behavior remains; do not intercept ordinary text editing. The owner performs native Windows visual acceptance.

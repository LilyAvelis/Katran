# Global summon and paste back

Current local 3.8 contract (2026-11-17): [Enter implementation and evidence](patch-3.8/PASTE-IMPLEMENTATION.md).

`Enter` pastes the selected/cursor objects to the last external window visited before returning to Katran. `Alt+Enter` opens the object. Katran preserves the receiver's existing focused control; it does not force a child/DOM editor focus. The owner clarified that this control behavior generally already matches Raycast.

The production UI starts an out-of-context foreground observer before creating its window, even without global shortcuts. The dispatcher obtains validated native evidence; an ordinary return from B to an already-open Katran no longer requires resummoning over B. Own Settings/main transitions retain B. Unknown/transient/new invalid destinations prevent delivery instead of falling back to A. No target history is stored or logged.

`begin_paste_previous {owner_window}` validates owner foreground and freezes a target/identity/visit lease with the existing revision-bound copy job. `complete_paste_previous {owner_window}` returns `copying` until ready, cancels on a changed visit/focus, and authorizes one delivery after complete Clipboard publication. Returning after focus loss does not resurrect that attempt. A fresh summon from the same external app can establish a new intent.

Activation uses a bounded readiness handshake, then repeats identity, foreground, modifier and Clipboard-sequence checks. Partial SendInput is never retried. `sent` means Windows accepted input, not that Notion/chat inserted or attached the object. A post-send observer cleanup warning does not negate submitted input. Windows can deny activation or block input across integrity levels; the last validation and OS input are not an atomic operation.

File publication now owns a message-only HWND through closing Clipboard, retaining CF_HDROP plus optional single-image DIBV5. Copy replies include safe publication format/count/warning facts. Configured diagnostics distinguish prepare, cancellation, copy and delivery without file paths, URLs, titles or text. Native recipient acceptance remains an owner check; the local scripts/paste-receiver.html is an optional offline browser diagnostic.

## Summon and tray

Settings → General records the global overlay chord. Settings → Tabs assigns independent show-only tab chords. No default chord or startup service is installed. Registration failures remain visible; Katran does not take another application's chord. Physical Ctrl/Alt/Shift/Win combinations support standard punctuation/navigation keys, including Ctrl+Shift+Slash.

The global hotkey thread captures the external window before showing Katran. The dispatcher revalidates captures against current observation and process identity. Overlay summon while Katran is foreground toggles visibility; a tab shortcut always shows its requested enabled tab. Native summon moves Katran to the target's virtual desktop/monitor when Windows permits. Repeated own-process calls retain the visit target.

With a resident tray/listener, × hides; explicit Quit exits. Escape hides after transient editors/dialogs close. Tray menu activation is not guessed into a previous editor origin. Native tray and receiver acceptance are performed by the owner, not desktop automation.

## Tests

Host and publication boundaries are injectable. Synthetic tests cover last external destination without summon, stale capture rejection, owner/Settings transitions, focus-loss cancellation, target replacement, timeout/partial-send single-use, successful-send cleanup warnings, repeat summon and Clipboard owner lifetime. They do not open real Clipboard, install hooks or prove Notion acceptance. Ordinary Service/test construction deliberately does not start native observation; a native integration client must supply WindowsHost::with_observer().

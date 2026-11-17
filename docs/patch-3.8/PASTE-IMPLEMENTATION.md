# Enter follow-up · 2026-11-17

Subsequent browser-Notion failure was isolated and corrected at the file receipt
sequence boundary; the owner confirmed the same file now inserts. See
[incident and native confirmation](PASTE-SEQUENCE.md). Earlier pending acceptance
statements below describe the first Enter artifact, not that later confirmation.

Owner scope: use the last external window visited before returning to an already
open Katran; investigate Notion/chat file delivery. Lilia clarified that retaining
the receiver's focused text control generally already works. This patch does not
force child/DOM focus. Local main and primary installation only; no publication.

## Destination and one delivery attempt

Production UI creates `WindowsHost::with_observer()` before eframe activates its
window, independently of assigned hotkeys. Ordinary Service constructors and
headless tests do not install native hooks. An unavailable observer fails closed
for paste instead of silently reusing a remembered target.

`foreground_windows.rs` receives ordered out-of-context foreground/destroy events
on a dedicated message thread. It keeps only the current identity, external
candidate and visit epoch in RAM. Own main/Settings transitions preserve the
external target. A newer unknown/shell-transient window clears the candidate;
destroy invalidates it. Arbitrary launcher windows are not guessed away. No
titles, content or paths are collected. Snapshot/arm/check requests drain the
native queue through a bounded barrier and compare current foreground. Hook
startup, barriers and request failures are explicit; Drop unhooks on its thread.

`BeginPastePrevious` obtains native evidence through the injectable Host and
freezes `PendingPaste/PasteLease`: copy job, owner identity, exact target and
visit epoch. Enter does not require a fresh explicit summon. Before polling or
delivery, changed owner/focus/visit cancels the operation; returning afterwards
does not revive it. New captures cannot retarget a pending lease. Stale A captures
are rejected after B. Own Settings windows retain B; separate external windows
in one process remain distinct. Successful input consumes the old intent; a new
summon from the same still-foreground external window can start a fresh visit.

Native activation arms the expected target transition, requests foreground and
uses a bounded 500 ms WM_NULL readiness handshake. It does not attach input
queues or use an arbitrary sleep. Modifiers, identity, foreground and clipboard
sequence are checked again before SendInput. Zero input is `input_blocked`;
partial input is `input_partial`, with best-effort key release and no retry.
An observer cleanup error after successful input remains `sent` with a bounded
`tracking_warning`; it does not falsely report that no input was submitted.

Windows event delivery and input are not an atomic transaction. The observer's
own bounded queue/barrier can fail closed, but WinEvent does not expose proof of
all OS queue losses or of an unobserved extremely fast leave/return. Synthetic
tests and code review do not establish those native timing guarantees.

## File publication and Notion investigation

`copy_out.rs` now opens Clipboard with a real message-only owner HWND. The open
guard closes before that window is destroyed, on success and errors. The existing
payload policy remains: primary Unicode-path CF_HDROP, optional single-image
CF_DIBV5, copy effect and owner marker. Required publication failure blocks Enter;
optional pixel failure is a warning and retains the file-list result.

Both synchronous and prepared-copy API replies include `publication`: successful
static format names, published-file count, optional-pixel warning flag and exact
sequence. Grouped exports count output files, not selected source items. Fake or
drag sinks without publication evidence return no facts. Existing configured
diagnostics record prepare/cancel/copy/delivery stages and bounded categories;
human-readable warnings, paths, window titles, text and URLs are not copied into
these facts. `sent` still means submitted input, not an attached file.

The NULL-owner publication was a verified Win32-contract defect and is fixed.
The previous-target defect is fixed in the dispatcher policy. Neither establishes
why a particular Notion editor rejected a file. Chromium reads CF_HDROP and can
consume bitmap data; absence of virtual-file formats alone does not justify an
OLE rewrite. Notion's official media guide documents upload/drag routes and does
not promise universal clipboard attachment in every editor context.
[Chromium clipboard](https://chromium.googlesource.com/chromium/src/+/main/ui/base/clipboard/clipboard_win.cc),
[Notion media guide](https://www.notion.com/help/images-files-and-media).

If Notion still fails, compare the same synthetic single PNG/non-image file with
Katran Enter versus Katran Ctrl+C followed by manual Ctrl+V in the same focused
editor. The local [browser receiver](../../scripts/paste-receiver.html) can show
Ctrl+V, paste events and MIME/file counts without reading names/content or using
network. It does not send a message, upload a file or prove Notion acceptance.
The fixture was prepared for owner operation; no native receiver test was run
against the owner's clipboard. Export retention/format preference are unchanged
pending evidence of a delayed-reader or recipient-specific failure.

## Evidence

- Overlay policy 5/5 and copy-before-paste API 2/2 passed.
- New paste intent API 6/6: B without summon, old target rejection, focus-loss
  cancellation, timeout/partial single-use, post-send cleanup and a second fresh
  summon from the same external window.
- Observer policy 4/4; synthetic file publication 11/11 passed. These tests do
  not install hooks, open real Clipboard or exercise recipient applications.
- Emoji catalogue API 7/7, smart-copy 2/2 and global tab API 3/3 passed. Headless
  UI aggregate passed 252/253; the failing legacy fake wrongly kept the external
  window foreground during Begin and passed its targeted rerun after correcting
  that simulation. No native paste was used to repair the test.
- Azuros's bounded source review found no remaining blocking issue after the
  repeated-summon correction; its requested combined API regression passed.
- Native manual acceptance remains required for B → already-open Katran,
  Notion and chats. See [the original audit](PASTE-AUDIT.md).

## Global shortcut correction in the same build

The owner reported Ctrl+Shift+Slash rejected and Shift combinations lost. The
parser now supports standard punctuation and navigation keys, including physical
Slash/Backslash/Comma/Equals/brackets. Physical modifier press/release events are
combined with each key event's modifier snapshot; focus loss clears held state.
API parser/persistence tests 2/2 and recorder tests 7/7 passed. Modifier-only
chords remain invalid; native Windows reservation errors remain explicit.

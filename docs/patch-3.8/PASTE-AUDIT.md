# Enter paste-back audit and next patch contract

2026-11-17 · Azuros Claudapi · read-only audit. Read began at `5acd8e1` and
the hotkey integration was rechecked at local `ada44aae7b8477274b00f8a1faafd89bcaf4723e`.
The relevant hotkey source is committed at that checkpoint. This audit changes
only this document. No build, native window operation, clipboard access,
recipient interaction or personal-file inspection was performed. Existing test
source was read; passing counts supplied by the integrator are not new audit runs.

## Decision

The first reported failure has a concrete source-level explanation: Katran
remembers the application captured by a summon, rather than the application
visited most recently before returning to an already open Katran. The new tab
hotkeys improve the explicit summon route but do not change that rule.

The next patch should first correct target acquisition and bind each pending
paste to one foreground intent. A small independent file-publication fix is also
justified by a documented Win32 contract mismatch. A general OLE/virtual-file
rewrite, unconditional sleeps, repeated Ctrl+V or a claim of Notion compatibility
is not justified by the evidence. `sent` remains input submission, not insertion.

Lilia's two observations must remain separate: stale A after visiting B is
strongly explained by the code; Telegram versus Notion/chat acceptance has not
been reproduced and is not yet attributable to a particular format or focus issue.

## Ranked findings

### 1. P1 · The remembered destination does not follow ordinary reactivation

**Verified code:** `Session::previous` has only the summon/adopt assignment paths
(`src/overlay.rs:140`, `:161`, `:196`). Validation checks that the stored identity
is still alive; it does not establish that it is the last external foreground
(`src/overlay.rs:203`). A successful send clears it (`:250`). Neither tray
Show/Toggle (`src/bin/overlay_ui.rs:107`) nor single-instance activation
(`src/bin/ui.rs:1939`, `src/lib.rs:1297`) captures the application being left.
There is no foreground-event observer or native `WM_ACTIVATE` origin adapter in
these paths. An egui focus event cannot by itself recover the previous HWND.

Therefore this sequence is sufficient in the current policy: capture A; keep
Katran open; visit B; click Katran; Enter. If A still exists, validation can select
A. If a prior successful paste cleared the session, the equivalent route instead
reports `no_previous_target`. This is a source-level derivation, not an independent
native reproduction of Lilia's exact sequence.

**Hotkey recheck:** `SummonTabOverlay` calls the existing validated adoption for
an external capture and deliberately preserves the previous target on a
self-foreground invocation (`src/lib.rs:773`, `src/bin/tab_hotkey_ui.rs:174`).
Its native listener captures before showing (`src/overlay_windows.rs:395`).
Consequently it helps B → explicit chord → Katran, while B → manual refocus is
still missing. `tests/tab_global_hotkeys_api.rs:143` tests the preservation rule,
not acquisition on normal reactivation.

### 2. P1 · Pending copy has no lease on the user's foreground intent

**Verified code:** Begin validates a destination, snapshots the selection through
the normal copy job, and stores only `(JobId, owner_hwnd)`
(`src/overlay_service.rs:65`, `:72`, `:76`; `src/lib.rs:129`). Complete polls that
job and eventually calls `Session::paste` (`src/overlay_service.rs:80`, `:126`).
`Session::paste` activates the saved target without first requiring Katran to
remain foreground or requiring an unchanged owner-activation epoch
(`src/overlay.rs:228`, `:236`). The final foreground checks occur **after** that
activation. Thus Enter → slow preparation → user switches to C can still restore
and paste into A. Those final checks do not detect that the user's intent changed.

Existing guards reject a new explicit summon while `overlay_copy` exists
(`src/overlay_service.rs:41`, `:56`; `src/lib.rs:778`). They currently prevent
explicit replacement of the session; a new observer must not bypass them by
mutating `Session::previous` during copy. The immutable destination for a pending
paste and the mutable last-external observation are different state.

This is an additional reachable control-flow gap, not a claim that it caused the
reported Notion behavior.

### 3. P2 · Immediate foreground verification can reject valid activation in progress

**Verified code and platform contract:** the native adapter returns immediately
after `SetForegroundWindow` (`src/overlay_windows.rs:122`), and the session
immediately requires the target to be foreground (`src/overlay.rs:237`). Windows
activation between input queues can complete asynchronously; Microsoft documents
why an immediate `GetForegroundWindow` can still return the old window.
This can produce an explicit `foreground_changed` even after a valid request.
It is not evidence that this happened on Lilia's machine.
[Microsoft activation explanation](https://devblogs.microsoft.com/oldnewthing/20161118-00/?p=94745).

Use a bounded readiness phase followed by the existing identity, foreground and
sequence checks. A finite `SendMessageTimeout(WM_NULL)` handshake is one documented
option; it still does not prove editor readiness. Do not attach input queues or
replace the checks with an arbitrary delay. Windows may genuinely deny activation
even when its normal permission conditions appear satisfied.
[SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow).

### 4. P2 · File publication uses a NULL clipboard owner

**Verified code and documented-contract mismatch:**
`src/copy_out.rs:656` uses `clipboard_win::Clipboard::new_attempts(20)`.
The pinned dependency is `clipboard-win = 5.4.3` (`Cargo.toml:46`). Its inspected
local source, `clipboard-win-5.4.3/src/lib.rs:142`, delegates to
`new_attempts_for(NULL, ...)`; `raw.rs:497` empties the clipboard and `:498` calls
`SetClipboardData`. Microsoft documents that NULL owner followed by emptying
leaves no owner and causes that publication call to fail.
[OpenClipboard](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard),
[SetClipboardData](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboarddata).

Text and table publication already create an owned message-only HWND and pass it
to `OpenClipboard` (`src/text_clipboard.rs:230`, `:261`;
`src/table_clipboard.rs:243`). File publication should use the same ownership
principle with the window alive through closing the publication guard. Keep pixel
decoding and export preparation in the existing worker.

This is sufficient reason for a bounded adapter fix. It is **not** proof of the
Telegram/Notion difference: Lilia reports successful file pastes in Telegram, so
actual Windows behavior must be measured rather than inferred from the failure
language in the API documentation. Do not promise that adding an owner fixes a
particular recipient.

### 5. P2 diagnostic boundary · Foreground top-level and injected input do not prove a paste

**Verified code:** the destination contains HWND/PID/process-start time, with no
focused child or recipient acknowledgement (`src/overlay.rs:5`). Native delivery
checks the clipboard sequence and top-level identity, then submits four Ctrl+V
events (`src/overlay_windows.rs:137`, `:173`). `state: sent` is produced after
those calls and the UI hides Katran (`src/overlay_service.rs:131`,
`src/bin/overlay_ui.rs:359`). There is no proof that an editable control, browser
tab, particular chat, upload handler or intended document accepted anything.

Windows reports how many input events were inserted. UIPI permits injection only
to equal/lower integrity targets and does not identify UIPI as the reason through
the return value/error. Existing keyboard state can also interfere; Katran's
modifier-release check is useful, but happens before activation and not atomically
with input (`src/overlay_windows.rs:102`). A partial send must never trigger an
automatic second paste: some of the gesture may already have taken effect.
[SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput).

`GetGUIThreadInfo` can optionally diagnose native focus presence after activation;
its handles can be invalid during transitions and it cannot establish DOM editor
or upload acceptance. Do not force focus into an arbitrary child as a fix.
[GetGUIThreadInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getguithreadinfo).

## Formats and lifetime: what is established

Smart copy is shared with Ctrl+C: all tables → HTML/TSV; all links → Unicode text;
all Text/Pods → Unicode text; otherwise revision-checked exported files
(`src/jobs_service.rs:606`). A mixed selection can therefore become files even
when individual items have text representations. Enter does not have a separate
recipient-specific payload policy.

Files offer Unicode `CF_HDROP` first, then optional single-image `CF_DIBV5`, then
copy `Preferred DropEffect` and an ownership marker (`src/copy_out.rs:18`, `:648`).
There is no PNG registered format or virtual-file descriptor/content pair in
this clipboard path. Single-image conversion is bounded to 32 million pixels
and 256 MiB; multi-file selections do not offer one combined bitmap
(`src/copy_out.rs:33`, `:626`). Pixel failure is a warning, while a successful
HDROP still authorizes input (`src/copy_out.rs:660`, `src/overlay_service.rs:103`).
That distinction should be visible in structured diagnostics.

Missing virtual-file formats is not established as a browser defect. Chromium's
current Windows clipboard implementation directly reads `CF_HDROP` in
`ReadFilenamesInternal`; its PNG reader prefers PNG and otherwise reads a bitmap
via `CF_DIB`. This is evidence about Chromium source, not the installed browser,
Notion build or an application's paste handler. Microsoft documents automatic
`CF_DIBV5` → `CF_DIB` conversion, so absence of an explicit DIB publication is
not by itself a missing-bitmap finding. Test the actual synthesized data and
recipient route before choosing extra formats.
[Chromium Windows clipboard source](https://chromium.googlesource.com/chromium/src/+/main/ui/base/clipboard/clipboard_win.cc),
[Windows synthesized formats](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats).

HDROP is a list of existing file locations; virtual-file content streams are a
different transfer contract. They are not a universal requirement for file paste.
[Shell clipboard formats](https://learn.microsoft.com/en-us/windows/win32/shell/clipboard).

Exports are independent checked snapshots, not original paths, and successful
publication preserves the current batch (`src/copy_out.rs:413`, `:559`). A later
successful **file-copy** commit deletes prior unprotected export batches
(`:562`); merely hiding Katran, quitting, or copying text is not that cleanup
path. A deferred consumer can therefore lose a path if another file copy occurs
before it finishes reading. This conditional lifetime limitation is real; it
does not explain an isolated immediate failure with no subsequent file copy.
Do not add indefinite retention or new OLE machinery without that reproduction.

The current HWND/PID/process-start identity catches different-process reuse. It
cannot uniquely identify a window recreated with the same handle inside the same
still-running process. Revalidate immediately and invalidate any observed target
destruction, but do not describe HWND as an unforgeable lifetime lease. Windows
explicitly notes handle recycling and the race after `IsWindow`.
[IsWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindow).

## Minimal implementation plan, API first

### Patch A · Correct destination and one pending intent

1. Add an injectable native foreground/activation observation boundary and a
   small domain policy for last-external state. Start it independently of which
   hotkeys are assigned; a visible Katran with no global chord must work. Store
   only target identity, observation generation and owner-activation generation
   in RAM. No catalogue/settings schema change or focus history database.
2. Recommended acquisition is one process-lifetime `EVENT_SYSTEM_FOREGROUND`
   observer retaining B before Katran gains focus, with an explicit ordering
   contract. A narrower own-window activation adapter may use `WM_ACTIVATE`'s deactivated
   HWND for the direct B → click/activate Katran route. This message can carry
   NULL and does not recover the application behind an intervening launcher.
   It is an alternative only for a patch deliberately limited to direct
   reactivation; it does not cover all existing launch routes. Neither a
   current-foreground query after Katran activates nor z-order guessing
   reconstructs B. [WM_ACTIVATE](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-activate).
3. For the foreground-hook option, observe own-process transitions too; blindly
   using `WINEVENT_SKIPOWNPROCESS` loses the boundary needed to bind B to this
   visit to Katran. Use an out-of-context, dedicated message-loop thread; keep
   callbacks short and avoid nested message pumping/COM/dispatcher/file work.
   Preserve external+own ordering in a bounded state, rather than overwriting B
   with a single latest-own event. Registration and unhook/teardown belong to the
   same thread. On startup failure, overflow or an unresolved transition, report
   tracking unavailable/pending instead of reusing a known stale A.
   [SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook),
   [UnhookWinEvent](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-unhookwinevent),
   [callback reentrancy](https://learn.microsoft.com/en-us/windows/win32/winauto/guarding-against-reentrancy-in-hook-functions).
4. Native observations are evidence read through the Host; a new public command
   must not make an arbitrary UI-supplied HWND authoritative. Revalidate current
   ownership, exact external window/process identity and observation generation.
   Ignore late captures older than a newer owner-activation boundary. Distinguish
   multiple windows in one external process. Own settings/secondary windows must
   preserve the external identity rather than becoming paste destinations.
5. Define the transient policy explicitly. Zero HWND, desktop/taskbar, invalid
   windows and unresolved activation gaps are not useful recipients. A launcher
   or tray hop can make the intended underlying application ambiguous. Filter
   only recognized native transient cases; do not silently skip arbitrary external
   applications, special-case a guessed Raycast protocol, or fall back to A after
   observing a newer unavailable B. Ambiguity should request a fresh explicit
   summon. Direct B → Katran is the first deterministic acceptance case.
6. At `BeginPastePrevious`, freeze a `PasteLease` containing copy job, owner
   identity, target identity and activation/observation generations. Keep the
   existing revision-bound selection snapshot. New observations may update the
   next candidate but cannot retarget this lease. Loss of owner focus during
   preparation cancels delivery even if the user subsequently returns; a new
   Enter is a new intent. Copy publication may already have happened, but no input
   may follow cancellation. Complete must consume at most this one lease.
7. Add bounded activation readiness and a deadline/cancellation check before
   sending. The expected self-requested target transition is part of this delivery;
   a different foreground transition cancels it. Recheck modifiers, sequence,
   target identity and final foreground at the send boundary. Preserve existing
   errors and never retry a partially submitted gesture automatically. Progress
   can retain `state: copying` with a phase field until ready, or explicitly update
   the UI poller if adding a new state; the current UI otherwise stops polling on
   every non-`copying` reply (`src/bin/overlay_ui.rs:363`).

### Patch B · Fix file owner and expose stage evidence

Use a real owned clipboard window for file publication and make its native
publication operations injectable. Preserve CF_HDROP, optional DIBV5, current
quotas and the copy-before-input receipt. Add structured publication facts to the
copy result: offered/successful format flags, item count and optional-pixel warning
category. Delivery diagnostics should distinguish target acquisition, copy,
activation readiness, clipboard replacement, input blocked/partial and input
submitted. Log only bounded categories/counts/durations/generations under existing
diagnostic settings; no window titles, paths, content, URLs or recipient names.

Do not label a submitted event as attached/inserted. Do not change clipboard
format preference, export retention or use OLE virtual files until the comparison
below identifies that boundary. Update the current baseline and overlay contract
with the eventual implementation; `docs/overlay.md:7` currently has the historical
Alt+Enter/Enter assignment reversed relative to the current catalogue.

## Tests that establish the changed behavior

**Before GUI wiring, synthetic command/Host tests must establish:**

- A captured → B observed → Katran manual activation → Enter targets B; initial
  B → Katran without an earlier summon also works. Closed/unavailable B never
  falls back to A. Same-process external windows remain distinct.
- Own main/settings transitions and self tab hotkeys preserve B. An old A hotkey
  receipt arriving after the B activation cannot replace B. Disabled tabs retain
  current behavior. Successful completion consumes one paste lease; a later real
  B → Katran visit can establish a fresh one.
- A pending copy cannot be retargeted by observation or summon. Owner focus loss
  while preparing cancels activation/input; return to Katran does not resurrect
  that intent. Complete cannot send twice, including failure/cancellation paths.
- Activation requested but not yet ready waits only within its deadline; a real
  focus steal, changed clipboard, held modifier, target replacement and timeout
  prevent input. A short native input count is classified without a second send.
- Observer ordering, startup failure, overflow, teardown and late callbacks have
  explicit results. A gap cannot silently select an older destination. Tests must
  exercise the policy through the dispatcher, not only a widget focus boolean.

**Publication tests with fake native operations** should assert non-NULL owned
HWND, balanced open/close/window lifetime, memory transfer ownership, primary
failure versus optional-pixel warning, Unicode multi-path HDROP, exact receipt
sequence and partial-copy rejection. Existing `tests/overlay_api.rs:80` tests
only the fake text clipboard path; it does not validate the file publisher.

**An isolated Windows VM fixture**, separately authorized and never run against
the owner's current clipboard, is required to prove the native boundary: a
synthetic receiver records Ctrl+V/WM_PASTE and reads the offered payload; verify
single PNG, non-image file, multiple files, DIB synthesis, delayed file reads and
Unicode names. A local browser receiver can record synthetic `paste` event
types/files. This verifies actual format retrieval and focus timing, rather than
equating a fake Host or `SendInput == 4` with recipient acceptance. Test delayed
consumption across a second file copy before changing the retention contract.

**Owner acceptance remains separate:** use a fresh test destination and the same
synthetic file to compare Katran Ctrl+C + manual Ctrl+V, Katran Enter, and Raycast;
start with a single PNG and a non-image file, then multi-file. Record native app
versus browser, whether attachment or pixels was expected, and any explicit Katran
error. If manual Ctrl+V works but Enter fails, investigate target/focus/input. If
both Katran routes fail with identical payload, compare publication/recipient
handling. Raycast's better observed result supplies a comparison case, not a
known implementation protocol. No chat message needs to be sent for acceptance.

Existing overlay tests are useful policy evidence for identity, modifiers,
sequence replacement, copy failure and single use; they are not Windows recipient
tests. `tests/external_paste.rs` covers files entering Folder, the opposite
direction. The global-hotkey candidate must not be described as fixing
Enter on the basis of these checks or this audit.

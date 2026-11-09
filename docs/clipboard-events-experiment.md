# Clipboard events experiment

Branch: `experiment/clipboard-events`. This is a comparison build, not a promotion
to the default application. The ordinary `Start Katran.cmd`, Start Menu/Raycast
entry, stable launcher and stable runtime pointer remain on polling. The Cargo
default UI feature set also retains its original polling path.

## Run and return to the accepted version

Build without starting a window or changing installed shortcuts:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-events-test.ps1
```

Close the currently running Katran manually. Start **Start Katran Events Test.cmd**.
Its title must contain **EVENTS TEST**. The test starter refuses to start while a
Katran process from this project is already running, preventing accidental testing
of the old window and parallel capture writers. `scripts/start-events-test.ps1
-PrintExecutable` only shows the test executable and opens nothing.

Both builds use the same existing `katran-data` shelf and existing storage API;
there is no schema migration. Images captured/deleted in the test remain changed
on that shelf after returning to the normal version. Test sequentially, not in
parallel. To return: close EVENTS TEST, then use ordinary **Start Katran.cmd** or
the normal Raycast application. Git switching/rebuilding is unnecessary for this
runtime rollback. Do not merge/promote before the owner accepts the comparison.

The experimental executable and source-hash manifest live under
`runtime/experiments/clipboard-events/<hash>/`; only that experiment's pointer
is updated. Regular Cargo outputs may be reused by the build, but the installed
normal application is an independent immutable executable. The test starter and
builder never register themselves in the Start Menu or alter its normal entry.

## Behavior and API

With feature `clipboard-events`, a dedicated thread creates a message-only HWND,
registers `AddClipboardFormatListener`, and blocks in `GetMessageW`. Its window
procedure only increments a coalesced generation signal on `WM_CLIPBOARDUPDATE`.
After dispatch, the host wakes egui; it never reads pixels/files, encodes PNG,
opens SQLite or invokes the domain dispatcher from the window procedure.
Shutdown unregisters the listener and destroys the HWND on its owner thread.
Registration or unexpected loop failure is logged and explicitly disables
capture. This test build has no automatic polling fallback that could mask failure.

`App::logic` collects signals and finishes/starts dispatcher jobs even when no
UI is rendered. Settings, Actions and window visibility no longer gate capture.
One existing Service remains the only writer. While another operation owns it,
notifications coalesce to a pending signal; job completion wakes logic again.
Native DnD retains its existing main-thread path; a capture pending during that
modal operation is processed after it returns. Clipboard is a mutable current
snapshot: neither backend guarantees a history of every fast intermediate copy.

The additive command `capture_clipboard` accepts a trigger:

```json
{"type":"capture_clipboard","trigger":{"kind":"changed"}}
{"type":"capture_clipboard","trigger":{"kind":"retry","ticket":4}}
```

It invokes the same `poll_clipboard` snapshot/ingestion adapter as before, adding
`read_ok`, optional `capture_error` and `capture_schedule` to the result. The top
level success means the scheduling command ran; a read failure is explicit in
`read_ok:false` / `capture_error`. `capture_schedule` contains ticket, attempts,
optional retry_after_ms, exhausted and stale. Clients follow that retry hint;
they do not choose retry budgets or duplicate storage behavior.

Only clipboard_busy, clipboard_error and an unstable snapshot (`state:retry`)
schedule retries. Delays are 25, 50, 100, 200, 400, 400, 400 ms: eight total
attempts and 1575 ms scheduled delay, plus actual execution time. Existing native
open attempts remain three per snapshot. New notifications replace older retry
tickets; early retries do no work. Pause invalidates pending retries. Success,
unsupported format, permanent errors and exhaustion stop periodic work. A fresh
notification or pause/re-enable reconciliation can recover after exhaustion;
the user sees an explicit message rather than an infinite retry loop.

Original snapshot consistency checks, provenance, partial file errors and
acknowledgements stay in `capture.rs`. Own clipboard publication still records
its receipt sequence. Explicit enable skips the current sequence; startup-enabled
capture (sequence initially zero) makes one initial attempt, preserving existing
startup behavior. `PollClipboard` remains compatible for explicit API clients.
`Source`/`Clock` injection boundaries allow synthetic API tests without reading
the owner's clipboard; the normal source delegates directly to the existing
native snapshot adapter.

## Checks and owner comparison

```powershell
cargo test --offline --features ui,clipboard-events --target-dir target/build
cargo test --offline --features ui --target-dir target/build
cargo clippy --offline --all-targets --features ui,clipboard-events --target-dir target/build -- -D warnings
```

Tests exercise deterministic retry timing/exhaustion/recovery, stale tickets,
pause/re-enable, transient/permanent errors and partial outcomes through the
dispatcher. Inbox tests cover signals arriving during work and zero repeated
snapshot attempts in idle. A background-client test captures synthetic bytes
without any egui rendering and with Settings/Actions left open. Synthetic native
host tests post messages only to their own message-only window, with actual
clipboard subscription disabled; they verify wakeups, teardown, startup rejection
and unexpected loop failure. Automated tests do not read/write the real clipboard,
start the GUI, activate user windows or validate a real browser upload.

Use synthetic/test images for the owner's native comparison:

| Scenario | Expected |
| --- | --- |
| Enable, browser Copy image; Explorer copy one/many files | Correct new items, thumbnails and origin, same as polling |
| Minimize/hide Katran, copy elsewhere, restore | Items already captured while no UI was rendered |
| Leave Settings/Actions/delete confirmation open and copy elsewhere | Capture continues, existing dialog/selection remains usable |
| Pause, copy; enable, then copy a new object | Paused copies ignored; current clipboard skipped on explicit enable; next copy captured |
| Startup capture enabled / disabled | One initial current-snapshot attempt / no initial read |
| Copy selected shelf files, paste in Afio/Telegram | Existing copy works; own publication is not immediately recaptured |
| Space, zoom, arrows, drag-out; capture updates during drag | Existing preview/export behavior retained; pending capture reconciles after drag |
| Single/group deletion, new copies afterward | Same exact-ID deletion/confirmation; external originals untouched |
| Repeated/quick copies and unsupported text | No unchanged-sequence duplication; unsupported data does not cause infinite retries |
| Long-lived busy/unstable clipboard | Explicit exhaustion; new copy can recover |

Real notification delivery, native minimized behavior and target applications
still require owner testing. No CPU/energy improvement is claimed from the
synthetic tests; a controlled same-build/state comparison is a separate measurement.

References: [Microsoft monitoring guidance](https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard#monitor-the-clipboard-contents),
[AddClipboardFormatListener](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-addclipboardformatlistener),
[message loops](https://learn.microsoft.com/en-us/windows/win32/winmsg/using-messages-and-message-queues#creating-a-message-loop).

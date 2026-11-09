# Application launch and activation · 1.0.5

## User entry point

The build installs `runtime/Katran.exe`, a stable native launcher with the Katran
icon and metadata. It reads the validated 12-hex current-version pointer and
starts the immutable UI executable with an absolute shelf path; it uses no shell
expansion. Both Start Katran.cmd and the current-user Start Menu link use it.

`runtime/Katran.exe --register` is a headless registration operation. It writes
`%APPDATA%/Microsoft/Windows/Start Menu/Programs/Katran.lnk` with the launcher path,
quoted data-root argument, project working directory, icon and
`System.AppUserModel.ID = LiliaAvelis.Katran`. The build invokes this mode without
opening any window. Registry entries, associations and desktop shortcuts are not
created. A regular installation package is unnecessary for this launcher setup.

Raycast indexes Start Menu applications. Find Katran under Settings → Applications,
assign Record Hotkey, and set Hotkey Action to Always Show if the desired behavior
is activation on every key press. Toggle Visibility is Raycast's default and can
minimize the foreground app instead. Search Scopes can include the Start Menu if
it was previously disabled. Existing shortcuts to target/debug should be replaced.

References: [Raycast Applications](https://manual.raycast.com/applications),
[Raycast search sources](https://manual.raycast.com/search-bar).

## One UI per shelf

Before opening SQLite/capture workers, the UI acquires a session-local named mutex
scoped by the canonical, case-normalized shelf root. The first copy holds it for
its lifetime and creates an auto-reset activation event. The UI window carries
an instance-specific property; a secondary launch locates that window, delegates
foreground permission to its process via AllowSetForegroundWindow, signals the
event and exits without opening SQLite or starting another shelf/capture worker.
An 8-second startup wait covers a primary that has not yet created its window;
timeout is an explicit error, never permission to open a duplicate.

The primary's listener only queues input and wakes egui. The UI client sends
`activate_window` through the same Rust dispatcher as all other commands; its
result is `{activation_requested:true,serial:N}`. The presentation adapter then
shows/restores its own window and requests foreground activation. Shelf contents,
selection, search and preview remain intact. Inputs queue while the exclusive
dispatcher is working; egui's hidden-window logic handles minimized activation.

Windows may deny foreground focus under its normal foreground-lock policy. The
command reports a request, not proof of focus; the adapter logs
`window_activation` with the actual boolean. It does not inject keys, attach input
queues, make the window permanently topmost or steal focus without a launch input.
References: [foreground delegation](https://devblogs.microsoft.com/oldnewthing/20090220-00/?p=19083),
[SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow).

## Verification and limits

Synthetic tests cover repeatable dispatcher activation, unchanged shelf state,
isolated kernel-event wakeups/lifetime cleanup, launcher pointer validation and a
COM shortcut round-trip (AppID and quoted Unicode/spaced arguments). They neither
open real app windows nor focus/minimize existing windows. Registration metadata
is checked headlessly after build. The owner tests: launch from Raycast, switch to
another app and invoke again repeatedly, minimize Katran and invoke, then check
that only the original shelf window/capture process remains.

Close any pre-1.0.5 UI manually once: older binaries do not participate in this
instance protocol. Separate data roots intentionally permit separate UI instances.
The launcher remains tied to this workspace; moving it alone breaks its relative
runtime/data layout. Registration can be refreshed with `--register` after moving
the whole project. Full machine-wide installation and an updater are future work.

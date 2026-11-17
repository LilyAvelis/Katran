# Global tab shortcuts · local 3.8 · 2026-11-17

Later keyboard correction: physical modifier press/release tracking now preserves
Ctrl+Shift/Alt+Shift combinations when event snapshots omit Shift. Parser accepts
standard punctuation/navigation keys including Slash and Comma. Ctrl+Shift+Slash
persists as a physical Win32 OEM chord; parser API 2/2 and recorder 7/7 passed.
See [the combined follow-up](PASTE-IMPLEMENTATION.md). Modifier-only bindings
remain invalid and Windows-reserved combinations can still fail registration.

Settings → Tabs places a recorder and clear control to the right of each eye.
There are no default bindings. `Settings.tab_hotkeys` maps stable `ShelfTab`
values to canonical physical Win32 chords. Win/Ctrl/Alt/Shift plus a letter,
digit, Space or F1–F24 are supported. Disabled tabs retain their assignment but
do not register it or become enabled when invoked.

`SetTabGlobalHotkey` validates and persists through the common settings path;
empty/None clears. Duplicate tab/global-overlay chords reject atomically with
`hotkey_conflict`. SetSettings also validates/canonicalizes the map. A General
draft merges the current map before saving. Persistence acknowledgement is
distinct from native registration: the row displays Active, Inactive, or the
registration error. Windows/other applications may reserve a requested chord.
Local action shortcuts are a separate namespace; assigning the same chord
globally can intentionally intercept that local action while Katran is active.

`SummonTabOverlay` is the API-first show-only route. It rejects disabled tabs
and pending paste jobs, adopts/revalidates an external capture through the
existing overlay session, selects the tab using its saved view and closes
Settings. A same-process foreground invocation preserves the previous external
paste target and always shows the requested tab; it never toggles visibility.
Wire v2 preserves window/process-start u64 values losslessly.

`tab_hotkey_ui.rs` owns presentation and listener lifetimes. Configuration
changes release obsolete tab and overlay registrations before applying new ones;
recording temporarily releases them. Native failures retry after three seconds.
The existing listener uses a dedicated message thread, MOD_NOREPEAT, and
unregisters on Drop. Summon closes transient UI through normal cleanup paths,
clears the old query and refreshes through the common API. This work does not
change Enter delivery or claim receiver-side file-paste compatibility.

Evidence: API 3/3, full UI 250/250, wire v2 5/5; recorder tests include physical
Win press/release and layout-independent capture. A separate native test briefly
registered Ctrl+Alt+Shift+F24, rejected a duplicate, dropped it, then successfully
registered again. It injected no input and moved no windows. The restricted
runner initially denied registration; the authorized native runner passed.
Owner checks still required: exact tab from another application, repeat while
Katran is visible, rebind/clear, occupied key status and disabled-tab behavior.

Settings schema remains v13. An older executable may reject settings containing
the new map; controlled rollback must restore the pre-change settings/profile
backup as well as the runtime pointer. No public release or GitHub push.

Windows contract: [RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey).

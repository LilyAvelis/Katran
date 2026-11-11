# Global summon and paste to the previous application

Settings â†’ ĞÑĞ½Ğ¾Ğ²Ğ½Ñ‹Ğµ â†’ Â«Ğ’Ñ‹Ğ·Ğ¾Ğ² Katran Ğ¿Ğ¾Ğ²ĞµÑ€Ñ… Ğ¾ĞºĞ½Ğ°Â» records a pressed chord through «Èçìåíèòü», with clear/cancel and a visible registered/error state plus «Ïîâòîğèòü». It is empty by default. RegisterHotKey conflicts are reported; Katran does not take a chord away from Raycast. Remove a conflicting assignment there or choose another chord. The application must be running; this feature installs no startup task or background service.

The WM_HOTKEY message thread records the foreground window and its process identity before showing Katran; the dispatcher adopts that receipt, rejecting stale identities/third-party focus while allowing Katran to have become foreground in the meantime. Repeated self-summon never overwrites a saved external target. The hotkey moves Katran's own window to that window's virtual desktop through IVirtualDesktopManager, and centers it in that monitor's work area. Windows can deny these operations; errors are explicit. Katran is raised, not permanently pinned above other applications. With a successfully registered hotkey, Ã— hides the main window while capture and the hotkey stay alive. Actions â†’ Â«Ğ’Ñ‹Ğ¹Ñ‚Ğ¸ Ğ¸Ğ· KatranÂ» exits completely; the hotkey is unavailable until the next launch. Escape hides a summoned window after previews/editors/dialogs are closed.

Select one or several objects and press `Alt Enter`. The dispatcher validates the remembered target, snapshots the selected objects, and starts the existing CopySelectedObjects job. Only a successful complete publication with its exact clipboard sequence authorizes restoring the target and sending Ctrl+V. Failed/partial copy, closed or replaced target, held modifiers, changed Clipboard, or refused foreground activation prevent sending. Recipients decide whether to accept text, images, files, links or table formats; `sent` means Windows accepted the input events, not that the recipient inserted them. Enter retains the existing open action.

Public dispatcher commands:

- `summon_overlay { owner_window }`: remember the external target; returns process-bound target and monitor work area.
- `begin_paste_previous { owner_window }`: validate target and prepare the normal asynchronous copy; returns copying/job_id.
- `complete_paste_previous { owner_window }`: return copying until completion; then send once or report an explicit error. The same prepared session cannot send twice.

`overlay::Host` is injectable for tests. Production validates HWND, PID and process creation time, then checks target foreground and clipboard sequence immediately before SendInput. Windows does not provide an atomic lock covering the last check and injection, so a residual focus race remains. UIPI can block input to elevated applications. This implementation neither requests elevation nor bypasses Windows restrictions.

Synthetic tests cover target replacement, lost foreground, clipboard replacement, modifier hold, failed copy and once-only dispatch. No native windows or personal Clipboard are manipulated by tests. Real multi-monitor/virtual-desktop summon and paste acceptance are performed by the owner.

Global recording temporarily unregisters the existing listener so Windows cannot consume the chord being recorded. On completion/cancel the current binding is registered again. The only settings change from this editor is overlay_hotkey; other unsaved preferences stay in the draft. Native paste acceptance is still owner-operated.

Recovery followup: an inspected disappearance left the resident process alive and had no recent Windows Application Error/WER entry. Same-current-desktop summons skip redundant desktop ID queries; failed summons request showing/focusing the resident window for recovery. Diagnostics explicitly record overlay_shown, overlay_hidden with paste_sent, and overlay_show_failed. These are not claims that an unobserved process crash can never occur.

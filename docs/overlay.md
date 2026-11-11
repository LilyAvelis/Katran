# Global summon and paste to the previous application

Settings → Основные → «Вызов Katran поверх окна» accepts a global chord such as `Alt R` or `Ctrl Shift K`. It is empty by default. RegisterHotKey conflicts are reported; Katran does not take a chord away from Raycast. Remove a conflicting assignment there or choose another chord. The application must be running; this feature installs no startup task or background service.

The hotkey records the foreground window and its process identity before showing Katran, moves Katran's own window to that window's virtual desktop through IVirtualDesktopManager, and centers it in that monitor's work area. Windows can deny these operations; errors are explicit. Katran is raised, not permanently pinned above other applications. With a successfully registered hotkey, × hides the main window while capture and the hotkey stay alive. Actions → «Выйти из Katran» exits completely; the hotkey is unavailable until the next launch. Escape hides a summoned window after previews/editors/dialogs are closed.

Select one or several objects and press `Alt Enter`. The dispatcher validates the remembered target, snapshots the selected objects, and starts the existing CopySelectedObjects job. Only a successful complete publication with its exact clipboard sequence authorizes restoring the target and sending Ctrl+V. Failed/partial copy, closed or replaced target, held modifiers, changed Clipboard, or refused foreground activation prevent sending. Recipients decide whether to accept text, images, files, links or table formats; `sent` means Windows accepted the input events, not that the recipient inserted them. Enter retains the existing open action.

Public dispatcher commands:

- `summon_overlay { owner_window }`: remember the external target; returns process-bound target and monitor work area.
- `begin_paste_previous { owner_window }`: validate target and prepare the normal asynchronous copy; returns copying/job_id.
- `complete_paste_previous { owner_window }`: return copying until completion; then send once or report an explicit error. The same prepared session cannot send twice.

`overlay::Host` is injectable for tests. Production validates HWND, PID and process creation time, then checks target foreground and clipboard sequence immediately before SendInput. Windows does not provide an atomic lock covering the last check and injection, so a residual focus race remains. UIPI can block input to elevated applications. This implementation neither requests elevation nor bypasses Windows restrictions.

Synthetic tests cover target replacement, lost foreground, clipboard replacement, modifier hold, failed copy and once-only dispatch. No native windows or personal Clipboard are manipulated by tests. Real multi-monitor/virtual-desktop summon and paste acceptance are performed by the owner.

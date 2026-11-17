# Kiki acceptance checks · FEATURE-5 · 3.8

## Scope and current state

This is an acceptance checklist for the integrated FEATURE-5 change, not an
implementation audit. The change is integrated at commit
`12360d1ab4e4e9394b09d821e8f4935dcb1f6260` on `codex/katran-3.8`. The exact
staged CLI/UI hashes were checked and the independent CLI smoke is recorded in
[VERIFICATION.md](VERIFICATION.md). Native Windows UI and product acceptance
remain **PENDING OWNER**. This checklist covers per-tab tag sets and safe
legacy migration, the inline live tag editor and persistence, contextual
shortcut behavior, FolderX ranking, Folder-local favorites/history and Windows
Known Folders.

Use synthetic data and an isolated disposable profile for all data-changing
checks. For migration, make a synthetic legacy profile containing differently
styled tags and assignments across multiple content types; do not point the
check at the owner's active profile or personal files. The owner performs
native Windows UI checks. Headless/API evidence is not a native smoke test.

## Acceptance scenarios

| ID | Scenario and steps | Pass condition / evidence |
| --- | --- | --- |
| TAG-1 | In a synthetic profile, configure distinct tag names, colors, and materials for two tabs. Open each tab and inspect tag suggestions and filters; assign a tag to an object in each tab. | Each tab offers only its configured set. Object assignments remain a separate operation and are not changed by editing the available set. Record API results and a native UI observation. |
| TAG-2 | Open a disposable profile created with the pre-change shared-tag schema. Include several tags with non-default style fields, and objects assigned to different tags in different content categories. Run the migration, inspect all tag definitions and assignments, then reopen the profile. | Migration outcome is explicit and documented; no tag, style, or assignment disappears or is silently reassigned. Any cross-tab ambiguity is preserved or surfaced for an explicit decision, never guessed. Values and assignments remain after reopen. Attach before/after synthetic fixture data and API evidence. |
| TAG-3 | In tag settings, select a tag among several rows, open its editor, then select another tag and toggle the editor closed. | The editor opens immediately beneath the selected row, identifies that tag, follows selection correctly, and closes/reopens without editing the wrong row. Capture native UI evidence. |
| TAG-4 | With the editor open, change each supported style field (including color and texture/material), observe both the editor preview and a tagged item in the main interface, close settings, relaunch on the same synthetic profile, and inspect again. | Changes appear live in both places without a Save-then-preview step and survive application restart. API/storage evidence plus owner-observed native UI evidence; a successful build alone does not pass. |
| KEY-1 | Inspect the existing command catalogue and shortcut bindings. Exercise Ctrl+Shift+, in Actions and in tag/settings context; inspect Ctrl+K's current assignment without assuming an unconfirmed change. | Preserve intentional contextual behavior of Ctrl+Shift+,, including the Actions recorder and its tag/settings use; do not create a global binding. No further owner question is needed about this chord. |
| FOLDER-1 | Against the same available Everything index and query settings, capture the current ordered results and paths for `Katran` and `cat run`; then repeat against the integrated ranking and compare. Include Katran's project root and synthetic nested folders such as `.git`, `target`, `node_modules`, `build`, and `cache`. | The project root / strong folder-name matches rise above incidental parent-path matches and technical descendants for both queries. Multiword and separator variants are handled by the general ranker. Report the before/after ordered paths and rank explanations; there is no project-name special case and no fixed top-N candidate cap hiding a relevant folder. |
| FOLDER-2 | Search for a technical folder explicitly (for example, `node_modules` or `.git`) and open it from the results. | Technical/noisy directories rank lower for broad project queries but remain discoverable and openable by a specific query. |
| FOLDER-3 | Inspect the ordinary FolderX ranked results for both sample queries. | Folder AI is deferred. Acceptance at this stage concerns the ordinary algorithmic search only; no model/runtime or network request is required. |
| FOLDER-4 | While already in Folder/FolderX, press Ctrl+F and Ctrl+H. Repeat with focus in a text-editing field and in a non-Folder tab. Also open favorites/history through the existing Ctrl+Shift+K route. | Ctrl+F opens saved favorite folders and Ctrl+H opens folder history directly only in the intended Folder context, while typing/editing and other tabs retain their prior behavior. The existing search entry points still work and use the same data/actions. |
| FOLDER-5 | Open the Ctrl+Shift+K navigation/search UI and discover its visible standard-folders group. Resolve Desktop, Downloads, Documents, Pictures, Videos, and Music and open each. Compare against the Windows Known Folder API result, including at least one synthetic adapter case where paths are relocated/localized. | Every destination comes from the Known Folder API's current path rather than a manually assembled user-profile path. Group is visibly distinct from ranked search results and technical-noise penalties. A shortcut (proposed Ctrl+Y) is accepted only after checking catalogue conflicts, context, and text-editing behavior; visible entry remains discoverable without it. |
| NAV-1 | In the integrated app, exercise existing keyboard navigation around FolderX search, result selection/open, back/forward, and adjacent tabs before and after the new shortcuts. | Existing navigation and selection continue to work; new shortcuts do not become Windows-global hotkeys or intercept text editing. Record the exact context and observed outcomes. |

## Evidence and handoff gate

For code/API acceptance, record the integrated commit SHA, relevant targeted API
checks, and the exact build identity/hash supplied for manual review. Use the
same synthetic profile and index snapshot for before/after comparisons. Do not
call headless checks a native smoke test.

The owner performs the Windows UI acceptance on the runnable build. Do not use
computer-use automation, alter the installed build, access personal data or
clipboard contents, publish, or release as part of this checklist. Report any
shortcut decision still awaiting the owner separately; it must not obscure the
independent acceptance results.

## Isolated release CLI smoke (performed)

The staged CLI and UI hashes matched the supplied values. Only the CLI's
`serve-stdio` was run against a newly created temporary data directory. The UI
was not launched. No source-discovery, clipboard, or capture commands were
sent. Results:

- Images and Text slot definitions differed and remained isolated after closing
  and reopening the CLI on the same profile.
- Folder favorites/history actions returned their intents in Folder and were
  rejected in Images. A synthetic temporary folder appeared in both data lists.
- All six Known Folders resolved; no paths were opened, changed, or recorded.
- `SearchFolders("Katran")`: 100 returned, repository rank 1, incomplete,
  534.9 ms. `SearchFolders("cat run")`: 13 returned, repository rank 1,
  complete, 476.2 ms. Top-result names/categories without absolute paths are in
  [VERIFICATION.md](VERIFICATION.md).

The temporary profile was removed. These API/CLI checks do not constitute
native UI acceptance. Tag editor placement/live rendering, physical shortcuts,
and owner acceptance remain pending; see [VERIFICATION.md](VERIFICATION.md).

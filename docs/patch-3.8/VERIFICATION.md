# Katran 3.8 verification handoff

## Status

Independent CLI smoke was completed against the staged artifacts for
commit `12360d1ab4e4e9394b09d821e8f4935dcb1f6260` on `codex/katran-3.8`.
This is evidence for the commands and queries listed below. Native Windows UI
and product acceptance remain **PENDING OWNER**. Do not describe 3.8 as
accepted or publish it. After this isolated smoke, the owner separately
authorized main integration and primary local installation with existing data;
that later action is recorded in [TEAM.md](TEAM.md). It does not change the
scope of the smoke or imply native acceptance.

## Artifact identity

| Artifact | Staged path | SHA-256 |
| --- | --- | --- |
| CLI | `target/preview-3.8-build-12360d1/katran.exe` | `45c9d3cb934154d4acbcee3a039124dc94ceef30625c49cd11c60b4d257b2f4d` |
| UI executable | `target/preview-3.8-12360d1-portable/portable/runtime/versions/9266bfb24bfd/katran-ui.exe` | `9266bfb24bfdd7afe0f87dd03061119535ada4365dbd5446eed58688af144e99` |
| Portable package ZIP | Staged portable package | `0d13110e1889d50d09a1949e8a15f2b5dee5f741c5ed666277083b0d2f36e985` |

The CLI and UI hashes were checked against the supplied artifact identities.
The CLI was launched only with `serve-stdio` and a fresh temporary synthetic
profile. The UI executable was hash-checked but not launched. The full portable
package contract (schema 13, source inventory, three-file update set) was
reported passed by the integrator; the prior candidate package path was
rejected and is not the deliverable.

## Independent CLI smoke

The isolated profile had Downloads and capture-on-start disabled. No source
discovery, clipboard, capture, or UI commands were sent. The profile and its
synthetic folder were removed after the run.

- Saved different slot-1 definitions for Images and Text, read back each
  definition, then exited and reopened the exact CLI on the same profile.
  Both tab-specific styles/names survived restart and did not appear in the
  other tab's set.
- In Folder context, `RunAction(folder_favorites)` and
  `RunAction(folder_history)` returned their corresponding intents. In Images
  context, both were rejected as `unknown_action`.
- A synthetic temporary directory was added to favorites and history and read
  back from both lists.
- `ListKnownFolders` returned all six entries as resolved. Only volume and
  OneDrive-presence flags were retained below; no user paths were recorded and
  no destination was opened or modified.

| Known Folder | Result | Volume | Under OneDrive |
| --- | --- | --- | --- |
| Desktop | Resolved | C: | Yes |
| Downloads | Resolved | C: | No |
| Documents | Resolved | C: | Yes |
| Pictures | Resolved | C: | Yes |
| Videos | Resolved | D: | No |
| Music | Resolved | D: | No |

## Everything ranking snapshot

Two `SearchFolders` requests were sent to the exact staged CLI against the
existing local Everything index, limit 100. Times are single local request to
response observations, not benchmarks. Top results below show basenames and
categories only; absolute paths are intentionally omitted.

| Query | Returned | Repository rank | Incomplete | Time | Top-result summary |
| --- | ---: | ---: | --- | ---: | --- |
| `Katran` | 100 | **1** | Yes | 534.9 ms | First two basenames were `Katran` (repository first); next three were recovery-noise directories. |
| `cat run` | 13 | **1** | No | 476.2 ms | First two basenames were `Katran` (repository first); next results were Visual Studio runtime directories. |

For `Katran`, the result limit is full and the API marks acquisition incomplete;
the 100 returned items are not a complete index count. `cat run` returned 13
items with no stream reported truncated. Compared with the earlier debug
snapshot in [SEARCH.md](SEARCH.md), where `cat run` ranked the repository 12th,
the staged artifact ranked it first. This is one local index snapshot, not a
general recall guarantee.

## Checks not performed by this smoke

- Native tag editor placement, live rendering in the main interface, and
  keyboard behavior.
- Physical Ctrl+F / Ctrl+H, Ctrl+Shift+K, and contextual Ctrl+Shift+, input.
- Opening a real Known Folder destination or changing any user data.
- Manual Windows product acceptance by Lilia.

These remain pending. API results and successful packaging do not substitute
for the owner's native UI review.

## Native-feedback follow-up: local tag editor and Ctrl+Y

The owner confirmed that live styling works but clarified the palette editor
contract and requested a configurable Standard-folders shortcut. The following
targeted evidence covers the follow-up implementation, not native acceptance:

- API: `folder_standard_action` and the shared catalogue each passed 1/1.
  The new action is Folder-only, defaults to Ctrl+Y and persists a changed
  binding across restart. Shortcut policy tests passed 4/4, including preservation
  of an older user-defined Ctrl+Y assignment.
- Headless UI: the aggregate run passed 220 cases and initially failed three.
  Two new menu fixtures needed a real key release between presses and enough
  idle frames for egui sizing; their actual control/absence assertions were
  retained. The corrected tag-menu module passed 7/7, including the full shell
  route, rendered Name control, absence of preview/footer, captured-tab
  persistence, failed draft retention and Escape back-navigation.
- The Standard-folders UI test passed custom binding from Folder and an open
  picker, removal of the previous key after rebinding, and retention of Ctrl+Y
  by the focused query field. Settings accordion/live-save checks and the
  existing Actions shortcut recorder also passed in the aggregate run.
- The unrelated Windows synthetic cut/paste test reported access denied once
  in the parallel aggregate. Its isolated, single-thread rerun passed 1/1;
  the cause of the parallel sensitivity was not established. No source change
  or real clipboard/user-file operation was made to address that fixture.
- Final Clippy all-targets (`-D warnings`), formatting and whitespace checks
  passed. Schema v13, dependency lock, model/runtime, media and broader storage
  contracts are unchanged; their previous evidence was not rerun.

The primary local-main installation uses one newly built UI executable after
these checks. Its exact source checkpoint, hash and previous runtime are recorded
in the follow-up activation record in `TEAM.md` and the ignored runtime receipt.
Native owner acceptance of the new palette and physical keys remains pending.

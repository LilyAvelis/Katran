# Katran 3.8 · FEATURE-5 / Мастерская + Iris

Owner instruction: 2026-11-17. Preparation and implementation only; no public release, push, or replacement of the installed application. Product contract: [PRODUCT.md](PRODUCT.md).

Lead chat: `01a1ea4b-8cbb-7bf2-9d11-2efe604474d7`, runtime `/root` (current session; no separate Lead spawned). Baseline `c7c0b8e`, Katran 3.7.1, schema v12. Existing untracked user files are outside this change.

## Runtime roster

These are real canonical runtime session identifiers returned by spawn_agent. Reuse them with followup_task; do not spawn replacements. Completed sessions retain their handoff, not permanent persona memory.

| Persona | Runtime ID | Requested model / effort | Ownership | State / next step |
| --- | --- | --- | --- | --- |
| Polly / Appolion | `/root` | current lead session | `api.rs`, `lib.rs`, version/docs, integration and verification | Integrate agreed domain contracts; maintain this map |
| Fenna Ferrule | `/root/fenna` | `gpt-6.1-terra` / high | tag settings/material/menu/shelf UI, workspace picker, contextual actions/shortcuts/icons and UI tests | Implementation and focused fixes complete; UI 220 passed; source frozen |
| Mira Socket | `/root/mira` | `gpt-6.1-terra` / high | `everything.rs`, `known_folders.rs`, adapter tests | Implementation complete; real index syntax verified; final API ranking snapshot pending |
| Nemi Index | `/root/nemi` | `gpt-6.1-terra` / high | tag slots/service, numbered schema, scoped browse/pin cleanup, `folder_search_rank.rs`, tag/migration tests | Implementation and scoped migration checks complete; word-boundary ranking fix verified |
| Iris Vale | `/root/iris` | `gpt-6.1-sol` / low (= requested light) | UX contract only; no parallel UI edits | Acknowledged and delivered handoff to Fenna; reuse for targeted review |
| Kiki Click | `/root/kiki` | `gpt-6-luna` / high | independent acceptance scenarios and integrated-result verification | Acknowledged; `KIKI-CHECKS.md` prepared; final verification requires concrete integrated SHA/artifact |

All five requested models successfully launched. The runtime allows root plus three simultaneously working agents; initial Iris/Kiki starts returned `agent thread limit reached`, then succeeded after reconnaissance tasks completed. Keep the five existing sessions and schedule work within this concurrency bound. No persona/model substitution occurred. Garden CLI used its local snapshot after network failure; all five full cards were subsequently fetched through connected Notion and stored in `legion/`.

Project session inventory: earlier chats `Katran 3.7` (`01a1db19-4256-7f33-aa64-ecb61c5f5f57`) and `Katran 3.4` (`01a1ce3d-72a0-7e30-9209-3c245a393a3d`) are not loaded. Runtime inventory at start exposed only `/root`; no resumable FEATURE-5 worker sessions existed in this team tree.

## Agreed boundaries

- Mira acquires/deduplicates candidates; Nemi owns pure `rank_folders(query, &mut [EverythingFolder])`; final display limit follows ranking. Bounded acquisition must expose incompleteness. No project-name hardcode, cloud requests, or AI requirement.
- Tags: both definitions and assignments are scoped by tab. Migration preserves v12 definitions, styling and links in every tag-capable context; subsequent changes are independent. Nemi owns schema and all SQL changes; Polly owns public enum/dispatcher integration. Synthetic old-profile fixture only.
- Fenna owns UI after domain behavior is verified; Iris supplies UX contract without separate redesign. Kiki independently checks the integrated result. Native Windows acceptance belongs to Lilia; no desktop automation authorized.
- User clarification: `Ctrl+Shift+,` is intentionally contextual; preserve Actions recording and existing tag/settings context. Folder AI deferred until ordinary search is improved and assessed. `Ctrl+Y` remains a proposed contextual key, not final approval.

## Evidence / open boundaries

- Domain checks: library 150 passed / 1 ignored; catalogue snapshots 4, Desktop shelf 5, hardening 10, Known Folders API 1 passed. Tag storage 3 and scoped migration 2 passed; tag API 7 passed after correcting a synthetic fixture to assign in its intended tab. The migration suite includes rollback/retry after an injected synthetic schema collision. Folder UI and final integrated build checks are still pending.
- Everything initially failed with Windows error 623 in restricted execution. Read-only elevated execution succeeded. Live verification caught a real installed-version syntax mismatch (`nopath:` works, `no-path:` did not); fixed before the candidate build. See `SEARCH.md` for the original and intermediate ranked API snapshots. The latest word-boundary fix still needs its final artifact snapshot.
- Installed runtime and personal data remain unchanged; deliver an isolated runnable artifact with source identity for manual verification later.
- Local branch: `codex/katran-3.8`. First compile exposed a ranker closure type mismatch; fixed before test execution. Later test failures were fixture assumptions about disabled Text and formerly global assignments; no failed check is counted as passed.
- Final integration coverage: all 116 integration targets accounted for, 379 passed and 6 intentionally ignored. Library 151 passed / 1 ignored before the final ranking refinement; the affected ranker 2/2 and Everything adapter 6/6 passed afterwards. UI 220/220 plus 15 focused tag UI regressions passed. Launcher 4/4 passed. Clippy all-targets with `-D warnings` passed; the synthetic package contract passed. Dependency notices for 302 packages were generated before release compilation. Native visual acceptance remains open.

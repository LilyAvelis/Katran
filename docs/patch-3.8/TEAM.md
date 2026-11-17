# Katran 3.8 · FEATURE-5 / Мастерская + Iris

Initial owner instruction: 2026-11-17, preparation and implementation without publication. The later workflow correction authorizes local main integration and primary installation with existing data; see the activation record below. No public release or push. Product contract: [PRODUCT.md](PRODUCT.md).

Lead chat: `01a1ea4b-8cbb-7bf2-9d11-2efe604474d7`, runtime `/root` (current session; no separate Lead spawned). Baseline `c7c0b8e`, Katran 3.7.1, schema v12. Existing untracked user files are outside this change.

## Runtime roster

These are real canonical runtime session identifiers returned by spawn_agent. Reuse them with followup_task; do not spawn replacements. Completed sessions retain their handoff, not permanent persona memory.

| Persona | Runtime ID | Requested model / effort | Ownership | State / next step |
| --- | --- | --- | --- | --- |
| Polly / Appolion | `/root` | current lead session | `api.rs`, `lib.rs`, version/docs, integration and verification | Integrate agreed domain contracts; maintain this map |
| Fenna Ferrule | `/root/fenna` | `gpt-6.1-terra` / high | tag settings/material/menu/shelf UI, workspace picker, contextual actions/shortcuts/icons and UI tests | Implementation and focused fixes complete; UI 220 passed; source frozen |
| Mira Socket | `/root/mira` | `gpt-6.1-terra` / high | `everything.rs`, `known_folders.rs`, adapter tests | Implementation complete; real index syntax verified; final artifact search independently checked by Kiki |
| Nemi Index | `/root/nemi` | `gpt-6.1-terra` / high | tag slots/service, numbered schema, scoped browse/pin cleanup, `folder_search_rank.rs`, tag/migration tests | Implementation and scoped migration checks complete; word-boundary ranking fix verified |
| Iris Vale | `/root/iris` | `gpt-6.1-sol` / low (= requested light) | UX contract only; no parallel UI edits | Acknowledged and delivered handoff to Fenna; reuse for targeted review |
| Kiki Click | `/root/kiki` | `gpt-6-luna` / high | independent acceptance scenarios and integrated-result verification | Exact release hashes and isolated two-process CLI smoke passed; native owner acceptance pending |

All five requested models successfully launched. The runtime allows root plus three simultaneously working agents; initial Iris/Kiki starts returned `agent thread limit reached`, then succeeded after reconnaissance tasks completed. Keep the five existing sessions and schedule work within this concurrency bound. No persona/model substitution occurred. Garden CLI used its local snapshot after network failure; all five full cards were subsequently fetched through connected Notion and stored in `legion/`.

Project session inventory: earlier chats `Katran 3.7` (`01a1db19-4256-7f33-aa64-ecb61c5f5f57`) and `Katran 3.4` (`01a1ce3d-72a0-7e30-9209-3c245a393a3d`) are not loaded. Runtime inventory at start exposed only `/root`; no resumable FEATURE-5 worker sessions existed in this team tree.

## Agreed boundaries

- Mira acquires/deduplicates candidates; Nemi owns pure `rank_folders(query, &mut [EverythingFolder])`; final display limit follows ranking. Bounded acquisition must expose incompleteness. No project-name hardcode, cloud requests, or AI requirement.
- Tags: both definitions and assignments are scoped by tab. Migration preserves v12 definitions, styling and links in every tag-capable context; subsequent changes are independent. Nemi owns schema and all SQL changes; Polly owns public enum/dispatcher integration. Synthetic old-profile fixture only.
- Fenna owns UI after domain behavior is verified; Iris supplies UX contract without separate redesign. Kiki independently checks the integrated result. Native Windows acceptance belongs to Lilia; no desktop automation authorized.
- User clarification: `Ctrl+Shift+,` is intentionally contextual; preserve Actions recording and existing tag/settings context. Folder AI deferred until ordinary search is improved and assessed. `Ctrl+Y` remains a proposed contextual key, not final approval.

## Evidence / open boundaries

- Domain checks: library 150 passed / 1 ignored; catalogue snapshots 4, Desktop shelf 5, hardening 10, Known Folders API 1 passed. Tag storage 3 and scoped migration 2 passed; tag API 7 passed after correcting a synthetic fixture to assign in its intended tab. The migration suite includes rollback/retry after an injected synthetic schema collision. Folder UI and final integrated build checks are still pending.
- Everything initially failed with Windows error 623 in restricted execution. Read-only elevated execution succeeded. Live verification caught a real installed-version syntax mismatch (`nopath:` works, `no-path:` did not); fixed before the candidate build. See `SEARCH.md` for original, intermediate and final exact-artifact API snapshots. The latest word-boundary fix was independently verified in that final snapshot.
- Installed runtime and personal data remain unchanged; deliver an isolated runnable artifact with source identity for manual verification later.
- Local branch: `codex/katran-3.8`. First compile exposed a ranker closure type mismatch; fixed before test execution. Later test failures were fixture assumptions about disabled Text and formerly global assignments; no failed check is counted as passed.
- Final integration coverage: all 116 integration targets accounted for, 379 passed and 6 intentionally ignored. Library 151 passed / 1 ignored before the final ranking refinement; the affected ranker 2/2 and Everything adapter 6/6 passed afterwards. UI 220/220 plus 15 focused tag UI regressions passed. Launcher 4/4 passed. Clippy all-targets with `-D warnings` passed; the synthetic package contract passed. Dependency notices for 302 packages were generated before release compilation. Native visual acceptance remains open.

## Candidate artifact

- Source commit: `12360d1ab4e4e9394b09d821e8f4935dcb1f6260`.
- One optimized build completed in 1m37s using the existing `target/build`
  cache, Rust/Cargo 1.95.0, Windows x64, default features plus
  `ui,clipboard-events`, without the optional bundled Apple resource.
- Input hashes were captured before compilation and matched afterwards.
  The producer receipt and executable hashes are in
  `target/preview-3.8-build-12360d1/receipt.json`.
- Complete Portable staging directory:
  `target/preview-3.8-12360d1-portable/portable/`.
  Launch its `Katran.exe`; the portable flag gives it a separate local profile.
  The installed launcher remains on `d7eae2862b31`.
- UI SHA-256: `9266bfb24bfdd7afe0f87dd03061119535ada4365dbd5446eed58688af144e99`.
- CLI SHA-256: `45c9d3cb934154d4acbcee3a039124dc94ceef30625c49cd11c60b4d257b2f4d`.
- Portable ZIP SHA-256:
  `0d13110e1889d50d09a1949e8a15f2b5dee5f741c5ed666277083b0d2f36e985`.
- Actual package contract passed: helper/notices, schema v13 and the existing
  three-file updater format. The optional model and FFmpeg runtime are not in
  this local preview package. No installer was run, no tag/release created,
  and nothing was pushed.
- The earlier `target/preview-3.8-12360d1/` directory is an incomplete rejected
  packaging attempt, not the candidate. Packaging expected an empty string
  rather than null for the absent optional Apple digest; correcting that
  receipt metadata and packaging to a fresh directory reused the same
  already verified binaries. No recompilation occurred.
- Kiki independently confirmed the exact UI/CLI hashes, scoped tag style/name
  persistence across two CLI processes, Folder action contexts, synthetic
  favorites/history and six read-only OS Known Folder resolutions. Both real
  `Katran` and `cat run` searches ranked this repository first. Details are
  recorded separately in `VERIFICATION.md`. Native UI acceptance belongs
  to Lilia and is not implied by any API or headless result.
- Owner reminder: native acceptance does not itself authorize a GitHub push,
  tag or public release. Primary local installation was explicitly authorized
  by the later correction recorded below.

## Primary launch staging · owner correction 2026-11-17

The owner rejected the empty-profile preview as the normal acceptance route and
instructed Polly to integrate local main and install to the primary Raycast app
with existing data. This supersedes the initial no-install restriction.

- Verified the actual Start Menu/Raycast link points to `runtime/Katran.exe`
  with the existing repository `katran-data` profile.
- Created a consistent SQLite backup of schema v12 with SQLite's online backup
  API and verified its integrity. Exact private paths and pointer backup are
  recorded locally in ignored `runtime/rollback-3.8.json`.
- Copied the already verified runtime and notices to
  `runtime/versions/9266bfb24bfd`, verified every copied file hash, and atomically
  changed the primary launch pointer from `d7eae2862b31` to `9266bfb24bfd`.
  No compilation or shortcut replacement was needed. The materialized Apple
  font remains in the existing profile; optional local runtimes are unchanged.
- Previous runtime and pointer backup remain available. Full rollback after
  migration requires stopping Katran and restoring the pre-v13 database, not
  merely changing the executable pointer.
- The existing 3.7.1 process was left running. The owner must fully exit it once
  through the tray and use normal Raycast launch for 3.8 activation. Migration
  is intentionally deferred to that startup to avoid concurrent old/new writers.
- Native owner acceptance remains pending. Nothing was pushed to GitHub.

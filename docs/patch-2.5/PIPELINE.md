# Katran 2.5 — local integration pipeline

2026-11-11. Walter's execution order; this is a plan, not evidence of completed implementation or checks. Owner authorized autonomous local implementation and Git checkpoints, with no GitHub push. Windows GUI acceptance remains owner-operated.

## Checkpoint 0 — preserve the starting tree

Polly inspects and commits the existing dirty product files listed in TEAM.md and the onboarding documents as a clearly named baseline checkpoint. Preserve existing work; exclude credentials, personal data and runtime exports. Record the actual SHA, not an assumed clean 2.0 baseline. No build is required just to save this checkpoint.

## P1 — useful Folder vertical slice, safety first

1. Settle the small shared contracts before parallel edits: event/revision-bound external pin that survives projection disappearance; transfer intent and per-item receipts; source/view generation; authority immediately before an effect. Polly owns API/dispatcher/schema/catalogue integration. Nemi owns an isolated persistence module; Mira owns an isolated transfer/native boundary; Fenna owns navigation/domain state. Three worker slots are the limit; reuse existing sessions.
2. Integrate additive persistence and protection first. Pin/restart works; external replacement leaves the original pin stale/unavailable and does not transfer it to new content. Protect the same pinned real file through Folder aliases. Pin/unpin and source changes must serialize with an authorized destructive effect. Preserve v6 backup and reject future schemas.
3. Implement nonrecursive live Folder API and selection invalidation, explicit folder selection, Everything adapter with bounded requests and explicit unavailable state. Keep current folder session-only. Establish copy/materialization with create-new destinations, verified content and per-item receipts before enabling any source removal. Handle same-folder/conflicts explicitly.
4. Enable move/delete only for paths whose identity and pin barriers are proven. Ctrl+X itself has no effect. Pinned is copy-only. A verified destination followed by source-removal failure reports destination-created/source-not-removed and preserves the destination. Recycle is mandatory for managed-folder Delete; no permanent fallback. Existing pathname-based Shell recycle is not identity-atomic: resolve it or fail closed with a visible limitation. Do not hold the runnable slice hostage to an unsafe adapter.
5. After synthetic domain checks pass, connect a thin GUI: Folder replaces All, folder search/selection, browse/preview, copy/cut/paste and visible partial outcomes; pin toggle and category-local pinned view. Add source/view generation filtering so old loads cannot replace the current view. Verify ordinary Folder copy suppression and explicit Add to Katran through the API before GUI binding.

**P1 checkpoint and runnable artifact:** commit the integrated slice, run the affected API suites plus the required compile/check gate, then Rhea builds one immutable runnable artifact with source SHA/input hashes. Give Lilia its exact launcher/path and a short synthetic-folder checklist: select Folder, copy, cut with pinned mixed selection, conflict/partial error, external file change, return import. List any fail-closed move/delete limitation explicitly. This early feedback build does not claim full 2.5 completion. No installer, release packaging or publication is needed.

**Minimum P1 safety gates:** synthetic migration/restart and committed-WAL backup; external same-path replacement; pin after preparation; Folder alias; source replacement before effect; no-overwrite/same-folder; destination verification failure; destination-created/source-not-removed; mixed pins/shared blobs. Review only the destructive boundary and its race tests with Walter. Tests never touch personal originals or real clipboard contents.

## P2 — complete the working workflow

Finish representations (TXT/MD, CSV, grouped bookmarks.html and mixed batches), intentional return import and self-capture suppression, folder history/favorites and confirmed history clearing. Complete managed-file recycle/move if P1 deliberately failed closed. A grouped output must verify every participating input before any source deletion. Receipts and retry behavior must prevent silent duplication after a partial result.

Commit the coherent domain changes, run only newly affected API tests, then GUI-bind verified behavior. Use Kiki's existing scenarios for owner acceptance, including browser bookmark import. Refresh the runnable artifact only after a product-code change warrants it; reuse the P1 artifact for unchanged behavior.

## P3 — persistent properties and tags, then responsiveness

Add display name/opener and category-local tags with one active filter. Keep display names separate from external filenames/URLs. Choose and document non-destructive unpin metadata retention. Complete reset/navigation rules, favorite/history edge cases and preview invalidation. Nemi/Fenna/Vela work on disjoint files; Polly integrates shared edits.

Gate: properties/tags survive restart, category/filter transitions cannot leak stale selections/results, external originals remain unchanged. Pax measures the deterministic browse/switch/search harness once matching behavior exists; optimize demonstrated regressions, not speculative architecture. Commit this stage separately.

## P4 — low-priority dates, Raycast decision and polish

Preserve own-object ingest timestamps through late materialization without rewriting external timestamps or shared blob metadata. Investigate actual Raycast clipboard markers; implement filtering only with reliable evidence, otherwise document a concrete compromise for Lilia. Do not silently ban single emoji. Finish cosmetic labels/icons/layout after the workflow is usable.

Gate: targeted timestamp/format checks and a bounded regression check of the accepted 2.0 flows affected by changes. Record unresolved product limitations. Final local checkpoint and runnable build are separate from owner acceptance and any future public release authorization.

## Execution discipline

Rhea serializes Cargo/build work in existing target/build. Reuse passed checks for unchanged inputs and save SHA, lockfile/toolchain/features/profile and artifact SHA256. Before any expensive retry name changed input, invalidated evidence, minimum stage and retained result. After two failures of one stage diagnose first. No duplicate clean builds, cache clearing, minute-by-minute agent polling, CI changes or new services for this work.

While compilation runs, workers perform independent scoped implementation/docs or stop; an idle persona needs no replacement agent. Lune keeps TEAM/HANDOFF aligned with actual checkpoints, active sessions, blockers and the next runnable artifact. Native Windows acceptance is pending until Lilia reports it. SQLite rollback requires stopped writers and a coherent backup restore; it does not restore blobs or originals.

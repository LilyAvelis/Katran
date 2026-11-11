# Katran 2.5 — onboarding findings

2026-11-11. Planning and source inspection only; no implementation or test success claimed.

## Fenna — application and API

- State/navigation: src/interaction.rs, src/tab_service.rs, src/tab_layout.rs, src/settings.rs. Dispatcher: src/api.rs, src/lib.rs. Query: src/shelf_tabs.rs, src/catalogue_query.rs. Clients: src/bin/tab_bar.rs, src/bin/ui.rs, src/bin/api_client.rs, src/bin/shelf_view.rs.
- Introduce Folder plus category-local stream/pinned/one-tag state; same-number toggles without timer, leaving category resets slice/tag. Folder context is session-only. Invalidate browse cursors and selection when context changes; current visible() does not itself prune selected IDs.
- Pin protection belongs to domain delete/cut and background authorization, not GUI filtering. Relevant src/delete_selection.rs and delete_jobs.
- First bounded ticket: navigation state machine and synthetic state tests; shared enum/dispatcher integration remains Polly-owned.

## Mira — Windows and transfer

- Existing self-clipboard sequence suppression in src/capture.rs and receipts in jobs_service/text/table services are starting points, not proof of all Folder race behavior.
- Existing src/folder_watch.rs canonicalization/revisions/ReadDirectoryChangesW and source_runtime.rs watchers can be adapted; Folder must remain a current nonrecursive listing, not a second historical import stream.
- src/downloads.rs NativeTrash/IFileOperation is currently Downloads-scoped; src/delete_jobs.rs permanently deletes ShareX originals. Patch 2.5 requires recycling managed-folder deletes.
- Ctrl+X records intent only. Create destination without overwrite; verify contents; revalidate identity/revision and pin before source removal. Pinned always copy. Report per-item copied/moved/pinned-copy/error and destination-created/source-not-removed separately. No batch atomicity claim. Grouped bookmarks requires verification before deleting any participating link. Handle same-folder explicitly. Path replacement TOCTOU needs safe handles/identity or fail-closed implementation.
- Existing Everything approach located by Mira in neighboring Afiocira/crates/photo-gui/src/everything.rs and search.rs, docs/everything.md: bounded helper/IPC, timeout, output cap, explicit absence; inspect provenance/license before reuse, keep Katran independent.
- Raycast emoji source marker is not established; do not ban emoji globally.
- First ticket: synthetic no-overwrite copy/verification/error adapter, no source deletion until reviewed.

## Nemi — persistence

- SQLite index.sqlite3, WAL/FK on open, schema VERSION=6, pre-migration VACUUM INTO backup and transactional migration already exist (src/catalogue_schema.rs, src/lib.rs).
- Candidate additive v7: event properties (pin/display name/opener), tags, separate history/favorites. Current folder never persisted; favorites survive history clearing. One category and one active tag constrain queries.
- Blocking identity issue: src/folder_commit.rs can replace event ID when same-path content changes. Do not finalize an event_id-only external-pin design or silently attach a pin to replacement content. Coordinate with Mira/Fenna.
- Existing detected_at_ms can supply owned-object ingest time; avoid redundant time columns and changing shared deduplicated blob mtime for distinct events. External file timestamps remain untouched.
- Unpin metadata/tag retention, opener descriptor, path normalization and external-pin continuity need explicit implementation contracts. Avoid destructive metadata policy by accident.
- Rollback means a stopped writer and coherent pre-v7 backup restoration; post-backup writes would be lost, and SQLite restore does not restore blobs or external originals. Never run the old binary directly on v7 as a rollback procedure.
- First ticket: isolated persistence module and synthetic v6 fixture/migration/restart tests after identity contract; shared schema/query edits stay with Polly.

## Vela — preview and cache

- Image IDs, edge handling and content revisions are present. Stale-decode
  filtering is present; asynchronous list loading is not.
- Folder needs a source namespace and view generation so results from an old
  source or view cannot be treated as current.
- Read-only onboarding finding; no implementation or verification claimed.

## Rhea — baseline

- Reported baseline: HEAD `7e2a32ef3a7531d189b84c012ec2cbb0e5999ac4`,
  Rust/Cargo `1.95`, `Cargo.lock` SHA-256
  `bfa3e393e445b5d1be2839106444043424dbfde08cc090adc41991d47683cc10`.
- Historical runtime 2.0 artifact directory `62ac72c82f87` (executable SHA-256 `62ac72c82f877a4810235e646ad041c4a96711b42fdc2cc34b2f5538c4585ab2`) is not verification
  of the dirty 2.5 working tree. No build or test success is claimed.

## Pax — measurement

- `tests/latency.rs` already has an opt-in synthetic harness: 100/1,000 events
  and 100 files/watch at 1,100 events. Historical `docs/latency.md` and
  `reviews/latency-1.3-hardening` / `azuros-1.3-readiness` are not a 2.5
  baseline. Pax ran no new measurements.
- First measurement ticket: after API behavior exists, build a dedicated
  deterministic browse/search/stream-pinned-Folder harness; report cold/warm
  p50/p95/max against matching input fingerprints. Coordinate Cargo execution
  with Rhea.

## Continuation state

- Fenna, Mira, Nemi, Vela and Rhea completed read-only onboarding and can take
  targeted followup; this does not mean they are concurrently active.
- Kiki (`/root/kiki`) completed onboarding with 11 scenarios. Coverage includes
  move/pin, partial outcomes, same-folder operations, re-copy without capture,
  and category/tag behavior. Synthetic API cases are distinct from owner-run
  GUI checks; no tests or code changes were made by Kiki.
- Pax (`/root/pax`, Luna high) completed read-only onboarding. Walter
  (`/root/walter`, Sol low) completed the bounded read-only safety review. All nine
  sessions exist; four concurrent slots include Polly, so they are not all
  active at once.
- Inspect the dirty files listed in
  `TEAM.md` before implementation; Polly retains shared API/dispatcher/schema
  and catalogue edits.

## Integration order

1. Settle external-object identity, transfer receipts and pin revalidation; Walter reviews original-file safety.
2. Domain state/persistence/transfer contracts and synthetic API verification.
3. GUI clients and preview invalidation after domain behavior is verified.
4. Rhea serializes targeted checks and artifact builds; Kiki's scenarios guide owner-operated acceptance; Pax measures matching inputs.

This is sufficient to resume work and negotiate the main contracts. Inspect the dirty tree and resolve external
object identity, transfer receipts, pin revalidation, and Folder source/view
generations first.

No public push, release or permission to modify personal files inferred from onboarding.

## Walter — safety review and first integration ticket

All nine assigned sessions completed onboarding. No product code, tests, builds, GUI or personal data operations were performed during it.

- Proposed minimal identity policy (not yet implemented): keep external pin/metadata on its original event/revision, show stale/unavailable after external replacement, and do not let a new same-path event inherit pin. Pinned metadata must survive projection disappearance.
- Concrete current recycle limitation: src/delete_jobs.rs releases LockedOriginal before Shell recycle; src/downloads.rs invokes trash by pathname after a separate hash check. A pathname replacement between verification and effect is possible. Simply switching ShareX to recycle does not close this gap. Review identity-bound recycle or fail closed; never use permanent deletion as fallback.
- Pin/source-generation revalidation needs a serialization barrier or lease covering the authorized effect, including background jobs. A second SELECT without that barrier is insufficient.
- Folder has no pin feature, but may address the very same real file pinned in Downloads/Screenshots. Resolve alias protection by real file identity so Folder deletion cannot bypass the pin.
- Per-item transfer receipts distinguish copied/moved/pinned-copy/skipped-pinned and destination-created/source-not-removed, including destination path/reason. Preserve verified copies after source-removal failure; retries must not silently duplicate them. Delete a shared owned blob only after all event references are gone.
- Synthetic race checks: external rewrite/replace after pin; pin after preparation; source replacement before effect; same-folder/conflict/verification failure; partial receipt; shared blob and Folder alias; migration failure/restart/WAL backup and coherent restore.
- First integration ticket: Mira and Nemi define one short PreparedTransfer/authorized-effect/per-item-receipt contract covering identity, pin barrier, aliasing and recycle failure. Polly integrates shared API/schema; Walter reviews that bounded contract and race evidence before enabling destructive adapters.

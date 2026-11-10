# 1.5 domain contracts

Checkpoint before changes: `585df0c` (1.2.6). The owner accepted this work as1.5.0 and authorized its GitHub push. Initial audit:
[azuros-1.3-readiness.md](reviews/azuros-1.3-readiness.md).

## Jobs and clients

`BeginJob {command}` returns either immediate `{state:"succeeded",result}` or
pending `{job_id,state:"queued"}`. `PollJob {job_id}` returns a typed JobStatus;
its outer `ok:true` only means the status request worked. Terminal states are
succeeded/failed/cancelled. Errors retain code/message. `CancelJob` cannot undo an
external effect: the atomic effect fence returns job_not_cancellable once won.
Unknown/expired IDs are explicit errors. Immediate commands need no polling.

One dispatcher keeps interaction/settings/selection available. Workers receive
owned inputs and private receipts, never Service or its SQLite connection.
Foreground/capture/discovery lanes reserve8/2/2 outstanding slots and one lazy
worker per lane; a claimed result retains capacity until its commit completes.
Completed status retention is64; bounded completion polling favors F/C/F/D.
Preparation cancellation drops owned staging, and cannot create unbounded retired
workers. Continued deletion stages retain their ID, admission and effect fence.

GUI has separate foreground, capture and startup-warmup job tracking. Foreground
preview/export does not gate background capture. Listener signals coalesce the
current Clipboard snapshot; this is not a lossless Clipboard history. Capture
still requires the process to run, and never falls back to legacy polling after a
listener failure. The owner accepted event capture as the default previously.

Bytes/reference hashing, PNG encoding, durable staging, independent copy files,
clipboard pixel encoding and drag visuals prepare off-thread. A private cache
receipt commits quota/metadata and a no-overwrite publication. Concurrent private
receipts for the same digest create distinct arrivals and one cache object.
The synchronous API uses the same storage and domain helpers.

Selection checks preserve exact IDs/revisions. Copy and drag publish immutable
owned snapshots. Save/Import hashes the bytes actually exported against the
recorded revision, returns per-item source_changed on changes and removes its
partial output. It never silently saves newer bytes under an older arrival.

## Sources and authority

`EnsureSources` starts the domain source runtime. `PollSources {limit:1..8}`
consumes bounded private worker receipts; `SourceStatus` reports enabled/root/
generation/state/error/backlog/worker_count. `RescanSource {source_id}` explicitly
retries or rescans. Legacy ScanSharex/ScanDownloads now schedule that same runtime;
they do not synchronously return completed enumeration. Settings alone persist
and revoke configuration authority; they start no filesystem workers until an
explicit Ensure/Poll or an already-running runtime reconfiguration.

Configuration generations persist, including A→B→A. SetSettings synchronizes them
before returning; late receipts cannot regain old authority. Each source has at
most two active/retired workers total; disabled/failing sources are explicit.
Discovery/hash/complete availability snapshots occur on native-notification
workers. Failed/truncated snapshots cannot mark unseen files missing. No scan
runs in a visual component or a second CLI ingestion implementation.

`Event.delete_policy` is shelf_only or live_original{source_id,generation,mode}.
A provenance string never grants destructive authority. Generic IngestPath is
shelf-only even if source="sharex". Only verified private source receipts grant
live membership; they cannot convert a newly-created generic shelf event into an
original. Public CommitFolderCandidates/CommitScreenshotMatches reject raw claims
with untrusted_worker_result. Projection membership and current generation must
still match immediately before deletion authorization.

Source receipts survive shelf record removal to prevent unchanged resurrection;
a later content revision/revert gets a fresh ID. Projection availability hides
missing originals without erasing arrival history. Browse uses indexed SQL
filter/search/cursor before LIMIT; it does not materialize/stat unrelated history.
Pixel classification uses a persistent continuing queue and indexed digests.
Identical sources do not generate a clips×sources table; accepted digests remain
valid for the current configured generation after original removal. Startup
recovers inflight work even when schema is already current. Transactional counters
keep idle classification polling independent of completed history.

## Deletion and native boundaries

Single deletion proceeds immediately; a group first returns confirmation_required
and the exact ID snapshot. Enter confirms in GUI. Preparation verifies originals
off-thread. Actor authorization rechecks ID/revision/policy/current generation;
that is the explicit point of no return. Changing settings afterwards does not
undo an action already authorized. Failures are per-item, including missing IDs,
changed objects and recycle errors; nested cleanup warnings reach the GUI.

ShareX permanent removal hashes and retains the same deny-write/delete Windows
handle through authorization and disposition, preventing pathname replacement.
Downloads uses an injectable recycler and no permanent fallback. Shell recycling
is a pathname API: it revalidates bytes immediately before Shell, but cannot offer
an atomic file-identity transaction against hostile concurrent pathname mutation.
Tests use a fake recycler, never the owner's actual files or Recycle Bin.

Shell thumbnails/icons use a bounded headless helper process. Both pipe write and
read/extraction are under a hard deadline; timeout kills/reaps the child tree and
joins its writer. Old sibling UI binaries cannot be launched as helpers without
matching manifest hash and shell_visual_protocol1. Warm reference visuals avoid
redundant whole-file hash passes; freshness verification is retained. ActionRow /
KeyCaps share measured fonts, padding, strokes and paint geometry, with actual
production-font/SVG bounds and ancestor-clipping tests.

QuickLook has a separate versioned authenticated-PID ACK endpoint, never a probe
of stock's vulnerable legacy pipe. Lost ACK queries the same request/instance;
there is no blind Toggle replay. See [backend contract](quicklook-fast-backend.md).

## Operational limits and migration

Source traversal remains all-or-error at20,000 entries per root, skipping links.
The100k catalogue and1m completed-classification tests do not claim support for
1m files in a configured folder. Stable-file discovery uses two observations and
native signals; intermediate revisions and size/mtime-preserving edits may be
missed. GUI source polling cadence is about40ms; image rendering adds its own work.

MaintainCache is still explicit synchronous maintenance. Clipboard publication,
old-export cleanup and shelf-only blob cleanup remain bounded actor finalization;
this release does not claim hard real-time deadlines for these operations or for
arbitrary OS/provider load. Heavy preparation, QuickLook waits and native recycle
effects no longer take the entire interactive Service. Cache reference integrity
uses full hashes, not a size/mtime identity shortcut.

Ordered schema version2 rejects future versions before modifying the DB.
Before migration, a consistent SQLite checkpoint is saved automatically under
katran-data/backups/index-before-v2-*.sqlite3 (including committed WAL contents).
Legacy lazy tables are retained for history/compatibility, with no authority from
their unverified matches. Close all Katran processes before restoring a checkpoint;
old1.2.x code overwrites user_version and must not be used casually against the
upgraded live DB. A DB checkpoint cannot undo intentional original-file deletion.

Builds fingerprint inputs before and after compilation, verify immutable copied
executables/manifests and atomically replace the launch pointer after registration.
See [installer contract and limitations](build-install-contract.md). Owner performs
physical Windows UI/Explorer/ShareX/QuickLook acceptance. Automated API/headless
checks are evidence of their stated boundaries, not visual acceptance.

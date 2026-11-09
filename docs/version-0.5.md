# 0.5 · audit corrections and diagnostics

The owner chose 0.5.0 as the current development version. No GitHub Release or
installer is published. Use Start Katran.cmd after closing old windows manually.
Database migrations run at next startup, preserving existing events and source
files; a migration cannot reconstruct records deleted by an earlier version.

## Correct object identity

Events now use monotonically increasing IDs. Image cache keys include content
revision; stale replies cannot replace current cards. Copy requests use the
displayed IDs+revisions. Ctrl+C while capture is busy is queued with that exact
snapshot, not ignored. The UI reports queued/running/finished/error states.

Reference events record a streaming SHA-256 at ingestion; readable legacy
references are backfilled once. Preview checks source content before cache reuse;
copy verifies the final exported snapshot, including changes that preserve file
length and timestamp. Changed source returns an error: copy the current source
again to create a new arrival. Missing, deleted or corrupt objects do not become
different objects through ID reuse. A failed copy leaves the previous system
clipboard intact and displays an error; paste is ready only after copy succeeds.

## Cache lifecycle and performance

Exports are independent snapshots (no hard links), for files as well as images.
Previous paths stay valid during a pending replacement and on failure, then old
batches retire after successful publication. Existing+prepared bytes must fit the
export budget (equal to configured blob quota). Legacy exports may already exceed
it; the new copy is rejected with a clear error instead of deleting pending paste.
Older Windows clipboard history entries are not guaranteed after replacement.

Recovery reclaims only recognized unreferenced owned blobs/staging and protects
legacy reference paths. Managed-root paths cannot become new external references.
Maintenance is an explicit asynchronous action. Blob cleanup failures are shown.

Preview PNGs persist across launches with a versioned 128 MiB cache. Two bounded
workers prioritize the current visible set; obsolete queued work is discarded.
GPU textures use LRU and a memory budget, not a wholesale reset. In-flight OS
decoding is not forcibly preempted. Capture/copy/maintenance run outside the UI
thread with one writable Service, while preview clients are read-only in SQLite.
Search filters before LIMIT, uses Unicode matching and stable cursor pagination;
the shelf exposes keyboard-accessible load-more.

## Local logging

`katran-data/logs/events.jsonl` and events.1..3.jsonl contain only local application
events: keys, pointer/scroll events, typed/pasted text, commands, IDs and results.
No global keyboard hook, network upload, screenshot pixels or image byte payloads.
User-authorized text is capped per event; known GitHub/OpenAI token patterns are
masked. Logs are sensitive local diagnostics, not readable only by a particular
agent. They are excluded from Git along with all real data and runtime binaries.

A 1024-entry bounded nonblocking queue feeds a disk writer. Four files of 2 MiB
bound storage to about 8 MiB. Overflow drops records rather than blocking input;
API Status exposes cumulative accepted/dropped/write-error counters. UI never
calls the explicit blocking test/CLI flush. Serialization still has finite CPU
cost; no zero-latency claim is made.

## Structure

Wire types live in api.rs; actions/shortcuts share one definition table in
actions.rs. storage.rs owns SQL/recovery; preview.rs owns image decode/cache;
capture.rs owns the native input adapter. GUI image/palette/shelf modules are
separate. Existing public Rust reexports and API version 1 remain compatible;
CopyEventsChecked, SearchPage and MaintainCache are additive commands.

General preferences accept documented defaults for missing fields, retain strict
unknown/corrupt-field errors, and are validated before save. Tests cover migration
and restart. See development.md for actual ownership and extension recipes.

# Synthetic latency measurements and folder scheduling

Run without reading or replacing the system clipboard or touching personal files:

```powershell
cargo test --offline --release --test latency --target-dir target/build -- --ignored --nocapture
cargo test --offline --lib folder_watch --target-dir target/build
```

The report uses temporary synthetic folders and the Rust command API. It records
nearest-rank p50/p95 and maximum, with no machine-dependent CI deadline. Cold
means an empty application preview cache; it does not mean an empty OS disk cache.
The image fixture is a deterministic 1920×1080 PNG, 7,415,162 bytes. A trailing
unique payload makes the new-blob ingestion samples exercise durable publication.
These are local optimized Windows x86_64 measurements on 2026-11-10, not a
guarantee for another machine or a measurement of time until pixels are displayed.

## Baseline before native folder scheduling

| Operation | Samples | p50 ms | p95 ms |
| --- | ---: | ---: | ---: |
| Clipboard image API, new blob | 20 | 11.59 | 12.98 |
| Clipboard image API, existing blob | 30 | 6.84 | 7.94 |
| Image file visual, cold cache, edge 280 | 10 | 34.20 | 35.13 |
| Image file visual, warm cache, edge 280 | 30 | 7.36 | 7.94 |
| Image file visual, cold cache, edge 512 | 10 | 35.40 | 36.11 |
| Image file visual, warm cache, edge 512 | 30 | 8.37 | 8.91 |
| Browse page 100, history 100 | 30 | 2.83 | 3.16 |
| Browse page 100, history 1,000 | 30 | 24.93 | 30.67 |
| Unchanged ShareX scan, 100 files + 1,100 history | 30 | 151.75 | 220.65 |
| ShareX discovery alone, same 100 files | 30 | 3.14 | 3.70 |
| Downloads discovery alone, same 100 files | 30 | 2.84 | 3.09 |

Initial observation of 100 small PNG files took 6.05 ms; the subsequent first
ingestion took 184.36 ms (one sample each). Repeated scan results vary with
concurrent builds and I/O; another run reached p95 292.57 ms. The discovery
traversal itself accounts for little of the unchanged scan cost. Scans perform
per-event availability writes and classification while owning the dispatcher.

The former GUI schedule ran every two seconds and discovery required two stable
observations. A fresh screenshot therefore normally waited roughly two to four
seconds before ingestion, plus scan/dispatcher/render time. Shortening only the
timer would increase recurring I/O and dispatcher contention.

## Native folder worker contract

`src/folder_watch.rs` arms overlapped `ReadDirectoryChangesW` notifications and
does initial discovery and stable observations outside `Service`. There is one
worker per configured source (at most two): a large Downloads hash does not hold
the ShareX worker. A 50 ms observation interval checks writer stability; bursts
of native notifications do not bypass this interval. Notifications also trigger
full bounded discovery to repair rename, removal and buffer-overflow cases.
There is no silent periodic polling fallback after a watcher error.

Each candidate carries its source, canonical configured root, canonical file
path, byte count, modification time and content hash. Scanners cap traversal at
20,000 entries; candidates are hashed off the dispatcher. Newly settled files
overtake pending initial history. Delivery holds at most eight events, matching
one bounded dispatcher commit batch. Clients must drain events, prioritize
clipboard capture, commit through the domain API and refresh after a successful
commit or removal signal. They must report per-item failures.

The dispatcher must validate configured-root equality, containment, current
metadata, source mode and existing durable receipts before inserting references.
Prepared hashes are a local-worker trust boundary: their public serialization
alone does not cryptographically prove their association with a file. Existing
preview/export/deletion content checks remain necessary. A future opaque worker
receipt can tighten this boundary without moving full-file hashing back into
interactive commits.

Dropping the watcher sets a stop flag and never waits on disk/native completion
on the GUI thread. The worker owns and cancels its native I/O before releasing
overlapped buffers. A blocked filesystem read can delay that worker's eventual
exit; arbitrary unresponsive storage cannot be given a hard latency bound.

## Native measurements with a real 50 ms stability interval

| Operation, 1,100 existing history events | Samples | p50 ms | p95 ms |
| --- | ---: | ---: | ---: |
| Write → prepared, 32×32 PNG | 20 | 61.44 | 71.07 |
| Write → prepared → API commit → browse, 32×32 PNG | 20 | 98.23 | 109.98 |
| Write → prepared, 1920×1080 PNG | 20 | 65.34 | 76.16 |
| Write → prepared → API commit → browse, 1920×1080 PNG | 20 | 105.90 | 113.06 |

The 1920×1080 total maximum was 113.55 ms. The small-file total maximum was
139.44 ms. These samples remove the intentional two-to-four-second polling
delay; the remaining full-history projection prevents a consistent sub-100 ms
API round trip. The total excludes preview decode and UI presentation.

Watcher creation and all native reads run on the long-lived root worker. The
regression test deliberately returns a watcher from a short-lived setup thread:
Windows may cancel pending I/O when its issuing thread exits. Hashing checks
the stop flag between 64 KiB reads so retired workers release large local files
promptly; one blocked filesystem read still cannot be preempted safely.

## Remaining costs and measurement limits

`BrowseShelf` still projects all history, including filesystem availability,
before applying the page limit. Its roughly tenfold increase from 100 to 1,000
events is measurable. Persist classification and source membership so filtering
and pagination can happen in SQL; avoid filesystem work for unrelated history.
Pixel identity belongs in a separate bounded domain worker and persisted
classification, never synchronous browse or a long dispatcher scan.

Warm previews of referenced images still validate original content by reading
and hashing it; an application cache hit is not zero-I/O. Native Shell extraction
is provider-dependent and can hang in an installed extension. This report times
the deterministic image decoder, not arbitrary native Shell handlers. Two UI
workers bound concurrency but do not enforce a deadline on a hung extension;
process isolation would be needed for a reliable hard timeout.

The native latency test measures file write → prepared candidate → API commit →
API browse. It excludes UI frame scheduling, texture upload and on-screen
presentation. Less than 100 ms is an aspiration, not an established end-to-end
guarantee; 500 ms should not be treated as an acceptable intentional delay.

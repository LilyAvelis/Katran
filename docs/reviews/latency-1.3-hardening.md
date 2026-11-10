# Independent latency check · 2026-11-10

The new boundaries improve responsiveness, but they do not establish a universal
100 ms screenshot-to-screen guarantee. Measurements below stop at a successful
`BrowseShelf` response containing the newly committed arrival. They do not include
image decoding, GUI scheduling, texture upload, rendering, ShareX capture or its
file encoding. No personal files, Clipboard contents or desktop automation were
used. The installed 1.2.6 runtime and its launch pointer were not changed.

## Reproduction and provenance

`scripts/measure-hardening-latency.ps1` compiles a temporary headless Rust test
against already built libraries. It derives the native watcher fixture from
`tests/latency.rs`, leaving that test unchanged. Build the desired profile first;
the script prints the selected library filenames and timestamps and does not
invoke Cargo or install a build. It removes only its own fresh temporary directory.

```powershell
./scripts/measure-hardening-latency.ps1 -Profile debug -PollMilliseconds 5
./scripts/measure-hardening-latency.ps1 -Profile debug -PollMilliseconds 40
# After the integrator's release build:
./scripts/measure-hardening-latency.ps1 -Profile release -PollMilliseconds 5
```

Independent runs used Windows x86_64, debug `libkatran-f093831fcf41e5c3.rlib`,
built 2026-11-10 11:50:25 +03:00. Newer source edits require a rebuild before
remeasuring. Thirty sequential samples per fixture; percentiles are nearest-rank;
durations use `as_secs_f64() * 1000`. Cold means empty application cache, never
an assertion about the operating system's cache. Runs were sequential; concurrent
build activity and normal machine load were not controlled.

## Native file arrival

Each source is configured once and reaches `watching` before timing. Every sample
writes a new PNG to a temporary configured folder, waits for the real native
watcher and trusted source commit, then queries the Screenshots tab and checks the
exact committed event ID. There is no explicit rescan inside the timed arrival.
The existing ordinary history contains 1,100 synthetic events. PNG sizes are 217
bytes for 32×32 and 7,415,162 bytes for the deterministic 1920×1080 fixture.

| API host cadence / fixture | n | median ms | p95 ms | maximum ms |
|---|---:|---:|---:|---:|
| 5 ms, limit 8 / 32×32 | 30 | 88.265 | 109.614 | 118.321 |
| 5 ms, limit 8 / 1920×1080 | 30 | 173.959 | 214.367 | 222.834 |
| 40 ms, limit 1 / 32×32 | 30 | 104.687 | 111.711 | 113.453 |
| 40 ms, limit 1 / 1920×1080 | 30 | 246.119 | 252.919 | 253.973 |

The 40 ms host models `source_tick`'s requested cadence and commit limit, but is
not a GUI test: repaint requests can run later under actual load. The 5 ms host is
the programmatic test client, not the product's UI cadence. `folder_watch.rs`
retains a 50 ms metadata stability interval and a 10 ms worker loop. This explains
why watcher delivery itself is not the whole latency budget.

A preceding independent 5 ms run measured 89.163 / 111.259 / 117.342 ms for the
small fixture and 229.238 / 245.528 / 274.306 ms for the large fixture (median /
p95 / maximum, n=30 each). This repeat variation is material; these samples are
diagnostic measurements rather than a CI performance deadline.

## Catalogue and interactive independence

The same debug probe measured a fixed first page of 100 ordinary events, n=30:

| History count | median ms | p95 ms | maximum ms |
|---:|---:|---:|---:|
| 100 | 4.754 | 5.165 | 5.803 |
| 1,100 | 4.815 | 5.252 | 5.634 |

This is a small synthetic scale check, not proof for every search or large
catalogue. The stronger structural change is `catalogue_query`: SQL resolves
visibility, source generation, matching and cursor before `ORDER BY ... LIMIT`;
only the returned page is decoded. Substring search can still inspect candidates;
bounded returned rows do not imply constant work for every query.

`tests/job_dispatch.rs::blocked_preview_keeps_interaction_responsive_and_reports_irreversible_effect`
holds an injected external preview launcher blocked after `BeginJob`, then calls
Interaction, SetTab and Status. Its 500 ms bound is a regression assertion, not a
measured sub-100 ms promise. `tests/domain_jobs.rs::blocked_discovery_cannot_take_foreground_or_capture_workers`
proves blocked discovery cannot occupy foreground or capture lanes. These are
separate claims: the latter tests scheduler capture capacity with synthetic work,
not end-to-end native Clipboard capture during a blocked real QuickLook backend.

## Visual I/O and Shell recovery

Logical full-content hash passes for immutable reference arrivals, from code
inspection (not operating-system byte counters):

| Visual request | Before | After |
|---|---:|---:|
| Warm image | 2 | 1 |
| Warm non-image Shell/generic cache | 3 | 1 |
| Cold image or Shell extraction | 3 | 2 |

Warm still reads the entire reference once. Cold retains the second hash after
extraction/decode so a changed source cannot publish a visual for the old arrival.
The existing stale-cache and changes-during-extraction tests pass. These counts
exclude decoder reads, PNG cache reads, serialization and filesystem metadata.
Deletion and copying retain their own strict revision contracts.

Shell is now a separate process boundary. The parent enforces a 1,200 ms deadline
per native attempt, bounded request/response frames, and kill/reap. A Windows job
also kills descendants before joining the one bounded stdin writer. The helper
protocol tests repeatedly exercise a near-maximum request to a child that never
reads stdin, then a crash and a successful subsequent process. Four worker tests
and seven file-visual tests passed during implementation; library Clippy passed.
These prove recovery and resource cleanup paths, not fast real Shell handlers.
Thumbnail failure followed by icon failure can consume two deadlines; separate
image and Shell work lanes are therefore still important for image responsiveness.

Automatic sibling discovery checks a declared worker protocol and executable
digest before launch, avoiding accidental GUI startup from an old adjacent build.
Absent or invalid helpers produce explicit fallback reasons, never in-process
COM fallback. Real installed-handler latency and final screen presentation remain
the owner's manual Windows acceptance checks.

## Integrator release check

Optimized1.3.0 `cargo test --release --test latency -- --ignored --nocapture`
passed. Twenty new-file samples per size against1,100 arrivals,5ms API-host polling:

| Write→trusted commit→Browse | Median | p95 | Max |
| --- | ---: | ---: | ---: |
|32px PNG|78.974ms|114.564ms|117.014ms|
|1920×1080 PNG|85.866ms|113.065ms|122.711ms|

Warm image visual512px: median5.355ms/p955.775ms/max6.077ms. Source creation,
rendering, texture upload, actual GUI cadence and ShareX's own capture/encoding are
not included. Initial100-file discovery/commit took1,140.838ms; the0.006ms rescan
number only measures scheduling and is not enumeration latency. The debug results
above remain as measured; optimized release removes much of the large PNG cost.
These are source-arrival measurements, not new QuickLook content-ready timings.
The production fast backend has since been activated and reports acknowledged
protocol readiness; the earlier isolated title benchmarks remain separate.
